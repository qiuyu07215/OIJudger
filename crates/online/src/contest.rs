//! An online contest: accepts submissions, judges them with the resident
//! [`JudgeService`], keeps the database and scoreboards, and notifies
//! listeners (the web server pushes these to browsers).

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Weak};

use anyhow::{Context, Result};
use judge_core::model::{ContestFormat, OiFeedback, OnlineConfig};
use judge_core::service::{Job, JudgeService, ServiceEvent};
use judge_core::{AppSettings, Contest, ContestDir, Engine, Event, Lang, Problem, ProblemKind, ProblemResult, Status, Task};
use parking_lot::{Mutex, RwLock};
use serde::Serialize;

use crate::accounts::{hash_password, valid_username};
use crate::clock::{now_ms, Clock, Phase};
use crate::db::{Account, Db, JudgingState, Role, Submission};
use crate::scoring::{self, IcpcRules, Outcome, Row, Sub};

/// Something changed; listeners decide who needs to know.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Notice {
    /// A submission was added or its judging changed.
    Submission { id: i64, account: i64 },
    Scoreboard,
    Clarification { id: i64, from: Option<i64>, to: Option<i64>, public: bool },
    Announcement { id: i64 },
    Balloon,
    Print,
    /// Contest settings, times or freeze state changed.
    Contest,
    /// Judging queue length changed.
    Queue { queued: usize, running: usize },
}

pub type Notify = Arc<dyn Fn(Notice) + Send + Sync>;

/// Why a submission was refused (shown to the contestant).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "reason", rename_all = "snake_case")]
pub enum SubmitError {
    NotRunning,
    UnknownProblem,
    /// Only C / C++ single-file submissions are accepted.
    UnsupportedLanguage,
    UnsupportedProblem,
    TooLarge { max_kb: u64 },
    TooSoon { wait_s: u64 },
    Empty,
    Disabled,
}

/// Who is looking at a scoreboard.
#[derive(Debug, Clone, Copy)]
pub enum Viewer {
    Public,
    Contestant(i64),
    Jury,
}

#[derive(Debug, Clone, Serialize)]
pub struct BoardProblem {
    pub name: String,
    pub letter: String,
    pub title: String,
    pub color: String,
    pub full: f64,
}

#[derive(Debug, Clone, Serialize)]
pub struct BoardRow {
    pub username: String,
    pub display_name: String,
    pub group_name: String,
    #[serde(flatten)]
    pub row: Row,
}

#[derive(Debug, Clone, Serialize)]
pub struct Scoreboard {
    pub format: ContestFormat,
    pub problems: Vec<BoardProblem>,
    pub rows: Vec<BoardRow>,
    /// The viewer sees the frozen board.
    pub frozen: bool,
    /// OI: scores are not shown yet.
    pub hidden: bool,
}

const STATE_UNFROZEN: &str = "unfrozen";
/// Label of submissions made after the end of the contest.
pub const TOO_LATE: &str = "TOO-LATE";
const STATE_PUBLISHED: &str = "results_published";

pub struct OnlineContest {
    pub dir: ContestDir,
    contest: RwLock<Contest>,
    pub db: Db,
    app: AppSettings,
    service: Mutex<Option<JudgeService>>,
    notify: Notify,
    me: Weak<OnlineContest>,
    /// Serialises writing submissions back into `source/`.
    write_back: Mutex<()>,
}

impl OnlineContest {
    /// Open a contest folder (with `contest.toml`) for online use.
    pub fn open(root: &Path, app: AppSettings, notify: Notify) -> Result<Arc<OnlineContest>> {
        let dir = ContestDir::new(std::path::absolute(root).unwrap_or_else(|_| root.to_path_buf()));
        let mut contest = Contest::load(&dir.root)?;
        contest.online.get_or_insert_with(OnlineConfig::default);
        let db = Db::open(&dir.online_db())?;
        Ok(Arc::new_cyclic(|me| OnlineContest {
            dir,
            contest: RwLock::new(contest),
            db,
            app,
            service: Mutex::new(None),
            notify,
            me: me.clone(),
            write_back: Mutex::new(()),
        }))
    }

    pub fn contest(&self) -> Contest {
        self.contest.read().clone()
    }

    pub fn config(&self) -> OnlineConfig {
        self.contest.read().online.clone().unwrap_or_default()
    }

    pub fn clock(&self) -> Clock {
        Clock::new(&self.config())
    }

