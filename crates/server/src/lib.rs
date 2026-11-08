//! OIJudger online contest web server.
//!
//! Plain HTTP for a LAN / computer lab. Contestants are untrusted: every
//! endpoint checks the role and ownership, the contest clock is enforced
//! server side, and test data is never served (only declared samples).

mod api;

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;

use anyhow::{Context, Result};
use axum::body::Bytes;
use axum::extract::{ConnectInfo, FromRequestParts};
use axum::http::request::Parts;
use axum::http::{header, HeaderMap, HeaderValue, Method, StatusCode, Uri};
use axum::response::{IntoResponse, Response};
use axum::Router;
use online::{Account, Notice, Notify, OnlineContest, Role, Scoreboard};
use parking_lot::Mutex;
use serde::Serialize;
use tokio::sync::broadcast;

pub use online;

const COOKIE: &str = "oij_session";

/// Shared by all requests.
pub struct AppState {
    pub contest: Arc<OnlineContest>,
    pub events: broadcast::Sender<Notice>,
    /// Failed logins per IP: (count, window start ms).
    login_failures: Mutex<HashMap<String, (u32, i64)>>,
    /// Single-use login tokens (desktop app opening the jury pages).
    one_time: Mutex<HashMap<String, (i64, i64)>>,
    /// Running "judge after the contest" (OI).
    pub offline_judging: Mutex<Option<OfflineJudging>>,
    /// Set when the server stops: ends the open event streams, which
    /// would otherwise keep the graceful shutdown waiting forever.
    pub(crate) closing: tokio::sync::watch::Sender<bool>,
    /// Encoded scoreboards, so each board version is serialised and
    /// compressed once rather than once per page reload.
    board_bytes: Mutex<Vec<EncodedBoard>>,
}

struct EncodedBoard {
    board: Arc<Scoreboard>,
    json: Bytes,
    gzip: Option<Bytes>,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct OfflineJudging {
    pub running: bool,
    pub done: usize,
    pub total: usize,
    pub error: Option<String>,
}

pub type State = Arc<AppState>;

/// Create the broadcast channel and the notify hook to pass to
/// [`OnlineContest::open`].
pub fn channel() -> (broadcast::Sender<Notice>, Notify) {
    let (tx, _) = broadcast::channel(2048);
    let t = tx.clone();
    (
        tx,
        Arc::new(move |n| {
            let _ = t.send(n);
        }),
    )
}

impl AppState {
    pub fn new(contest: Arc<OnlineContest>, events: broadcast::Sender<Notice>) -> State {
        Arc::new(AppState {
            contest,
            events,
            login_failures: Mutex::new(HashMap::new()),
            one_time: Mutex::new(HashMap::new()),
            offline_judging: Mutex::new(None),
            closing: tokio::sync::watch::channel(false).0,
            board_bytes: Mutex::new(Vec::new()),
        })
    }

    /// `board` as JSON, gzip-compressed if `gzip`; returns whether it is.
    pub(crate) fn encoded_board(&self, board: &Arc<Scoreboard>, gzip: bool) -> Result<(Bytes, bool)> {
        use std::io::Write;
        let mut cache = self.board_bytes.lock();
        let i = match cache.iter().position(|e| Arc::ptr_eq(&e.board, board)) {
            Some(i) => i,
            None => {
                let json = Bytes::from(serde_json::to_vec(&**board)?);
                // Current public and jury boards, plus the ones just replaced.
                if cache.len() >= 4 {
                    cache.remove(0);
                }
                cache.push(EncodedBoard { board: board.clone(), json, gzip: None });
                cache.len() - 1
            }
        };
        let e = &mut cache[i];
        if !gzip {
            return Ok((e.json.clone(), false));
        }
        if e.gzip.is_none() {
            let mut enc = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
            enc.write_all(&e.json)?;
            e.gzip = Some(Bytes::from(enc.finish()?));
        }
        Ok((e.gzip.clone().unwrap_or_default(), true))
    }

    /// A single-use token that logs in as `account_id` within a minute
    /// (`/api/token-login?token=...`).
    pub fn one_time_login(&self, account_id: i64) -> String {
        let t = online::accounts::generate_token();
        let now = online::now_ms();
        let mut m = self.one_time.lock();
        m.retain(|_, (_, exp)| *exp > now);
        m.insert(t.clone(), (account_id, now + 60_000));
        t
    }

