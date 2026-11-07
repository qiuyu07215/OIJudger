//! JSON API under `/api`.

use std::collections::HashMap;
use std::convert::Infallible;
use std::time::Duration;

use axum::extract::{Path, Query, State as S};
use axum::http::{header, HeaderMap, StatusCode};
use axum::response::sse::{Event as SseEvent, KeepAlive, Sse};
use axum::response::{IntoResponse, Redirect, Response};
use axum::routing::{delete, get, post};
use axum::{Json, Router};
use judge_core::model::{ContestFormat, OiFeedback, OnlineConfig};
use judge_core::{ProblemResult, Verdict};
use online::{Account, JudgingState, Notice, Phase, Role, Submission, SubmitError, Viewer};
use serde::{Deserialize, Serialize};
use tokio_stream::wrappers::BroadcastStream;
use tokio_stream::{Stream, StreamExt};

use crate::{
    clear_cookie, session_cookie, token_hash, Admin, ApiError, ApiResult, Jury, MaybeUser, OfflineJudging, Staff,
    State, User,
};

pub fn routes() -> Router<State> {
    Router::new()
        .route("/api/info", get(info))
        .route("/api/login", post(login))
        .route("/api/logout", post(logout))
        .route("/api/token-login", get(token_login))
        .route("/api/me", get(me))
        .route("/api/scoreboard", get(scoreboard))
        .route("/api/team-row", get(team_row))
        .route("/api/problemset-stats", get(problemset_stats))
        .route("/api/jury/submissions/{id}/ignore", post(ignore))
        .route("/api/events", get(events))
        .route("/api/problems/{name}/statement", get(statement))
        .route("/api/problems/{name}/files/{file}", get(statement_file))
        .route("/api/problems/{name}/samples/{file}", get(sample))
        .route("/api/submit", post(submit))
        .route("/api/submissions", get(submissions))
        .route("/api/submissions/{id}", get(submission))
        .route("/api/clarifications", get(clarifications).post(ask))
        .route("/api/announcements", get(announcements))
        .route("/api/print", get(my_prints).post(print))
        .route("/api/print/queue", get(print_queue))
        .route("/api/print/queue/{id}", post(print_done))
        .route("/api/balloons", get(balloons))
        .route("/api/balloons/{id}", post(balloon_done))
        .route("/api/jury/status", get(jury_status))
        .route("/api/jury/rejudge", post(rejudge))
        .route("/api/jury/clarifications/{id}/answer", post(answer))
        .route("/api/jury/messages", post(jury_message))
        .route("/api/jury/announcements", post(announce))
        .route("/api/jury/announcements/{id}", delete(delete_announcement))
        .route("/api/jury/freeze", post(freeze))
        .route("/api/jury/publish", post(publish))
        .route("/api/jury/reload", post(reload))
        .route("/api/jury/resolver", get(resolver))
        .route("/api/jury/judge-offline", get(offline_status).post(offline_start))
        .route("/api/jury/config", get(get_config).put(put_config))
        .route("/api/jury/accounts", get(accounts).post(create_account))
        .route("/api/jury/accounts/import", post(import_accounts))
        .route("/api/jury/accounts/{id}", axum::routing::put(update_account).delete(delete_account))
        .route("/api/jury/accounts/{id}/reset", post(reset_password))
}

// ---- views ------------------------------------------------------------------

#[derive(Serialize)]
struct AccountView {
    id: i64,
    username: String,
    display_name: String,
    group_name: String,
    seat: String,
    role: Role,
}

impl From<&Account> for AccountView {
    fn from(a: &Account) -> Self {
        AccountView {
            id: a.id,
            username: a.username.clone(),
            display_name: a.display_name.clone(),
            group_name: a.group_name.clone(),
            seat: a.seat.clone(),
            role: a.role,
        }
    }
}

#[derive(Serialize)]
struct ProblemInfo {
    name: String,
    letter: String,
    title: String,
    color: String,
    full: f64,
    time_ms: u64,
    memory_mb: u64,
    stdio: bool,
    input_file: String,
    output_file: String,
    kind: judge_core::ProblemKind,
    /// "pdf", "md", "html" or "" (no statement).
    statement: String,
    samples: Vec<String>,
}

#[derive(Serialize)]
struct Info {
    title: String,
    format: ContestFormat,
    phase: Phase,
    now_ms: i64,
    start_ms: Option<i64>,
    end_ms: Option<i64>,
    freeze_at: Option<i64>,
    frozen: bool,
    public_scoreboard: bool,
    results_published: bool,
    oi_feedback: OiFeedback,
    max_source_kb: u64,
    submit_interval_s: u64,
    freeze_min: u64,
    medals: [u32; 3],
    problems: Vec<ProblemInfo>,
    me: Option<AccountView>,
}

fn problems_visible(st: &State, acc: Option<&Account>) -> bool {
    acc.is_some_and(|a| a.role != Role::Contestant)
        || (acc.is_some() && matches!(st.contest.clock().phase(online::now_ms()), Phase::Running | Phase::Ended))
}