    /// Problems offered online, in contest order.
    pub fn problems(&self) -> Vec<Problem> {
        self.contest.read().problems.clone()
    }

    pub fn problem(&self, name: &str) -> Option<Problem> {
        self.contest.read().problems.iter().find(|p| p.name == name).cloned()
    }

    /// Change the online settings (saved to `contest.toml`).
    pub fn set_config(&self, cfg: OnlineConfig) -> Result<()> {
        let mut c = self.contest.write();
        c.online = Some(cfg);
        c.save(&self.dir.root)?;
        drop(c);
        (self.notify)(Notice::Contest);
        (self.notify)(Notice::Scoreboard);
        Ok(())
    }

    /// Reload `contest.toml` (problems changed in the desktop editor) and
    /// restart judging so the new configuration is used.
    pub fn reload(&self) -> Result<()> {
        let mut contest = Contest::load(&self.dir.root)?;
        contest.online.get_or_insert_with(OnlineConfig::default);
        *self.contest.write() = contest;
        if self.service.lock().is_some() {
            self.start_judging()?;
        }
        (self.notify)(Notice::Contest);
        (self.notify)(Notice::Scoreboard);
        Ok(())
    }

    // ---- judging service ----------------------------------------------------

    /// Start (or restart) the judging service and requeue unfinished judgings.
    pub fn start_judging(&self) -> Result<()> {
        let left = self.service.lock().take().map(|s| s.shutdown(false)).unwrap_or_default();
        drop(left); // their judgings are still unfinished in the database: requeued below
        let engine = Engine::new(
            self.dir.clone(),
            self.contest(),
            self.app.judge_settings(),
            self.app.effective_compilers(),
        );
        anyhow::ensure!(
            !engine.tools.compilers.is_empty(),
            "No C/C++ compiler found. Install MinGW (g++) or set the compiler path in settings."
        );
        let me = self.me.clone();
        let service = JudgeService::start(
            engine,
            Arc::new(move |ev| {
                if let Some(c) = me.upgrade() {
                    c.on_service_event(ev);
                }
            }),
        )?;
        *self.service.lock() = Some(service);
        for (sid, jid) in self.db.unfinished()? {
            self.enqueue(sid, jid)?;
        }
        Ok(())
    }

    pub fn stop_judging(&self) {
        if let Some(s) = self.service.lock().take() {
            s.shutdown(true);
        }
    }

    pub fn judging_active(&self) -> bool {
        self.service.lock().is_some()
    }

    pub fn queue(&self) -> (usize, usize) {
        self.service.lock().as_ref().map(|s| (s.queued(), s.running())).unwrap_or((0, 0))
    }

    fn submission_root(&self, sid: i64) -> PathBuf {
        self.dir.submissions().join(sid.to_string())
    }

    fn enqueue(&self, sid: i64, jid: i64) -> Result<()> {
        let Some(sub) = self.db.submission(sid)? else { return Ok(()) };
        let Some(acc) = self.db.account(sub.account_id)? else { return Ok(()) };
        let cfg = self.config();
        if cfg.format == ContestFormat::Oi && cfg.oi_feedback == OiFeedback::None {
            // Nothing to tell the contestant during an OI contest.
            let now = now_ms();
            self.db.judging_done(jid, JudgingState::Done, Outcome::Pending, "", 0.0, 0.0, &[], 0, "", now)?;
            (self.notify)(Notice::Submission { id: sid, account: acc.id });
            return Ok(());
        }
        let job = Job {
            id: jid,
            contestant: acc.username.clone(),
            problem: sub.problem.clone(),
            source_root: self.submission_root(sid),
            stop_on_failure: cfg.format == ContestFormat::Icpc,
            compile_only: cfg.format == ContestFormat::Oi,
        };
        match self.service.lock().as_ref() {
            Some(s) => s.submit(job),
            None => return Ok(()), // judged when the service starts
        }
        self.notify_queue();
        Ok(())
    }

    fn notify_queue(&self) {
        let (queued, running) = self.queue();
        (self.notify)(Notice::Queue { queued, running });
    }

