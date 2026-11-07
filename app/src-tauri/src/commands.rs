//! Tauri commands called by the UI.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use judge_core::{AppSettings, Contest, Lang, Problem, ProblemResult};
use oij::project::{Project, Selection};
use parking_lot::{Mutex, RwLock};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, State};

use crate::progress::{Progress, ProgressView};

type CmdResult<T> = Result<T, String>;

fn err(e: impl std::fmt::Display) -> String {
    format!("{e:#}")
}

#[derive(Default)]
pub struct AppState {
    project: RwLock<Option<Arc<Project>>>,
    judging: Mutex<Option<Arc<AtomicBool>>>,
    progress: Arc<Mutex<Option<Progress>>>,
}

impl AppState {
    fn project(&self) -> CmdResult<Arc<Project>> {
        self.project.read().clone().ok_or_else(|| "No contest is open".to_string())
    }
    fn is_judging(&self) -> bool {
        self.judging.lock().is_some()
    }
}

/// Run blocking work off the UI thread.
async fn blocking<T: Send + 'static>(f: impl FnOnce() -> CmdResult<T> + Send + 'static) -> CmdResult<T> {
    tauri::async_runtime::spawn_blocking(f).await.map_err(err)?
}

#[derive(Serialize)]
pub struct AppInfo {
    version: String,
    settings_path: String,
    cpu_cores: usize,
    default_workers: usize,
}

#[tauri::command]
pub async fn app_info() -> AppInfo {
    AppInfo {
        version: env!("CARGO_PKG_VERSION").into(),
        settings_path: AppSettings::path().display().to_string(),
        cpu_cores: sandbox::cpu_cores().len(),
        default_workers: judge_core::workspace::default_worker_count(),
    }
}

#[tauri::command]
pub async fn get_settings() -> AppSettings {
    AppSettings::load()
}

#[tauri::command]
pub async fn save_settings(settings: AppSettings) -> CmdResult<()> {
    settings.save().map_err(err)
}

#[derive(Serialize)]
pub struct CompilerInfo {
    lang: Lang,
    path: PathBuf,
    args: String,
    version: String,
}

#[tauri::command]
pub async fn detect_compilers() -> CmdResult<Vec<CompilerInfo>> {
    blocking(|| {
        Ok(judge_core::detect_compilers()
            .into_iter()
            .map(|c| {
                let version = c.version();
                CompilerInfo { lang: c.lang, path: c.path, args: c.args, version }
            })
            .collect())
    })
    .await
}

#[derive(Debug, Clone, Serialize)]
pub struct CellView {
    pub contestant: String,
    pub problem: String,
    pub score: f64,
    pub full: f64,
    pub label: String,
    pub status: String,
    pub time_ms: u64,
    /// Sources, data or settings changed since this result was produced.
    pub stale: bool,
}

#[derive(Serialize)]
pub struct ContestView {
    root: String,
    contest: Contest,
    contestants: Vec<String>,
    cells: Vec<CellView>,
}

fn contest_view(project: &Project) -> CmdResult<ContestView> {
    let settings = AppSettings::load();
    let engine = project.engine(&settings);
    let hashes = engine.problem_hashes();
    let problems: HashMap<&str, &Problem> = project.contest.problems.iter().map(|p| (p.name.as_str(), p)).collect();
    let mut cells = Vec::new();
    for s in project.summaries().map_err(err)? {
        let Some(p) = problems.get(s.problem.as_str()) else { continue };
        if !project.contestants.contains(&s.contestant) {
            continue;
        }
        let stale = hashes.get(&s.problem) != Some(&s.problem_hash)
            || judge_core::source_fingerprint(&project.dir, &s.contestant, p) != s.source_hash;
        cells.push(CellView {
            contestant: s.contestant,
            problem: s.problem,
            score: s.score,
            full: s.full,
            label: s.label,
            status: format!("{:?}", s.status),
            time_ms: s.time_ms,
            stale,
        });
    }
    Ok(ContestView {
        root: project.root().display().to_string(),
        contest: project.contest.clone(),
        contestants: project.contestants.clone(),
        cells,
    })
}

#[tauri::command]
pub async fn open_contest(path: String, state: State<'_, AppState>) -> CmdResult<ContestView> {
    if state.is_judging() {
        return Err("Judging is in progress".into());
    }
    let project = blocking(move || Project::open(std::path::Path::new(&path)).map_err(err)).await?;
    let project = Arc::new(project);
    let mut settings = AppSettings::load();
    settings.add_recent(project.root().to_path_buf());
    let _ = settings.save();
    *state.project.write() = Some(project.clone());
    blocking(move || contest_view(&project)).await
}

#[tauri::command]
pub async fn reload_contest(state: State<'_, AppState>) -> CmdResult<ContestView> {
    let current = state.project()?;
    if state.is_judging() {
        return blocking(move || contest_view(&current)).await;
    }
    let root = current.root().to_path_buf();
    drop(current);
    let project = Arc::new(blocking(move || Project::open(&root).map_err(err)).await?);
    *state.project.write() = Some(project.clone());
    blocking(move || contest_view(&project)).await
}