async fn info(S(st): S<State>, MaybeUser(acc, _): MaybeUser) -> ApiResult<Json<Info>> {
    let c = &st.contest;
    let cfg = c.config();
    let clock = c.clock();
    let now = online::now_ms();
    let contest = c.contest();
    let problems = if problems_visible(&st, acc.as_ref()) {
        let board = c.scoreboard(Viewer::Public).map(|b| b.problems).unwrap_or_default();
        contest
            .problems
            .iter()
            .zip(board)
            .map(|(p, b)| ProblemInfo {
                name: p.name.clone(),
                letter: b.letter,
                title: b.title,
                color: b.color,
                full: b.full,
                time_ms: p.subtasks.iter().map(|s| s.time_ms).max().unwrap_or(0),
                memory_mb: p.max_memory_mb(),
                stdio: p.stdio,
                input_file: if p.stdio { String::new() } else { p.input_file() },
                output_file: if p.stdio { String::new() } else { p.output_file() },
                kind: p.kind,
                statement: c
                    .statement(p)
                    .and_then(|f| f.extension().map(|e| e.to_string_lossy().to_ascii_lowercase()))
                    .map(|e| if e == "htm" { "html".into() } else if e == "markdown" { "md".into() } else { e })
                    .unwrap_or_default(),
                samples: c
                    .samples(p)
                    .iter()
                    .filter_map(|f| f.file_name().map(|n| n.to_string_lossy().into_owned()))
                    .collect(),
            })
            .collect()
    } else {
        Vec::new()
    };
    Ok(Json(Info {
        title: contest.title.clone(),
        format: cfg.format,
        phase: clock.phase(now),
        now_ms: now,
        start_ms: clock.start_ms,
        end_ms: clock.end_ms(),
        freeze_at: clock.freeze_at(),
        frozen: c.frozen(now),
        public_scoreboard: cfg.public_scoreboard,
        results_published: c.results_published(),
        oi_feedback: cfg.oi_feedback,
        max_source_kb: cfg.max_source_kb,
        submit_interval_s: cfg.submit_interval_s,
        freeze_min: cfg.freeze_min,
        medals: cfg.medals,
        problems,
        me: acc.as_ref().map(AccountView::from),
    }))
}

// ---- login --------------------------------------------------------------------

#[derive(Deserialize)]
struct LoginReq {
    username: String,
    password: String,
}

async fn login(S(st): S<State>, MaybeUser(_, ip): MaybeUser, Json(req): Json<LoginReq>) -> ApiResult<Response> {
    let now = online::now_ms();
    {
        let mut f = st.login_failures.lock();
        let e = f.entry(ip.clone()).or_insert((0, now));
        if now - e.1 > 60_000 {
            *e = (0, now);
        }
        if e.0 >= 10 {
            return Err(ApiError::new(StatusCode::TOO_MANY_REQUESTS, "too_many_attempts", "too many failed logins; wait a minute"));
        }
    }
    let found = st.contest.db.account_for_login(req.username.trim())?;
    let ok = match found {
        Some((acc, hash)) => {
            let pw = req.password.clone();
            let verified = tokio::task::spawn_blocking(move || online::accounts::verify_password(&pw, &hash))
                .await
                .unwrap_or(false);
            (verified && !acc.disabled && (acc.ip_lock.is_empty() || acc.ip_lock == ip)).then_some(acc)
        }
        None => None,
    };
    let Some(acc) = ok else {
        st.login_failures.lock().entry(ip.clone()).or_insert((0, now)).0 += 1;
        st.contest.db.log(now, req.username.trim(), "login_failed", &ip);
        return Err(ApiError::new(StatusCode::UNAUTHORIZED, "bad_login", "wrong user name or password"));
    };
    st.login_failures.lock().remove(&ip);
    let token = online::accounts::generate_token();
    st.contest.db.create_session(&token_hash(&token), acc.id, now, &ip)?;
    st.contest.db.log(now, &acc.username, "login", &ip);
    let mut res = Json(AccountView::from(&acc)).into_response();
    res.headers_mut().insert(header::SET_COOKIE, session_cookie(&token));
    Ok(res)
}

async fn logout(S(st): S<State>, headers: HeaderMap) -> ApiResult<Response> {
    if let Some(t) = crate::cookie_token(&headers) {
        st.contest.db.delete_session(&token_hash(&t))?;
    }
    let mut res = StatusCode::NO_CONTENT.into_response();
    res.headers_mut().insert(header::SET_COOKIE, clear_cookie());
    Ok(res)
}

#[derive(Deserialize)]
struct TokenQuery {
    token: String,
}

/// Single-use login link (desktop app opening the jury pages).
async fn token_login(S(st): S<State>, MaybeUser(_, ip): MaybeUser, Query(q): Query<TokenQuery>) -> ApiResult<Response> {
    let Some(id) = st.take_one_time(&q.token) else { return Ok(Redirect::to("/login").into_response()) };
    let token = online::accounts::generate_token();
    st.contest.db.create_session(&token_hash(&token), id, online::now_ms(), &ip)?;
    let mut res = Redirect::to("/jury").into_response();
    res.headers_mut().insert(header::SET_COOKIE, session_cookie(&token));
    Ok(res)
}