    fn on_service_event(&self, ev: ServiceEvent) {
        let now = now_ms();
        match ev {
            ServiceEvent::Started { id } => {
                let _ = self.db.judging_started(id, now);
                if let Ok(Some(sid)) = self.db.valid_judging_submission(id) {
                    if let Ok(Some(s)) = self.db.submission(sid) {
                        (self.notify)(Notice::Submission { id: sid, account: s.account_id });
                    }
                }
                self.notify_queue();
            }
            ServiceEvent::Done { id, result } => {
                if let Err(e) = self.finish_judging(id, result, now) {
                    self.db.log(now, "system", "error", &format!("judging {id}: {e:#}"));
                }
                self.notify_queue();
            }
            ServiceEvent::Case { .. } => {}
            ServiceEvent::Message(m) => self.db.log(now, "system", "judge", &m),
        }
    }

    fn finish_judging(&self, jid: i64, result: Option<ProblemResult>, now: i64) -> Result<()> {
        let Some(sid) = self.db.valid_judging_submission(jid)? else { return Ok(()) }; // superseded by a rejudge
        let Some(sub) = self.db.submission(sid)? else { return Ok(()) };
        let Some(r) = result else {
            self.db.judging_done(jid, JudgingState::Cancelled, Outcome::Pending, "", 0.0, 0.0, &[], 0, "", now)?;
            return Ok(());
        };
        let cfg = self.config();
        let compile_only = cfg.format == ContestFormat::Oi;
        let outcome = match r.status {
            Status::CompileError => Outcome::CompileError,
            Status::SystemError | Status::NoSource => Outcome::Error,
            Status::Judged if compile_only => Outcome::Pending,
            Status::Judged if r.full > 0.0 && r.score >= r.full - 1e-9 => Outcome::Accepted,
            Status::Judged => Outcome::Rejected,
        };
        let label = if compile_only && r.status == Status::Judged { "OK" } else { r.label() };
        let subtask_scores: Vec<f64> = r.subtasks.iter().map(|s| s.score).collect();
        let detail = serde_json::to_string(&r)?;
        self.db.judging_done(
            jid,
            JudgingState::Done,
            outcome,
            label,
            r.score,
            r.full,
            &subtask_scores,
            r.time_ms as i64,
            &detail,
            now,
        )?;
        if outcome == Outcome::Accepted && cfg.format != ContestFormat::Oi {
            let first = !self.db.balloons()?.iter().any(|b| b.problem == sub.problem);
            if self.db.add_balloon(sub.account_id, &sub.problem, sid, now, first)?.is_some() {
                (self.notify)(Notice::Balloon);
            }
        }
        if cfg.format == ContestFormat::Ioi {
            self.write_back_best(&sub)?;
        }
        (self.notify)(Notice::Submission { id: sid, account: sub.account_id });
        (self.notify)(Notice::Scoreboard);
        Ok(())
    }

    // ---- submissions ------------------------------------------------------

    /// Accept a submission from `account`. `ext` is the source file
    /// extension ("cpp", "c", ...). Returns the submission id.
    pub fn submit(
        &self,
        account: &Account,
        problem: &str,
        ext: &str,
        source: &[u8],
        ip: &str,
    ) -> Result<std::result::Result<i64, SubmitError>> {
        let now = now_ms();
        let cfg = self.config();
        let jury = account.role.is_jury();
        if account.disabled {
            return Ok(Err(SubmitError::Disabled));
        }
        let phase = self.clock().phase(now);
        // As in DOMjudge: after the end a submission is stored as TOO-LATE
        // and not judged; before the start it is refused.
        let too_late = !jury && phase == Phase::Ended;
        if !jury && !too_late && phase != Phase::Running {
            return Ok(Err(SubmitError::NotRunning));
        }
        let Some(p) = self.problem(problem) else { return Ok(Err(SubmitError::UnknownProblem)) };
        let ext = ext.trim_start_matches('.').to_ascii_lowercase();
        if Lang::from_path(Path::new(&format!("x.{ext}"))).is_none() {
            return Ok(Err(SubmitError::UnsupportedLanguage));
        }
        if matches!(p.kind, ProblemKind::Answer | ProblemKind::Communication) {
            return Ok(Err(SubmitError::UnsupportedProblem));
        }
        if source.iter().all(|b| b.is_ascii_whitespace()) {
            return Ok(Err(SubmitError::Empty));
        }
        if source.len() as u64 > cfg.max_source_kb * 1024 {
            return Ok(Err(SubmitError::TooLarge { max_kb: cfg.max_source_kb }));
        }
        if !jury {
            if let Some(last) = self.db.last_submission_ms(account.id)? {
                let wait = cfg.submit_interval_s as i64 * 1000 - (now - last);
                if wait > 0 {
                    return Ok(Err(SubmitError::TooSoon { wait_s: (wait as u64).div_ceil(1000) }));
                }
            }
        }
        anyhow::ensure!(valid_username(&account.username), "invalid user name");
        let filename = format!("{}.{ext}", p.source_name());
        let lang = if ext == "c" { "c" } else { "cpp" };
        let (sid, jid) =
            self.db.add_submission(account.id, &p.name, lang, &filename, source.len() as i64, now, ip)?;
        let dir = self.submission_root(sid).join(&account.username).join(&p.name);
        std::fs::create_dir_all(&dir)?;
        std::fs::write(dir.join(&filename), source).context("storing submission")?;
        self.db.log(now, &account.username, "submit", &format!("#{sid} {}", p.name));
        if too_late {
            self.db.judging_done(jid, JudgingState::Done, Outcome::Pending, TOO_LATE, 0.0, 0.0, &[], 0, "", now)?;
            (self.notify)(Notice::Submission { id: sid, account: account.id });
            return Ok(Ok(sid));
        }
        if cfg.format != ContestFormat::Ioi && !jury {
            self.write_back(&account.username, &p, &dir.join(&filename))?;
        }
        (self.notify)(Notice::Submission { id: sid, account: account.id });
        self.enqueue(sid, jid)?;
        (self.notify)(Notice::Scoreboard);
        Ok(Ok(sid))
    }