    fn take_one_time(&self, token: &str) -> Option<i64> {
        let now = online::now_ms();
        let (id, exp) = self.one_time.lock().remove(token)?;
        (exp > now).then_some(id)
    }
}

/// A running server.
pub struct ServerHandle {
    pub addr: SocketAddr,
    pub state: State,
    shutdown: Option<tokio::sync::oneshot::Sender<()>>,
    task: Option<tokio::task::JoinHandle<()>>,
}

impl ServerHandle {
    /// Stop accepting connections and wait for the server task.
    pub async fn stop(mut self) {
        let _ = self.state.closing.send(true);
        if let Some(s) = self.shutdown.take() {
            let _ = s.send(());
        }
        if let Some(mut t) = self.task.take() {
            // Requests still running after a few seconds are dropped.
            if tokio::time::timeout(std::time::Duration::from_secs(5), &mut t).await.is_err() {
                t.abort();
            }
        }
    }
}

/// Bind `addr` and serve in the background (on the current tokio runtime).
pub async fn start(state: State, addr: &str) -> Result<ServerHandle> {
    let listener = tokio::net::TcpListener::bind(addr).await.with_context(|| format!("listening on {addr}"))?;
    let local = listener.local_addr()?;
    let app = router(state.clone());
    let (tx, rx) = tokio::sync::oneshot::channel::<()>();
    let task = tokio::spawn(async move {
        let _ = axum::serve(listener, app.into_make_service_with_connect_info::<SocketAddr>())
            .with_graceful_shutdown(async move {
                let _ = rx.await;
            })
            .await;
    });
    Ok(ServerHandle { addr: local, state, shutdown: Some(tx), task: Some(task) })
}

pub fn router(state: State) -> Router {
    api::routes()
        .fallback(static_file)
        .layer(axum::middleware::from_fn(security_headers))
        .layer(axum::middleware::from_fn(csrf_guard))
        .layer(tower_http::compression::CompressionLayer::new())
        .layer(axum::extract::DefaultBodyLimit::max(2 << 20))
        .with_state(state)
}

/// Requests that change data must carry `X-Requested-With: oij`. Browsers
/// cannot send a custom header cross-origin without a CORS preflight, which
/// this server never allows, so other sites cannot forge requests.
async fn csrf_guard(req: axum::extract::Request, next: axum::middleware::Next) -> Response {
    let m = req.method();
    if m != Method::GET && m != Method::HEAD && m != Method::OPTIONS {
        let ok = req.headers().get("x-requested-with").is_some_and(|v| v.as_bytes() == b"oij");
        if !ok {
            return ApiError::new(StatusCode::FORBIDDEN, "csrf", "missing X-Requested-With header").into_response();
        }
    }
    next.run(req).await
}

async fn security_headers(req: axum::extract::Request, next: axum::middleware::Next) -> Response {
    let mut res = next.run(req).await;
    let h = res.headers_mut();
    h.insert("x-content-type-options", HeaderValue::from_static("nosniff"));
    h.insert("x-frame-options", HeaderValue::from_static("SAMEORIGIN"));
    h.insert("referrer-policy", HeaderValue::from_static("same-origin"));
    res
}

#[derive(rust_embed::RustEmbed)]
#[folder = "$CARGO_MANIFEST_DIR/../../app/web/dist"]
#[allow_missing = true]
struct WebAssets;

async fn static_file(uri: Uri) -> Response {
    let path = uri.path().trim_start_matches('/');
    if path.starts_with("api/") {
        return ApiError::new(StatusCode::NOT_FOUND, "not_found", "no such endpoint").into_response();
    }
    let (file, name) = match WebAssets::get(path) {
        Some(f) if !path.is_empty() => (f, path.to_string()),
        // Single-page app: unknown paths get index.html.
        _ => match WebAssets::get("index.html") {
            Some(f) => (f, "index.html".to_string()),
            None => return (StatusCode::NOT_FOUND, "Web pages are not built (app/web: npm run build)").into_response(),
        },
    };
    let mime = mime_guess::from_path(&name).first_or_octet_stream();
    let cache = if name.starts_with("assets/") { "public, max-age=31536000, immutable" } else { "no-cache" };
    ([(header::CONTENT_TYPE, mime.as_ref().to_string()), (header::CACHE_CONTROL, cache.to_string())], file.data)
        .into_response()
}

// ---- errors ---------------------------------------------------------------

#[derive(Debug, Serialize)]
pub struct ApiError {
    #[serde(skip)]
    status: StatusCode,
    error: String,
    message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    detail: Option<serde_json::Value>,
}

impl ApiError {
    pub fn new(status: StatusCode, error: &str, message: impl Into<String>) -> ApiError {
        ApiError { status, error: error.into(), message: message.into(), detail: None }
    }
    pub fn with_detail(mut self, d: impl Serialize) -> ApiError {
        self.detail = serde_json::to_value(d).ok();
        self
    }
    pub fn unauthorized() -> ApiError {
        ApiError::new(StatusCode::UNAUTHORIZED, "unauthorized", "please log in")
    }
    pub fn forbidden() -> ApiError {
        ApiError::new(StatusCode::FORBIDDEN, "forbidden", "not allowed")
    }
    pub fn not_found() -> ApiError {
        ApiError::new(StatusCode::NOT_FOUND, "not_found", "not found")
    }
    pub fn bad_request(msg: impl Into<String>) -> ApiError {
        ApiError::new(StatusCode::BAD_REQUEST, "bad_request", msg)
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (self.status, axum::Json(&self)).into_response()
    }
}

impl From<anyhow::Error> for ApiError {
    fn from(e: anyhow::Error) -> ApiError {
        ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, "internal", format!("{e:#}"))
    }
}