async fn me(User(a, _): User) -> Json<AccountView> {
    Json(AccountView::from(&a))
}

// ---- scoreboard and events -----------------------------------------------------

async fn scoreboard(S(st): S<State>, MaybeUser(acc, _): MaybeUser) -> ApiResult<Json<online::Scoreboard>> {
    let viewer = match &acc {
        Some(a) if a.role.is_jury() => Viewer::Jury,
        Some(a) => Viewer::Contestant(a.id),
        None => {
            if !st.contest.config().public_scoreboard {
                return Err(ApiError::unauthorized());
            }
            Viewer::Public
        }
    };
    Ok(Json(st.contest.scoreboard(viewer)?))
}

/// Server-sent events: notices relevant to the viewer. Clients refetch
/// what changed.
async fn events(
    S(st): S<State>,
    MaybeUser(acc, _): MaybeUser,
) -> Sse<impl Stream<Item = Result<SseEvent, Infallible>>> {
    let rx = st.events.subscribe();
    let (id, role) = acc.map(|a| (Some(a.id), Some(a.role))).unwrap_or((None, None));
    let jury = role.is_some_and(|r| r.is_jury());
    let staff = role.is_some_and(|r| r != Role::Contestant);
    let stream = BroadcastStream::new(rx).filter_map(move |n| {
        let n = n.ok()?;
        let show = match &n {
            Notice::Submission { account, .. } => jury || Some(*account) == id,
            Notice::Scoreboard | Notice::Contest => true,
            Notice::Clarification { from, to, public, .. } => {
                jury || id.is_some() && (*from == id || *to == id || *public || (from.is_none() && to.is_none()))
            }
            Notice::Announcement { .. } => id.is_some(),
            Notice::Balloon | Notice::Print => staff,
            Notice::Queue { .. } => jury,
        };
        show.then(|| Ok(SseEvent::default().json_data(&n).unwrap_or_default()))
    });
    let hello = tokio_stream::once(Ok(SseEvent::default().event("hello").data("{}")));
    let mut closing = st.closing.subscribe();
    let stop = async move {
        let _ = closing.wait_for(|c| *c).await;
    };
    Sse::new(futures_util::StreamExt::take_until(hello.chain(stream), stop)).keep_alive(KeepAlive::new().interval(Duration::from_secs(15)))
}

// ---- problem files --------------------------------------------------------------

fn problem_for(st: &State, acc: &Account, name: &str) -> ApiResult<judge_core::Problem> {
    if !problems_visible(st, Some(acc)) {
        return Err(ApiError::new(StatusCode::FORBIDDEN, "not_started", "the contest has not started"));
    }
    st.contest.problem(name).ok_or_else(ApiError::not_found)
}

fn file_response(path: &std::path::Path, download: Option<&str>) -> ApiResult<Response> {
    let data = std::fs::read(path).map_err(|_| ApiError::not_found())?;
    let ext = path.extension().map(|e| e.to_string_lossy().to_ascii_lowercase()).unwrap_or_default();
    let mime = match ext.as_str() {
        "md" | "markdown" => "text/markdown; charset=utf-8".to_string(),
        "in" | "out" | "ans" | "txt" => "text/plain; charset=utf-8".to_string(),
        _ => mime_guess::from_path(path).first_or_octet_stream().to_string(),
    };
    let mut res = ([(header::CONTENT_TYPE, mime)], data).into_response();
    if let Some(name) = download {
        if let Ok(v) = format!("attachment; filename=\"{}\"", name.replace('"', "")).parse() {
            res.headers_mut().insert(header::CONTENT_DISPOSITION, v);
        }
    }
    Ok(res)
}

async fn statement(S(st): S<State>, User(a, _): User, Path(name): Path<String>) -> ApiResult<Response> {
    let p = problem_for(&st, &a, &name)?;
    let f = st.contest.statement(&p).ok_or_else(ApiError::not_found)?;
    file_response(&f, None)
}

/// Images next to the statement (only image types, so test data next to it
/// can never be read this way).
async fn statement_file(
    S(st): S<State>,
    User(a, _): User,
    Path((name, file)): Path<(String, String)>,
) -> ApiResult<Response> {
    let p = problem_for(&st, &a, &name)?;
    let f = st.contest.statement(&p).ok_or_else(ApiError::not_found)?;
    let ok_name = !file.contains(['/', '\\', ':']) && !file.starts_with('.');
    let ext = std::path::Path::new(&file).extension().map(|e| e.to_string_lossy().to_ascii_lowercase());
    let image = matches!(ext.as_deref(), Some("png" | "jpg" | "jpeg" | "gif" | "svg" | "webp" | "bmp"));
    if !ok_name || !image {
        return Err(ApiError::not_found());
    }
    file_response(&f.parent().unwrap().join(file), None)
}

async fn sample(S(st): S<State>, User(a, _): User, Path((name, file)): Path<(String, String)>) -> ApiResult<Response> {
    let p = problem_for(&st, &a, &name)?;
    let f = st
        .contest
        .samples(&p)
        .into_iter()
        .find(|f| f.file_name().is_some_and(|n| n.to_string_lossy() == file))
        .ok_or_else(ApiError::not_found)?;
    file_response(&f, Some(&file))
}