    /// Source text of a submission.
    pub fn source(&self, sub: &Submission) -> Result<String> {
        let acc = self.db.account(sub.account_id)?.context("account not found")?;
        let p = self.submission_root(sub.id).join(&acc.username).join(&sub.problem).join(&sub.filename);
        Ok(String::from_utf8_lossy(&std::fs::read(&p).with_context(|| format!("reading {}", p.display()))?).into_owned())
    }

    /// Copy a submission into `source/<user>/<problem>/`, replacing the
    /// previous one (also with another extension).
    fn write_back(&self, username: &str, p: &Problem, file: &Path) -> Result<()> {
        let _g = self.write_back.lock();
        let dir = self.dir.contest_source().join(username).join(&p.name);
        std::fs::create_dir_all(&dir)?;
        for ext in Lang::SOURCE_EXTS {
            let old = dir.join(format!("{}.{ext}", p.source_name()));
            if old.is_file() {
                std::fs::remove_file(&old)?;
            }
        }
        std::fs::copy(file, dir.join(file.file_name().unwrap())).context("writing submission into source/")?;
        Ok(())
    }

    /// IOI: keep the best-scoring submission in `source/`.
    fn write_back_best(&self, sub: &Submission) -> Result<()> {
        let acc = self.db.account(sub.account_id)?.context("account")?;
        if acc.role.is_jury() {
            return Ok(());
        }
        let Some(p) = self.problem(&sub.problem) else { return Ok(()) };
        let best = self
            .db
            .submissions(Some(sub.account_id))?
            .into_iter()
            .filter(|s| s.problem == sub.problem && s.state == JudgingState::Done)
            .max_by(|a, b| a.score.partial_cmp(&b.score).unwrap_or(std::cmp::Ordering::Equal).then(b.id.cmp(&a.id)));
        if let Some(best) = best {
            let file = self.submission_root(best.id).join(&acc.username).join(&best.problem).join(&best.filename);
            self.write_back(&acc.username, &p, &file)?;
        }
        Ok(())
    }

    /// Judge submissions again (jury).
    pub fn rejudge(&self, ids: &[i64], actor: &str) -> Result<usize> {
        let mut n = 0;
        for &sid in ids {
            if self.db.submission(sid)?.is_none() {
                continue;
            }
            let jid = self.db.new_judging(sid)?;
            self.enqueue(sid, jid)?;
            n += 1;
        }
        self.db.log(now_ms(), actor, "rejudge", &format!("{n} submission(s)"));
        (self.notify)(Notice::Scoreboard);
        Ok(n)
    }

    // ---- scoreboard -------------------------------------------------------

    /// Is the scoreboard currently frozen for non-jury viewers?
    pub fn frozen(&self, now: i64) -> bool {
        let clock = self.clock();
        clock.freeze_at().is_some_and(|f| now >= f) && !self.state_flag(STATE_UNFROZEN)
    }

