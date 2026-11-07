//! "Online contest" panel: run the web server for the open contest inside
//! the desktop app and open the jury interface in an app window.

use std::sync::Arc;

use judge_core::AppSettings;
use online::{OnlineContest, Role};
use serde::Serialize;
use tauri::{AppHandle, Manager, State, WebviewUrl, WebviewWindowBuilder};

use crate::commands::{err, AppState, CmdResult};

pub struct OnlineRuntime {
    handle: server::ServerHandle,
    contest: Arc<OnlineContest>,
    judging_error: Option<String>,
    /// Password of the admin account created on first start (shown once).
    admin_password: Option<String>,
}

impl OnlineRuntime {
    pub fn contest(&self) -> &Arc<OnlineContest> {
        &self.contest
    }
}

#[derive(Serialize, Default)]
pub struct OnlineView {
    running: bool,
    /// Address the server listens on.
    addr: String,
    /// URLs contestants can use (LAN addresses when listening on all interfaces).
    urls: Vec<String>,
    local_url: String,
    /// Configured listen address (contest.toml [online] bind).
    bind: String,
    judging: bool,
    judging_error: Option<String>,
    admin_password: Option<String>,
    queued: usize,
    running_jobs: usize,
    online: i64,
    format: String,
}

fn view(rt: Option<&OnlineRuntime>, bind: String) -> OnlineView {
    let Some(rt) = rt else { return OnlineView { bind, ..Default::default() } };
    let addr = rt.handle.addr;
    let port = addr.port();
    let urls = if addr.ip().is_unspecified() {
        server::lan_addresses().into_iter().map(|ip| format!("http://{ip}:{port}/")).collect()
    } else {
        vec![format!("http://{addr}/")]
    };
    let (queued, running_jobs) = rt.contest.queue();
    OnlineView {
        running: true,
        addr: addr.to_string(),
        urls,
        local_url: format!("http://127.0.0.1:{port}/"),
        bind,
        judging: rt.contest.judging_active(),
        judging_error: rt.judging_error.clone(),
        admin_password: rt.admin_password.clone(),
        queued,
        running_jobs,
        online: rt.contest.db.online_count(online::now_ms(), 120_000).unwrap_or(0),
        format: format!("{:?}", rt.contest.config().format).to_lowercase(),
    }
}

fn configured_bind(state: &AppState) -> String {
    state
        .project()
        .ok()
        .and_then(|p| p.contest.online.clone())
        .map(|o| o.bind)
        .unwrap_or_else(|| judge_core::model::OnlineConfig::default().bind)
}

#[tauri::command]
pub async fn online_status(state: State<'_, AppState>) -> CmdResult<OnlineView> {
    let bind = configured_bind(&state);
    Ok(view(state.online.lock().as_ref(), bind))
}

/// Start the server for the open contest (judging service included).
#[tauri::command]
pub async fn online_start(bind: Option<String>, state: State<'_, AppState>) -> CmdResult<OnlineView> {
    if state.online.lock().is_some() {
        return online_status(state).await;
    }
    let root = state.project()?.root().to_path_buf();
    let (tx, notify) = server::channel();
    let (contest, admin_password, judging_error) = tauri::async_runtime::spawn_blocking(move || {
        let contest = OnlineContest::open(&root, AppSettings::load(), notify).map_err(err)?;
        let pw = contest.ensure_admin().map_err(err)?;
        let judging_error = contest.start_judging().err().map(|e| format!("{e:#}"));
        Ok::<_, String>((contest, pw, judging_error))
    })
    .await
    .map_err(err)??;
    let bind = bind.filter(|b| !b.trim().is_empty()).unwrap_or_else(|| contest.config().bind);
    let st = server::AppState::new(contest.clone(), tx);
    let handle = match server::start(st, &bind).await {
        Ok(h) => h,
        Err(e) => {
            let c = contest.clone();
            let _ = tauri::async_runtime::spawn_blocking(move || c.stop_judging()).await;
            return Err(err(e));
        }
    };
    *state.online.lock() = Some(OnlineRuntime { handle, contest, judging_error, admin_password });
    online_status(state).await
}

#[tauri::command]
pub async fn online_stop(app: AppHandle, state: State<'_, AppState>) -> CmdResult<()> {
    let rt = state.online.lock().take();
    if let Some(rt) = rt {
        rt.handle.stop().await;
        let c = rt.contest.clone();
        let _ = tauri::async_runtime::spawn_blocking(move || c.stop_judging()).await;
    }
    if let Some(w) = app.get_webview_window("jury") {
        let _ = w.close();
    }
    Ok(())
}

/// Open the jury interface in an app window, logged in as the admin.
#[tauri::command]
pub async fn online_open_jury(app: AppHandle, state: State<'_, AppState>) -> CmdResult<()> {
    let (url, title) = {
        let guard = state.online.lock();
        let rt = guard.as_ref().ok_or("The server is not running")?;
        let admin = rt
            .contest
            .db
            .accounts()
            .map_err(err)?
            .into_iter()
            .find(|a| a.role == Role::Admin)
            .ok_or("No admin account")?;
        let token = rt.handle.state.one_time_login(admin.id);
        let url = format!("http://127.0.0.1:{}/api/token-login?token={token}", rt.handle.addr.port());
        (url, format!("{} · Jury", rt.contest.contest().title))
    };
    let url: tauri::Url = url.parse().map_err(err)?;
    if let Some(w) = app.get_webview_window("jury") {
        w.navigate(url).map_err(err)?;
        let _ = w.set_focus();
        return Ok(());
    }
    WebviewWindowBuilder::new(&app, "jury", WebviewUrl::External(url))
        .title(title)
        .inner_size(1280.0, 860.0)
        .build()
        .map_err(err)?;
    Ok(())
}