// ---- submissions -------------------------------------------------------------------

#[derive(Deserialize)]
struct SubmitReq {
    problem: String,
    /// "cpp" or "c" (or a file name to take the extension from).
    lang: String,
    source: String,
}

async fn submit(S(st): S<State>, User(a, ip): User, Json(req): Json<SubmitReq>) -> ApiResult<Json<serde_json::Value>> {
    let ext = req.lang.rsplit('.').next().unwrap_or("cpp").to_string();
    let c = st.contest.clone();
    let r = tokio::task::spawn_blocking(move || c.submit(&a, &req.problem, &ext, req.source.as_bytes(), &ip))
        .await
        .map_err(|e| anyhow::anyhow!("{e}"))??;
    match r {
        Ok(id) => Ok(Json(serde_json::json!({ "id": id }))),
        Err(e) => {
            let msg = match &e {
                SubmitError::NotRunning => "the contest is not running".to_string(),
                SubmitError::UnknownProblem => "unknown problem".into(),
                SubmitError::UnsupportedLanguage => "only C and C++ are accepted".into(),
                SubmitError::UnsupportedProblem => "this problem cannot be submitted online".into(),
                SubmitError::TooLarge { max_kb } => format!("source is larger than {max_kb} KB"),
                SubmitError::TooSoon { wait_s } => format!("wait {wait_s} s before submitting again"),
                SubmitError::Empty => "source is empty".into(),
                SubmitError::Disabled => "account disabled".into(),
            };
            Err(ApiError::new(StatusCode::BAD_REQUEST, "submit_refused", msg).with_detail(&e))
        }
    }
}

#[derive(Serialize)]
struct SubmissionView {
    id: i64,
    account_id: i64,
    username: String,
    display_name: String,
    problem: String,
    lang: String,
    at_ms: i64,
    /// Contest time of the submission.
    contest_ms: i64,
    state: JudgingState,
    outcome: online::Outcome,
    label: String,
    score: Option<f64>,
    full: f64,
    time_ms: i64,
    ignored: bool,
}

/// What a viewer may see of a submission's result.
fn submission_view(st: &State, s: &Submission, names: &HashMap<i64, (String, String)>, jury: bool) -> SubmissionView {
    let cfg = st.contest.config();
    let clock = st.contest.clock();
    let (username, display_name) = names.get(&s.account_id).cloned().unwrap_or_default();
    let show_score = jury || cfg.format == ContestFormat::Ioi;
    SubmissionView {
        id: s.id,
        account_id: s.account_id,
        username,
        display_name,
        problem: s.problem.clone(),
        lang: s.lang.clone(),
        at_ms: s.at_ms,
        contest_ms: clock.contest_time(s.at_ms),
        state: s.state,
        outcome: s.outcome,
        label: s.label.clone(),
        score: show_score.then_some(s.score),
        full: s.full,
        time_ms: s.time_ms,
        ignored: s.ignored,
    }
}

fn account_names(st: &State) -> ApiResult<HashMap<i64, (String, String)>> {
    Ok(st
        .contest
        .db
        .accounts()?
        .into_iter()
        .map(|a| (a.id, (a.username, a.display_name)))
        .collect())
}

async fn submissions(S(st): S<State>, User(a, _): User) -> ApiResult<Json<Vec<SubmissionView>>> {
    let jury = a.role.is_jury();
    let subs = st.contest.db.submissions(if jury { None } else { Some(a.id) })?;
    let names = account_names(&st)?;
    Ok(Json(subs.iter().map(|s| submission_view(&st, s, &names, jury)).collect()))
}

#[derive(Serialize)]
struct CaseView {
    verdict: Verdict,
    time_ms: u64,
    memory_kb: u64,
    /// Jury only (checker messages may reveal expected output).
    message: String,
    input: String,
}

#[derive(Serialize)]
struct SubtaskView {
    score: f64,
    full: f64,
    cases: Vec<CaseView>,
}

#[derive(Serialize)]
struct SubmissionDetail {
    #[serde(flatten)]
    submission: SubmissionView,
    source: String,
    /// Compiler output (compile errors), judge messages for the jury.
    message: String,
    subtasks: Vec<SubtaskView>,
}