    pub fn unfreeze(&self, unfrozen: bool, actor: &str) -> Result<()> {
        self.db.set_state(STATE_UNFROZEN, if unfrozen { "1" } else { "0" })?;
        self.db.log(now_ms(), actor, "freeze", if unfrozen { "unfrozen" } else { "frozen" });
        (self.notify)(Notice::Contest);
        (self.notify)(Notice::Scoreboard);
        Ok(())
    }

    pub fn results_published(&self) -> bool {
        self.state_flag(STATE_PUBLISHED)
    }

    /// OI: show the final scores to contestants and on the public board.
    pub fn publish_results(&self, published: bool, actor: &str) -> Result<()> {
        self.db.set_state(STATE_PUBLISHED, if published { "1" } else { "0" })?;
        self.db.log(now_ms(), actor, "publish", &published.to_string());
        (self.notify)(Notice::Contest);
        (self.notify)(Notice::Scoreboard);
        Ok(())
    }

    fn state_flag(&self, key: &str) -> bool {
        self.db.get_state(key).ok().flatten().is_some_and(|v| v == "1")
    }

    fn board_problems(&self) -> Vec<BoardProblem> {
        self.problems()
            .iter()
            .enumerate()
            .map(|(i, p)| BoardProblem {
                name: p.name.clone(),
                letter: if p.letter.is_empty() { letter(i) } else { p.letter.clone() },
                title: p.display_title().to_string(),
                color: p.color.clone(),
                full: p.full_score(),
            })
            .collect()
    }

    /// Contestant accounts shown on the scoreboard.
    fn board_accounts(&self) -> Result<Vec<Account>> {
        Ok(self.db.accounts()?.into_iter().filter(|a| a.role == Role::Contestant && !a.disabled).collect())
    }

    /// Submissions as seen by `viewer` for scoring (`hidden` = frozen for them).
    fn scoring_subs(&self, accounts: &[Account], hide_frozen: bool) -> Result<Vec<Sub>> {
        self.scoring_subs_for(accounts, hide_frozen, None)
    }

    /// Like [`Self::scoring_subs`], but `except`'s own submissions are never hidden.
    fn scoring_subs_for(&self, accounts: &[Account], hide_frozen: bool, except: Option<i64>) -> Result<Vec<Sub>> {
        let clock = self.clock();
        let ids: std::collections::HashSet<i64> = accounts.iter().map(|a| a.id).collect();
        Ok(self
            .db
            .submissions(None)?
            .into_iter()
            .filter(|s| ids.contains(&s.account_id) && !s.ignored && s.label != TOO_LATE)
            .map(|s| Sub {
                id: s.id,
                account: s.account_id,
                time_ms: clock.contest_time(s.at_ms),
                outcome: if s.state == JudgingState::Done { s.outcome } else { Outcome::Pending },
                score: s.score,
                subtask_scores: s.subtask_scores.clone(),
                hidden: hide_frozen && Some(s.account_id) != except && clock.is_after_freeze(s.at_ms),
                problem: s.problem,
            })
            .collect())
    }

    fn icpc_rules(cfg: &OnlineConfig) -> IcpcRules {
        IcpcRules { penalty_min: cfg.penalty_min as i64, compile_error_penalty: cfg.compile_error_penalty }
    }

    pub fn scoreboard(&self, viewer: Viewer) -> Result<Scoreboard> {
        let now = now_ms();
        let cfg = self.config();
        let accounts = self.board_accounts()?;
        let ids: Vec<i64> = accounts.iter().map(|a| a.id).collect();
        let problems = self.board_problems();
        let names: Vec<String> = problems.iter().map(|p| p.name.clone()).collect();
        let jury = matches!(viewer, Viewer::Jury);
        let frozen = !jury && self.frozen(now);
        let mut hidden = false;
        let rows = match cfg.format {
            ContestFormat::Icpc => scoring::icpc_board(&ids, &names, &self.scoring_subs(&accounts, frozen)?, &Self::icpc_rules(&cfg)),
            ContestFormat::Ioi => scoring::ioi_board(&ids, &names, &self.scoring_subs(&accounts, frozen)?, cfg.ioi_scoring),
            ContestFormat::Oi => {
                if jury || self.results_published() {
                    let store = store::Store::open(&self.dir.results_db())?;
                    let by_name: HashMap<&str, i64> = accounts.iter().map(|a| (a.username.as_str(), a.id)).collect();
                    let scores: HashMap<(i64, String), f64> = store
                        .summaries()?
                        .into_iter()
                        .filter_map(|s| Some(((*by_name.get(s.contestant.as_str())?, s.problem), s.score)))
                        .collect();
                    scoring::score_board(&ids, &names, &scores)
                } else {
                    hidden = true;
                    scoring::score_board(&ids, &names, &HashMap::new())
                }
            }
        };
        let by_id: HashMap<i64, &Account> = accounts.iter().map(|a| (a.id, a)).collect();
        let rows = rows
            .into_iter()
            .filter_map(|row| {
                let a = by_id.get(&row.account)?;
                Some(BoardRow {
                    username: a.username.clone(),
                    display_name: if a.display_name.is_empty() { a.username.clone() } else { a.display_name.clone() },
                    group_name: a.group_name.clone(),
                    row,
                })
            })
            .collect();
        Ok(Scoreboard { format: cfg.format, problems, rows, frozen, hidden })
    }