#[tauri::command]
pub async fn close_contest(state: State<'_, AppState>) -> CmdResult<()> {
    if state.is_judging() {
        return Err("Judging is in progress".into());
    }
    *state.project.write() = None;
    Ok(())
}

#[tauri::command]
pub async fn save_contest_config(contest: Contest, state: State<'_, AppState>) -> CmdResult<ContestView> {
    if state.is_judging() {
        return Err("Judging is in progress".into());
    }
    contest.validate().map_err(err)?;
    let root = state.project()?.root().to_path_buf();
    let project = blocking(move || {
        contest.save(&root).map_err(err)?;
        Project::open(&root).map_err(err)
    })
    .await?;
    let project = Arc::new(project);
    *state.project.write() = Some(project.clone());
    blocking(move || contest_view(&project)).await
}

#[tauri::command]
pub async fn detect_problems(time_ms: u64, memory_mb: u64, state: State<'_, AppState>) -> CmdResult<Vec<Problem>> {
    let project = state.project()?;
    blocking(move || Ok(judge_core::discover::detect_problems(&project.dir, time_ms, memory_mb))).await
}

#[tauri::command]
pub async fn get_result(
    contestant: String,
    problem: String,
    state: State<'_, AppState>,
) -> CmdResult<Option<ProblemResult>> {
    let project = state.project()?;
    blocking(move || project.result(&contestant, &problem).map_err(err)).await
}

#[derive(Serialize)]
pub struct SourceFile {
    path: String,
    text: String,
}

#[tauri::command]
pub async fn read_source(
    contestant: String,
    problem: String,
    state: State<'_, AppState>,
) -> CmdResult<Vec<SourceFile>> {
    let project = state.project()?;
    blocking(move || {
        let Some(p) = project.contest.problem(&problem) else { return Ok(Vec::new()) };
        let names: Vec<String> = match (&p.kind, &p.communication) {
            (judge_core::ProblemKind::Communication, Some(c)) => c.sources.clone(),
            (judge_core::ProblemKind::Answer, _) => Vec::new(),
            _ => vec![p.source_name().to_string()],
        };
        let mut out = Vec::new();
        for n in names {
            if let Some(path) = judge_core::discover::find_source(&project.dir, &contestant, p, &n) {
                let data = std::fs::read(&path).unwrap_or_default();
                let text =
                    String::from_utf8(data.clone()).unwrap_or_else(|_| String::from_utf8_lossy(&data).into_owned());
                out.push(SourceFile { path: path.display().to_string(), text });
            }
        }
        Ok(out)
    })
    .await
}

#[derive(Debug, Deserialize)]
pub struct JudgeRequest {
    #[serde(default)]
    problems: Vec<String>,
    #[serde(default)]
    contestants: Vec<String>,
    #[serde(default)]
    only_stale: bool,
    /// Explicit (contestant, problem) pairs; overrides the filters.
    #[serde(default)]
    pairs: Vec<(String, String)>,
}

#[tauri::command]
pub async fn start_judge(req: JudgeRequest, app: AppHandle, state: State<'_, AppState>) -> CmdResult<()> {
    let project = state.project()?;
    let cancel = {
        let mut j = state.judging.lock();
        if j.is_some() {
            return Err("Judging is already running".into());
        }
        let c = Arc::new(AtomicBool::new(false));
        *j = Some(c.clone());
        c
    };
    let progress = state.progress.clone();
    *progress.lock() = Some(Progress::new());

    // Emitter: pushes batched progress to the UI until judging ends.
    let finished = Arc::new(AtomicBool::new(false));
    {
        let progress = progress.clone();
        let finished = finished.clone();
        let app = app.clone();
        std::thread::spawn(move || loop {
            let done = finished.load(Ordering::Acquire);
            let snapshot = {
                let mut p = progress.lock();
                match p.as_mut() {
                    Some(p) if p.dirty || done => Some(p.take()),
                    _ => None,
                }
            };
            if let Some(s) = snapshot {
                let _ = app.emit("judge-progress", s);
            }
            if done {
                break;
            }
            std::thread::sleep(Duration::from_millis(100));
        });
    }

    let judging_slot = app.clone();
    std::thread::spawn(move || {
        let settings = AppSettings::load();
        let sink = |ev: &judge_core::Event| {
            if let Some(p) = progress.lock().as_mut() {
                p.on_event(ev);
            }
        };
        let result = if req.pairs.is_empty() {
            let sel = Selection { problems: req.problems, contestants: req.contestants, only_stale: req.only_stale };
            project.judge(&settings, &sel, &sink, &cancel).map(|_| ())
        } else {
            judge_pairs(&project, &settings, req.pairs, &sink, &cancel)
        };
        if let Some(p) = progress.lock().as_mut() {
            p.view.running = false;
            p.dirty = true;
            if let Err(e) = &result {
                p.view.error = Some(err(e));
            }
        }
        finished.store(true, Ordering::Release);
        let st = judging_slot.state::<AppState>();
        *st.judging.lock() = None;
    });
    Ok(())
}