async fn submission(S(st): S<State>, User(a, _): User, Path(id): Path<i64>) -> ApiResult<Json<SubmissionDetail>> {
    let s = st.contest.db.submission(id)?.ok_or_else(ApiError::not_found)?;
    let jury = a.role.is_jury();
    if !jury && s.account_id != a.id {
        return Err(ApiError::not_found());
    }
    let names = account_names(&st)?;
    let view = submission_view(&st, &s, &names, jury);
    let source = st.contest.source(&s).unwrap_or_default();
    let detail: Option<ProblemResult> = match s.judging_id {
        Some(j) if s.state == JudgingState::Done => serde_json::from_str(&st.contest.db.judging_detail(j)?).ok(),
        _ => None,
    };
    let cfg = st.contest.config();
    let (message, subtasks) = match detail {
        None => (String::new(), Vec::new()),
        Some(r) => {
            let ce = r.status == judge_core::Status::CompileError;
            let show_cases = jury || cfg.format == ContestFormat::Ioi;
            let message = if jury || ce { r.message.clone() } else { String::new() };
            let subtasks = if show_cases {
                r.subtasks
                    .iter()
                    .map(|t| SubtaskView {
                        score: t.score,
                        full: t.full,
                        cases: t
                            .cases
                            .iter()
                            .map(|c| CaseView {
                                verdict: c.verdict,
                                time_ms: c.time_ms,
                                memory_kb: c.memory_kb,
                                message: if jury { c.message.clone() } else { String::new() },
                                input: if jury { c.input.clone() } else { String::new() },
                            })
                            .collect(),
                    })
                    .collect()
            } else {
                Vec::new()
            };
            (message, subtasks)
        }
    };
    Ok(Json(SubmissionDetail { submission: view, source, message, subtasks }))
}

// ---- clarifications and announcements --------------------------------------------------

#[derive(Serialize)]
struct ClarView {
    #[serde(flatten)]
    c: online::Clarification,
    from_name: String,
    to_name: String,
}

async fn clarifications(S(st): S<State>, User(a, _): User) -> ApiResult<Json<Vec<ClarView>>> {
    let jury = a.role.is_jury();
    let names = account_names(&st)?;
    let name = |id: Option<i64>| id.and_then(|i| names.get(&i)).map(|n| n.0.clone()).unwrap_or_default();
    let list = st.contest.db.clarifications(if jury { None } else { Some(a.id) })?;
    Ok(Json(
        list.into_iter()
            .map(|c| ClarView { from_name: name(c.from_account), to_name: name(c.to_account), c })
            .collect(),
    ))
}

#[derive(Deserialize)]
struct AskReq {
    #[serde(default)]
    problem: String,
    question: String,
}

async fn ask(S(st): S<State>, User(a, _): User, Json(req): Json<AskReq>) -> ApiResult<Json<serde_json::Value>> {
    let q = req.question.trim();
    if q.is_empty() || q.len() > 4000 {
        return Err(ApiError::bad_request("question must be 1-4000 characters"));
    }
    if !req.problem.is_empty() && req.problem != "#technical" && st.contest.problem(&req.problem).is_none() {
        return Err(ApiError::bad_request("unknown problem"));
    }
    let id = st.contest.ask(&a, &req.problem, q)?;
    Ok(Json(serde_json::json!({ "id": id })))
}

#[derive(Deserialize)]
struct AnswerReq {
    answer: String,
    #[serde(default)]
    public: bool,
}

async fn answer(S(st): S<State>, Jury(_): Jury, Path(id): Path<i64>, Json(req): Json<AnswerReq>) -> ApiResult<StatusCode> {
    st.contest.answer(id, &req.answer, req.public)?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Deserialize)]
struct MessageReq {
    /// User name; empty = everyone.
    #[serde(default)]
    to: String,
    #[serde(default)]
    problem: String,
    text: String,
}

async fn jury_message(S(st): S<State>, Jury(_): Jury, Json(req): Json<MessageReq>) -> ApiResult<StatusCode> {
    let to = if req.to.trim().is_empty() {
        None
    } else {
        Some(st.contest.db.account_for_login(req.to.trim())?.ok_or_else(|| ApiError::bad_request("unknown user"))?.0.id)
    };
    st.contest.jury_message(to, &req.problem, &req.text)?;
    Ok(StatusCode::NO_CONTENT)
}

async fn announcements(S(st): S<State>, User(_, _): User) -> ApiResult<Json<Vec<online::Announcement>>> {
    Ok(Json(st.contest.db.announcements()?))
}

#[derive(Deserialize)]
struct AnnounceReq {
    title: String,
    #[serde(default)]
    body: String,
}

async fn announce(S(st): S<State>, Jury(_): Jury, Json(req): Json<AnnounceReq>) -> ApiResult<StatusCode> {
    if req.title.trim().is_empty() {
        return Err(ApiError::bad_request("title is empty"));
    }
    st.contest.announce(&req.title, &req.body)?;
    Ok(StatusCode::NO_CONTENT)
}

async fn delete_announcement(S(st): S<State>, Jury(_): Jury, Path(id): Path<i64>) -> ApiResult<StatusCode> {
    st.contest.db.delete_announcement(id)?;
    let _ = st.events.send(Notice::Announcement { id });
    Ok(StatusCode::NO_CONTENT)
}

// ---- printing and balloons ------------------------------------------------------

#[derive(Deserialize)]
struct PrintReq {
    #[serde(default)]
    filename: String,
    content: String,
}

async fn print(S(st): S<State>, User(a, _): User, Json(req): Json<PrintReq>) -> ApiResult<Json<serde_json::Value>> {
    if req.content.trim().is_empty() || req.content.len() > 100 * 1024 {
        return Err(ApiError::bad_request("print up to 100 KB of text"));
    }
    let id = st.contest.print(&a, req.filename.trim(), &req.content)?;
    Ok(Json(serde_json::json!({ "id": id })))
}