pub type ApiResult<T> = std::result::Result<T, ApiError>;

// ---- authentication ---------------------------------------------------------

fn token_hash(token: &str) -> String {
    blake3::hash(token.as_bytes()).to_hex().to_string()
}

fn cookie_token(headers: &HeaderMap) -> Option<String> {
    headers.get_all(header::COOKIE).iter().filter_map(|v| v.to_str().ok()).flat_map(|v| v.split(';')).find_map(|c| {
        let (k, v) = c.trim().split_once('=')?;
        (k == COOKIE && !v.is_empty()).then(|| v.to_string())
    })
}

fn client_ip(parts: &Parts) -> String {
    parts.extensions.get::<ConnectInfo<SocketAddr>>().map(|c| c.0.ip().to_string()).unwrap_or_default()
}

/// The logged-in account, if any.
pub struct MaybeUser(pub Option<Account>, pub String);

impl FromRequestParts<State> for MaybeUser {
    type Rejection = ApiError;
    async fn from_request_parts(parts: &mut Parts, state: &State) -> ApiResult<Self> {
        let ip = client_ip(parts);
        let Some(token) = cookie_token(&parts.headers) else { return Ok(MaybeUser(None, ip)) };
        let acc = state.contest.db.session_account(&token_hash(&token), online::now_ms())?;
        let acc = acc.filter(|a| !a.disabled && (a.ip_lock.is_empty() || a.ip_lock == ip));
        Ok(MaybeUser(acc, ip))
    }
}

/// Any logged-in account.
pub struct User(pub Account, pub String);

impl FromRequestParts<State> for User {
    type Rejection = ApiError;
    async fn from_request_parts(parts: &mut Parts, state: &State) -> ApiResult<Self> {
        match MaybeUser::from_request_parts(parts, state).await? {
            MaybeUser(Some(a), ip) => Ok(User(a, ip)),
            _ => Err(ApiError::unauthorized()),
        }
    }
}

/// Jury or admin.
pub struct Jury(pub Account);

impl FromRequestParts<State> for Jury {
    type Rejection = ApiError;
    async fn from_request_parts(parts: &mut Parts, state: &State) -> ApiResult<Self> {
        let User(a, _) = User::from_request_parts(parts, state).await?;
        if a.role.is_jury() {
            Ok(Jury(a))
        } else {
            Err(ApiError::forbidden())
        }
    }
}

/// Admin only (accounts, contest settings).
pub struct Admin(pub Account);

impl FromRequestParts<State> for Admin {
    type Rejection = ApiError;
    async fn from_request_parts(parts: &mut Parts, state: &State) -> ApiResult<Self> {
        let User(a, _) = User::from_request_parts(parts, state).await?;
        if a.role == Role::Admin {
            Ok(Admin(a))
        } else {
            Err(ApiError::forbidden())
        }
    }
}

/// Jury, admin or balloon volunteer.
pub struct Staff(pub Account);

impl FromRequestParts<State> for Staff {
    type Rejection = ApiError;
    async fn from_request_parts(parts: &mut Parts, state: &State) -> ApiResult<Self> {
        let User(a, _) = User::from_request_parts(parts, state).await?;
        if a.role != Role::Contestant {
            Ok(Staff(a))
        } else {
            Err(ApiError::forbidden())
        }
    }
}

fn session_cookie(token: &str) -> HeaderValue {
    HeaderValue::from_str(&format!("{COOKIE}={token}; Path=/; HttpOnly; SameSite=Strict; Max-Age=86400"))
        .expect("valid cookie")
}

fn clear_cookie() -> HeaderValue {
    HeaderValue::from_static("oij_session=; Path=/; HttpOnly; SameSite=Strict; Max-Age=0")
}

/// IPv4 addresses contestants can probably reach this machine at: the
/// address of the default route (no packet is sent) plus other interfaces
/// found the same way for private ranges.
pub fn lan_addresses() -> Vec<std::net::IpAddr> {
    let mut v = Vec::new();
    for probe in ["8.8.8.8:80", "10.255.255.255:1", "192.168.255.255:1", "172.31.255.255:1"] {
        if let Ok(s) = std::net::UdpSocket::bind("0.0.0.0:0") {
            if s.connect(probe).is_ok() {
                if let Ok(a) = s.local_addr() {
                    let ip = a.ip();
                    if !ip.is_loopback() && !ip.is_unspecified() && !v.contains(&ip) {
                        v.push(ip);
                    }
                }
            }
        }
    }
    v
}