fn judge_pairs(
    project: &Project,
    settings: &AppSettings,
    pairs: Vec<(String, String)>,
    sink: &(dyn Fn(&judge_core::Event) + Sync),
    cancel: &AtomicBool,
) -> anyhow::Result<()> {
    // Group by problem so each call shares the same filters.
    let mut by: HashMap<String, Vec<String>> = HashMap::new();
    for (c, p) in pairs {
        by.entry(p).or_default().push(c);
    }
    let engine = project.engine(settings);
    let tasks: Vec<judge_core::Task> = by
        .into_iter()
        .flat_map(|(p, cs)| cs.into_iter().map(move |c| judge_core::Task { contestant: c, problem: p.clone() }))
        .collect();
    let save_err: Mutex<Option<anyhow::Error>> = Mutex::new(None);
    engine.run(
        tasks,
        &|ev| {
            if let judge_core::Event::TaskDone { result, .. } = &ev {
                if let Err(e) = project.store.lock().save(result) {
                    save_err.lock().get_or_insert(e);
                }
            }
            sink(&ev);
        },
        cancel,
    )?;
    match save_err.into_inner() {
        Some(e) => Err(e),
        None => Ok(()),
    }
}

#[tauri::command]
pub async fn cancel_judge(state: State<'_, AppState>) -> CmdResult<()> {
    if let Some(c) = state.judging.lock().as_ref() {
        c.store(true, Ordering::Relaxed);
    }
    Ok(())
}

#[tauri::command]
pub async fn judge_status(state: State<'_, AppState>) -> CmdResult<Option<ProgressView>> {
    Ok(state.progress.lock().as_ref().map(|p| p.view.clone()))
}

#[derive(Serialize)]
pub struct ProbeView {
    name: String,
    passed: bool,
    detail: String,
}

#[tauri::command]
pub async fn run_selftest(state: State<'_, AppState>) -> CmdResult<Vec<ProbeView>> {
    if state.is_judging() {
        return Err("Judging is in progress".into());
    }
    blocking(|| {
        let mut out = Vec::new();
        oij::selftest::run(&AppSettings::load(), &mut |r| {
            out.push(ProbeView { name: r.name.to_string(), passed: r.passed, detail: r.detail.clone() })
        })
        .map_err(err)?;
        Ok(out)
    })
    .await
}

use tauri::Manager;

/// Files below `data/<sub>` (relative to that folder, `/` separated), for
/// the "add by pattern" dialog.
#[tauri::command]
pub async fn list_data_files(sub: String, state: State<'_, AppState>) -> CmdResult<Vec<String>> {
    let project = state.project()?;
    blocking(move || {
        let rel = std::path::Path::new(sub.trim_matches(['/', '\\']));
        if rel.components().any(|c| !matches!(c, std::path::Component::Normal(_))) {
            return Err("invalid folder".to_string());
        }
        let base = project.dir.data().join(rel);
        let mut out = Vec::new();
        let mut stack = vec![base.clone()];
        while let Some(d) = stack.pop() {
            let Ok(entries) = std::fs::read_dir(&d) else { continue };
            for e in entries.flatten() {
                let p = e.path();
                if e.file_type().map(|t| t.is_dir()).unwrap_or(false) {
                    stack.push(p);
                } else if let Ok(rel) = p.strip_prefix(&base) {
                    out.push(rel.to_string_lossy().replace('\\', "/"));
                }
                if out.len() > 200_000 {
                    break;
                }
            }
        }
        Ok(out)
    })
    .await
}

#[tauri::command]
pub async fn get_stats(state: State<'_, AppState>) -> CmdResult<analysis::ContestStats> {
    let project = state.project()?;
    blocking(move || project.stats().map_err(err)).await
}

/// `format`: csv | xlsx | html | htm.
#[tauri::command]
pub async fn export_results(format: String, path: String, state: State<'_, AppState>) -> CmdResult<()> {
    let project = state.project()?;
    let fmt = oij::project::ExportFormat::parse(&format).ok_or_else(|| format!("unknown format {format}"))?;
    blocking(move || {
        let lang = AppSettings::load().language;
        project.export(fmt, std::path::Path::new(&path), &lang).map_err(err)
    })
    .await
}

#[tauri::command]
pub async fn similarity(
    problem: String,
    threshold: f64,
    state: State<'_, AppState>,
) -> CmdResult<Vec<analysis::similarity::Pair>> {
    let project = state.project()?;
    blocking(move || Ok(project.similarity(&problem, threshold))).await
}