async fn my_prints(S(st): S<State>, User(a, _): User) -> ApiResult<Json<Vec<online::PrintJob>>> {
    Ok(Json(st.contest.db.print_jobs(Some(a.id))?))
}

#[derive(Serialize)]
struct PrintView {
    #[serde(flatten)]
    job: online::PrintJob,
    username: String,
    display_name: String,
    seat: String,
}

async fn print_queue(S(st): S<State>, Staff(_): Staff) -> ApiResult<Json<Vec<PrintView>>> {
    let accs: HashMap<i64, Account> = st.contest.db.accounts()?.into_iter().map(|a| (a.id, a)).collect();
    Ok(Json(
        st.contest
            .db
            .print_jobs(None)?
            .into_iter()
            .map(|job| {
                let a = accs.get(&job.account_id);
                PrintView {
                    username: a.map(|a| a.username.clone()).unwrap_or_default(),
                    display_name: a.map(|a| a.display_name.clone()).unwrap_or_default(),
                    seat: a.map(|a| a.seat.clone()).unwrap_or_default(),
                    job,
                }
            })
            .collect(),
    ))
}

#[derive(Deserialize)]
struct DoneReq {
    done: bool,
}

async fn print_done(S(st): S<State>, Staff(_): Staff, Path(id): Path<i64>, Json(req): Json<DoneReq>) -> ApiResult<StatusCode> {
    st.contest.set_print_done(id, req.done)?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Serialize)]
struct BalloonView {
    #[serde(flatten)]
    b: online::Balloon,
    username: String,
    display_name: String,
    seat: String,
    letter: String,
    color: String,
    /// Problems this team had solved before (balloons already delivered).
    has: Vec<String>,
}

async fn balloons(S(st): S<State>, Staff(_): Staff) -> ApiResult<Json<Vec<BalloonView>>> {
    let accs: HashMap<i64, Account> = st.contest.db.accounts()?.into_iter().map(|a| (a.id, a)).collect();
    let problems: HashMap<String, (String, String)> = st
        .contest
        .scoreboard(Viewer::Jury)?
        .problems
        .into_iter()
        .map(|p| (p.name, (p.letter, p.color)))
        .collect();
    let all = st.contest.db.balloons()?;
    Ok(Json(
        all.iter()
            .map(|b| {
                let a = accs.get(&b.account_id);
                let (letter, color) = problems.get(&b.problem).cloned().unwrap_or_default();
                let has = all
                    .iter()
                    .filter(|o| o.account_id == b.account_id && o.id < b.id)
                    .filter_map(|o| problems.get(&o.problem).map(|p| p.0.clone()))
                    .collect();
                BalloonView {
                    b: b.clone(),
                    username: a.map(|a| a.username.clone()).unwrap_or_default(),
                    display_name: a.map(|a| a.display_name.clone()).unwrap_or_default(),
                    seat: a.map(|a| a.seat.clone()).unwrap_or_default(),
                    letter,
                    color,
                    has,
                }
            })
            .collect(),
    ))
}

async fn balloon_done(S(st): S<State>, Staff(_): Staff, Path(id): Path<i64>, Json(req): Json<DoneReq>) -> ApiResult<StatusCode> {
    st.contest.set_balloon_done(id, req.done)?;
    Ok(StatusCode::NO_CONTENT)
}

// ---- jury -------------------------------------------------------------------------

#[derive(Serialize)]
struct JuryStatus {
    judging_active: bool,
    queued: usize,
    running: usize,
    online: i64,
    accounts: usize,
    submissions: usize,
    unanswered: usize,
}

async fn jury_status(S(st): S<State>, Jury(_): Jury) -> ApiResult<Json<JuryStatus>> {
    let c = &st.contest;
    let (queued, running) = c.queue();
    Ok(Json(JuryStatus {
        judging_active: c.judging_active(),
        queued,
        running,
        online: c.db.online_count(online::now_ms(), 120_000)?,
        accounts: c.db.accounts()?.iter().filter(|a| a.role == Role::Contestant).count(),
        submissions: c.db.submissions(None)?.len(),
        unanswered: c.db.clarifications(None)?.iter().filter(|q| q.from_account.is_some() && q.answered_ms.is_none()).count(),
    }))
}

#[derive(Deserialize)]
struct RejudgeReq {
    #[serde(default)]
    ids: Vec<i64>,
    /// Rejudge all submissions of a problem and/or a team (user name).
    #[serde(default)]
    problem: String,
    #[serde(default)]
    team: String,
    /// Also rejudge accepted submissions (admin only, as in DOMjudge).
    #[serde(default)]
    include_correct: bool,
}