    /// The viewer's own scoreboard row as on DOMjudge's team page: during
    /// the freeze it keeps updating with the team's own results, and its
    /// rank is unknown (0, shown as "?").
    pub fn team_row(&self, account: i64) -> Result<Option<Scoreboard>> {
        let now = now_ms();
        let cfg = self.config();
        let accounts = self.board_accounts()?;
        if !accounts.iter().any(|a| a.id == account) {
            return Ok(None);
        }
        let frozen = self.frozen(now);
        if cfg.format == ContestFormat::Oi {
            let mut b = self.scoreboard(Viewer::Contestant(account))?;
            b.rows.retain(|r| r.row.account == account);
            return Ok(Some(b));
        }
        let ids: Vec<i64> = accounts.iter().map(|a| a.id).collect();
        let problems = self.board_problems();
        let names: Vec<String> = problems.iter().map(|p| p.name.clone()).collect();
        let subs = self.scoring_subs_for(&accounts, frozen, Some(account))?;
        let rows = match cfg.format {
            ContestFormat::Ioi => scoring::ioi_board(&ids, &names, &subs, cfg.ioi_scoring),
            _ => scoring::icpc_board(&ids, &names, &subs, &Self::icpc_rules(&cfg)),
        };
        let a = accounts.iter().find(|a| a.id == account).unwrap();
        let rows = rows
            .into_iter()
            .filter(|r| r.account == account)
            .map(|mut row| {
                if frozen {
                    row.rank = 0;
                }
                BoardRow {
                    username: a.username.clone(),
                    display_name: if a.display_name.is_empty() { a.username.clone() } else { a.display_name.clone() },
                    group_name: a.group_name.clone(),
                    row,
                }
            })
            .collect();
        Ok(Some(Scoreboard { format: cfg.format, problems, rows, frozen, hidden: false }))
    }

    /// Submission statistics per problem and contest interval for the
    /// problemset page: (accepted, rejected) counts, frozen ones excluded
    /// for non-jury viewers.
    pub fn problem_stats(&self, jury: bool, intervals: usize) -> Result<Vec<(String, Vec<(u32, u32)>)>> {
        let clock = self.clock();
        let accounts = self.board_accounts()?;
        let subs = self.scoring_subs(&accounts, !jury && self.frozen(now_ms()))?;
        let total = clock.duration_ms.max(1);
        let mut out: Vec<(String, Vec<(u32, u32)>)> =
            self.problems().iter().map(|p| (p.name.clone(), vec![(0, 0); intervals])).collect();
        for s in subs {
            if s.hidden {
                continue;
            }
            let Some(e) = out.iter_mut().find(|(n, _)| *n == s.problem) else { continue };
            let k = ((s.time_ms * intervals as i64) / total).clamp(0, intervals as i64 - 1) as usize;
            match s.outcome {
                Outcome::Accepted => e.1[k].0 += 1,
                Outcome::Rejected | Outcome::CompileError => e.1[k].1 += 1,
                _ => {}
            }
        }
        Ok(out)
    }

    /// Ignore a submission (or undo): it no longer counts anywhere.
    pub fn set_ignored(&self, id: i64, ignored: bool, actor: &str) -> Result<()> {
        let sub = self.db.submission(id)?.context("no such submission")?;
        self.db.set_ignored(id, ignored)?;
        self.db.log(now_ms(), actor, if ignored { "ignore" } else { "unignore" }, &format!("#{id}"));
        (self.notify)(Notice::Submission { id, account: sub.account_id });
        (self.notify)(Notice::Scoreboard);
        Ok(())
    }