async fn rejudge(S(st): S<State>, Jury(a): Jury, Json(req): Json<RejudgeReq>) -> ApiResult<Json<serde_json::Value>> {
    if req.include_correct && a.role != Role::Admin {
        return Err(ApiError::forbidden());
    }
    let ids = if req.ids.is_empty() {
        if req.problem.is_empty() && req.team.is_empty() {
            return Err(ApiError::bad_request("nothing to rejudge"));
        }
        let team = if req.team.is_empty() {
            None
        } else {
            Some(st.contest.db.account_for_login(&req.team)?.ok_or_else(|| ApiError::bad_request("unknown team"))?.0.id)
        };
        let problem = (!req.problem.is_empty()).then_some(req.problem.as_str());
        st.contest.rejudge_ids(problem, team, req.include_correct)?
    } else {
        req.ids
    };
    let n = st.contest.rejudge(&ids, &a.username)?;
    Ok(Json(serde_json::json!({ "count": n })))
}

#[derive(Deserialize)]
struct IgnoreReq {
    ignored: bool,
}

async fn ignore(S(st): S<State>, Jury(a): Jury, Path(id): Path<i64>, Json(req): Json<IgnoreReq>) -> ApiResult<StatusCode> {
    st.contest.set_ignored(id, req.ignored, &a.username).map_err(|e| ApiError::bad_request(format!("{e:#}")))?;
    Ok(StatusCode::NO_CONTENT)
}

/// The viewer's own row (DOMjudge team page): updated during the freeze,
/// rank unknown (0) while frozen.
async fn team_row(S(st): S<State>, User(a, _): User) -> ApiResult<Json<Option<online::Scoreboard>>> {
    Ok(Json(st.contest.team_row(a.id)?))
}

#[derive(Serialize)]
struct ProblemStats {
    problem: String,
    /// (accepted, rejected) per contest interval.
    intervals: Vec<(u32, u32)>,
}

/// Problemset page: submissions per problem over the contest.
async fn problemset_stats(S(st): S<State>, User(a, _): User) -> ApiResult<Json<Vec<ProblemStats>>> {
    if !problems_visible(&st, Some(&a)) {
        return Ok(Json(Vec::new()));
    }
    let stats = st.contest.problem_stats(a.role.is_jury(), 18)?;
    Ok(Json(stats.into_iter().map(|(problem, intervals)| ProblemStats { problem, intervals }).collect()))
}

#[derive(Deserialize)]
struct FreezeReq {
    unfrozen: bool,
}

async fn freeze(S(st): S<State>, Jury(a): Jury, Json(req): Json<FreezeReq>) -> ApiResult<StatusCode> {
    st.contest.unfreeze(req.unfrozen, &a.username)?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Deserialize)]
struct PublishReq {
    published: bool,
}

async fn publish(S(st): S<State>, Jury(a): Jury, Json(req): Json<PublishReq>) -> ApiResult<StatusCode> {
    st.contest.publish_results(req.published, &a.username)?;
    Ok(StatusCode::NO_CONTENT)
}

async fn reload(S(st): S<State>, Admin(_): Admin) -> ApiResult<StatusCode> {
    let c = st.contest.clone();
    tokio::task::spawn_blocking(move || c.reload()).await.map_err(|e| anyhow::anyhow!("{e}"))??;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Serialize)]
struct ResolverData {
    board: online::Scoreboard,
    steps: Vec<online::ResolveStep>,
}

async fn resolver(S(st): S<State>, Jury(_): Jury) -> ApiResult<Json<ResolverData>> {
    let (board, steps) = st.contest.resolver()?;
    Ok(Json(ResolverData { board, steps }))
}

async fn offline_status(S(st): S<State>, Jury(_): Jury) -> Json<Option<OfflineJudging>> {
    Json(st.offline_judging.lock().clone())
}

/// OI: judge everyone's last submission after the contest.
async fn offline_start(S(st): S<State>, Jury(a): Jury) -> ApiResult<StatusCode> {
    {
        let mut o = st.offline_judging.lock();
        if o.as_ref().is_some_and(|o| o.running) {
            return Err(ApiError::bad_request("already running"));
        }
        *o = Some(OfflineJudging { running: true, ..Default::default() });
    }
    st.contest.db.log(online::now_ms(), &a.username, "judge_offline", "");
    let st2 = st.clone();
    tokio::task::spawn_blocking(move || {
        let cancel = std::sync::atomic::AtomicBool::new(false);
        let r = st2.contest.judge_offline(
            &|ev| match ev {
                judge_core::Event::Started { total, .. } => {
                    if let Some(o) = st2.offline_judging.lock().as_mut() {
                        o.total = *total;
                    }
                    let _ = st2.events.send(Notice::Contest);
                }
                judge_core::Event::TaskDone { .. } => {
                    if let Some(o) = st2.offline_judging.lock().as_mut() {
                        o.done += 1;
                    }
                    let _ = st2.events.send(Notice::Contest);
                }
                _ => {}
            },
            &cancel,
        );
        if let Some(o) = st2.offline_judging.lock().as_mut() {
            o.running = false;
            o.error = r.err().map(|e| format!("{e:#}"));
        }
        let _ = st2.events.send(Notice::Contest);
    });
    Ok(StatusCode::ACCEPTED)
}

async fn get_config(S(st): S<State>, Jury(_): Jury) -> Json<OnlineConfig> {
    Json(st.contest.config())
}