    /// Submissions to rejudge for a problem and/or team. Accepted ones are
    /// skipped unless `include_correct` (as in DOMjudge); ignored and
    /// too-late ones always.
    pub fn rejudge_ids(&self, problem: Option<&str>, account: Option<i64>, include_correct: bool) -> Result<Vec<i64>> {
        Ok(self
            .db
            .submissions(account)?
            .into_iter()
            .filter(|s| problem.is_none_or(|p| s.problem == p))
            .filter(|s| !s.ignored && s.label != TOO_LATE)
            .filter(|s| include_correct || s.outcome != Outcome::Accepted)
            .map(|s| s.id)
            .collect())
    }

    /// ICPC resolver data (jury): the frozen board and the reveal steps.
    pub fn resolver(&self) -> Result<(Scoreboard, Vec<scoring::ResolveStep>)> {
        let cfg = self.config();
        let accounts = self.board_accounts()?;
        let ids: Vec<i64> = accounts.iter().map(|a| a.id).collect();
        let problems = self.board_problems();
        let names: Vec<String> = problems.iter().map(|p| p.name.clone()).collect();
        let subs = self.scoring_subs(&accounts, true)?;
        let (rows, steps) = scoring::resolve(&ids, &names, &subs, &Self::icpc_rules(&cfg));
        let by_id: HashMap<i64, &Account> = accounts.iter().map(|a| (a.id, a)).collect();
        let rows = rows
            .into_iter()
            .filter_map(|row| {
                let a = by_id.get(&row.account)?;
                Some(BoardRow {
                    username: a.username.clone(),
                    display_name: if a.display_name.is_empty() { a.username.clone() } else { a.display_name.clone() },
                    group_name: a.group_name.clone(),
                    row,
                })
            })
            .collect();
        Ok((Scoreboard { format: cfg.format, problems, rows, frozen: true, hidden: false }, steps))
    }

    // ---- OI: judge after the contest --------------------------------------

    /// Judge every contestant's last submission (already in `source/`) with
    /// the offline engine; results go to `.oij/results.db`.
    pub fn judge_offline(&self, sink: &(dyn Fn(&Event) + Sync), cancel: &AtomicBool) -> Result<usize> {
        let contest = self.contest();
        let engine = Engine::new(self.dir.clone(), contest.clone(), self.app.judge_settings(), self.app.effective_compilers());
        let accounts = self.board_accounts()?;
        let mut tasks = Vec::new();
        for a in &accounts {
            for p in &contest.problems {
                tasks.push(Task { contestant: a.username.clone(), problem: p.name.clone() });
            }
        }
        let n = tasks.len();
        let store = Mutex::new(store::Store::open(&self.dir.results_db())?);
        engine.run(
            tasks,
            &|ev: Event| {
                if let Event::TaskDone { result, .. } = &ev {
                    let _ = store.lock().save(result);
                }
                sink(&ev);
            },
            cancel,
        )?;
        (self.notify)(Notice::Scoreboard);
        Ok(n)
    }

    // ---- accounts ---------------------------------------------------------

    /// Create an account with the given password (or a generated one).
    /// Returns the password.
    pub fn create_account(&self, mut a: Account, password: Option<String>) -> Result<(i64, String)> {
        anyhow::ensure!(valid_username(&a.username), "invalid user name {:?} (use letters, digits, - _ .)", a.username);
        let password = password.filter(|p| !p.is_empty()).unwrap_or_else(crate::accounts::generate_password);
        a.id = 0;
        let id = self.db.create_account(&a, &hash_password(&password)?)?;
        (self.notify)(Notice::Scoreboard);
        Ok((id, password))
    }

    /// Set a new (generated) password; ends the account's sessions.
    pub fn reset_password(&self, id: i64, password: Option<String>) -> Result<String> {
        let password = password.filter(|p| !p.is_empty()).unwrap_or_else(crate::accounts::generate_password);
        self.db.set_password_hash(id, &hash_password(&password)?)?;
        self.db.delete_sessions_of(id)?;
        Ok(password)
    }

    /// Make sure an admin account exists; returns its new password if one
    /// was created.
    pub fn ensure_admin(&self) -> Result<Option<String>> {
        if self.db.accounts()?.iter().any(|a| a.role == Role::Admin) {
            return Ok(None);
        }
        let a = Account {
            id: 0,
            username: "admin".into(),
            display_name: "Administrator".into(),
            group_name: String::new(),
            seat: String::new(),
            role: Role::Admin,
            ip_lock: String::new(),
            disabled: false,
        };
        Ok(Some(self.create_account(a, None)?.1))
    }

    // ---- clarifications, announcements, balloons, printing -----------------

    pub fn ask(&self, account: &Account, problem: &str, question: &str) -> Result<i64> {
        let id = self.db.add_clarification(Some(account.id), None, problem, question.trim(), now_ms())?;
        (self.notify)(Notice::Clarification { id, from: Some(account.id), to: None, public: false });
        Ok(id)
    }

    pub fn answer(&self, id: i64, answer: &str, public: bool) -> Result<()> {
        self.db.answer_clarification(id, answer.trim(), public, now_ms())?;
        let c = self.db.clarifications(None)?.into_iter().find(|c| c.id == id);
        let from = c.as_ref().and_then(|c| c.from_account);
        (self.notify)(Notice::Clarification { id, from, to: from, public });
        Ok(())
    }

    /// Message from the jury to one contestant (or everyone with `to = None`).
    pub fn jury_message(&self, to: Option<i64>, problem: &str, text: &str) -> Result<i64> {
        let now = now_ms();
        let id = self.db.add_clarification(None, to, problem, "", now)?;
        self.db.answer_clarification(id, text.trim(), to.is_none(), now)?;
        (self.notify)(Notice::Clarification { id, from: None, to, public: to.is_none() });
        Ok(id)
    }

    pub fn announce(&self, title: &str, body: &str) -> Result<i64> {
        let id = self.db.add_announcement(title.trim(), body.trim(), now_ms())?;
        (self.notify)(Notice::Announcement { id });
        Ok(id)
    }

    pub fn set_balloon_done(&self, id: i64, done: bool) -> Result<()> {
        self.db.set_balloon_done(id, done)?;
        (self.notify)(Notice::Balloon);
        Ok(())
    }

    pub fn print(&self, account: &Account, filename: &str, content: &str) -> Result<i64> {
        let id = self.db.add_print_job(account.id, filename, content, now_ms())?;
        (self.notify)(Notice::Print);
        Ok(id)
    }

    pub fn set_print_done(&self, id: i64, done: bool) -> Result<()> {
        self.db.set_print_done(id, done)?;
        (self.notify)(Notice::Print);
        Ok(())
    }

    /// Sample files of a problem that contestants may download.
    pub fn samples(&self, p: &Problem) -> Vec<PathBuf> {
        let data = self.dir.data();
        if !p.samples.is_empty() {
            return p.samples.iter().map(|s| data.join(s)).filter(|f| f.is_file() && f.starts_with(&data)).collect();
        }
        let mut v: Vec<PathBuf> = std::fs::read_dir(data.join(&p.name))
            .map(|rd| {
                rd.flatten()
                    .map(|e| e.path())
                    .filter(|f| {
                        f.is_file()
                            && f.file_name()
                                .and_then(|n| n.to_str())
                                .is_some_and(|n| n.to_ascii_lowercase().starts_with("sample"))
                    })
                    .collect()
            })
            .unwrap_or_default();
        v.sort();
        v
    }

    /// Statement file of a problem, if configured and inside `data/`.
    pub fn statement(&self, p: &Problem) -> Option<PathBuf> {
        let data = self.dir.data();
        let rel = if p.statement.is_empty() {
            ["statement.pdf", "statement.md", "statement.html"].iter().map(|f| format!("{}/{f}", p.name)).find(|r| data.join(r).is_file())?
        } else {
            p.statement.clone()
        };
        let f = data.join(rel);
        (f.is_file() && f.starts_with(&data) && !rel_has_parent(&f, &data)).then_some(f)
    }
}

fn rel_has_parent(f: &Path, base: &Path) -> bool {
    f.strip_prefix(base).map(|r| r.components().any(|c| matches!(c, std::path::Component::ParentDir))).unwrap_or(true)
}

/// "A", "B", ..., "Z", "AA", ...
pub fn letter(i: usize) -> String {
    let mut s = String::new();
    let mut n = i + 1;
    while n > 0 {
        n -= 1;
        s.insert(0, (b'A' + (n % 26) as u8) as char);
        n /= 26;
    }
    s
}

impl Drop for OnlineContest {
    fn drop(&mut self) {
        if let Some(s) = self.service.get_mut().take() {
            s.shutdown(true);
        }
    }
}