async fn put_config(S(st): S<State>, Admin(a): Admin, Json(cfg): Json<OnlineConfig>) -> ApiResult<StatusCode> {
    if !cfg.start.trim().is_empty() && online::clock::parse_local(&cfg.start).is_none() {
        return Err(ApiError::bad_request("start time must look like 2026-11-08 09:00"));
    }
    st.contest.set_config(cfg)?;
    st.contest.db.log(online::now_ms(), &a.username, "config", "");
    Ok(StatusCode::NO_CONTENT)
}

// ---- accounts (admin) ------------------------------------------------------------------

async fn accounts(S(st): S<State>, Admin(_): Admin) -> ApiResult<Json<Vec<Account>>> {
    Ok(Json(st.contest.db.accounts()?))
}

#[derive(Deserialize)]
struct AccountReq {
    #[serde(flatten)]
    account: AccountFields,
    #[serde(default)]
    password: String,
}

#[derive(Deserialize)]
struct AccountFields {
    username: String,
    #[serde(default)]
    display_name: String,
    #[serde(default)]
    group_name: String,
    #[serde(default)]
    seat: String,
    #[serde(default = "contestant")]
    role: Role,
    #[serde(default)]
    ip_lock: String,
    #[serde(default)]
    disabled: bool,
}

fn contestant() -> Role {
    Role::Contestant
}

impl AccountFields {
    fn into_account(self, id: i64) -> Account {
        Account {
            id,
            username: self.username.trim().to_string(),
            display_name: self.display_name.trim().to_string(),
            group_name: self.group_name.trim().to_string(),
            seat: self.seat.trim().to_string(),
            role: self.role,
            ip_lock: self.ip_lock.trim().to_string(),
            disabled: self.disabled,
        }
    }
}

#[derive(Serialize)]
struct Credential {
    username: String,
    display_name: String,
    seat: String,
    password: String,
}

async fn create_account(S(st): S<State>, Admin(_): Admin, Json(req): Json<AccountReq>) -> ApiResult<Json<Credential>> {
    let a = req.account.into_account(0);
    let (username, display_name, seat) = (a.username.clone(), a.display_name.clone(), a.seat.clone());
    let c = st.contest.clone();
    let pw = Some(req.password);
    let (_, password) = tokio::task::spawn_blocking(move || c.create_account(a, pw))
        .await
        .map_err(|e| anyhow::anyhow!("{e}"))?
        .map_err(|e| ApiError::bad_request(format!("{e:#}")))?;
    Ok(Json(Credential { username, display_name, seat, password }))
}

#[derive(Deserialize)]
struct ImportReq {
    csv: String,
}

async fn import_accounts(S(st): S<State>, Admin(_): Admin, Json(req): Json<ImportReq>) -> ApiResult<Json<Vec<Credential>>> {
    let list = online::accounts::parse_account_csv(&req.csv).map_err(|e| ApiError::bad_request(format!("{e:#}")))?;
    let c = st.contest.clone();
    let creds = tokio::task::spawn_blocking(move || -> anyhow::Result<Vec<Credential>> {
        let mut out = Vec::new();
        for (a, pw) in list {
            let (_, password) = c.create_account(a.clone(), pw)?;
            out.push(Credential { username: a.username, display_name: a.display_name, seat: a.seat, password });
        }
        Ok(out)
    })
    .await
    .map_err(|e| anyhow::anyhow!("{e}"))?
    .map_err(|e| ApiError::bad_request(format!("{e:#}")))?;
    Ok(Json(creds))
}

async fn update_account(
    S(st): S<State>,
    Admin(_): Admin,
    Path(id): Path<i64>,
    Json(f): Json<AccountFields>,
) -> ApiResult<StatusCode> {
    let a = f.into_account(id);
    if !online::accounts::valid_username(&a.username) {
        return Err(ApiError::bad_request("invalid user name"));
    }
    st.contest.db.update_account(&a).map_err(|e| ApiError::bad_request(format!("{e:#}")))?;
    if a.disabled {
        st.contest.db.delete_sessions_of(id)?;
    }
    let _ = st.events.send(Notice::Scoreboard);
    Ok(StatusCode::NO_CONTENT)
}

async fn delete_account(S(st): S<State>, Admin(me): Admin, Path(id): Path<i64>) -> ApiResult<StatusCode> {
    if id == me.id {
        return Err(ApiError::bad_request("cannot delete your own account"));
    }
    if st.contest.db.submissions(Some(id))?.iter().next().is_some() {
        return Err(ApiError::bad_request("account has submissions; disable it instead"));
    }
    st.contest.db.delete_account(id)?;
    let _ = st.events.send(Notice::Scoreboard);
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Deserialize)]
struct ResetReq {
    #[serde(default)]
    password: String,
}

async fn reset_password(
    S(st): S<State>,
    Admin(_): Admin,
    Path(id): Path<i64>,
    Json(req): Json<ResetReq>,
) -> ApiResult<Json<serde_json::Value>> {
    let c = st.contest.clone();
    let pw = tokio::task::spawn_blocking(move || c.reset_password(id, Some(req.password)))
        .await
        .map_err(|e| anyhow::anyhow!("{e}"))??;
    Ok(Json(serde_json::json!({ "password": pw })))
}
