//! Parallel judging of many (contestant, problem) tasks.

use std::collections::{HashMap, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::OnceLock;

use anyhow::{Context, Result};
use parking_lot::{Condvar, Mutex};

use crate::compile::{build, BuildOutput, BuildRequest, CompilerConfig, Lang};
use crate::judge::{
    has_time_limit_case, judge_problem, recheck_problem, JudgeContext, ProblemTools, Settings, Toolset,
};
use crate::model::{Contest, ContestDir, Problem};
use crate::result::{CaseResult, ProblemResult};
use crate::workspace::{Worker, Workspace};

pub const TESTLIB_H: &str = include_str!("../../../assets/testlib/testlib.h");

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Task {
    pub contestant: String,
    pub problem: String,
}

#[derive(Debug, Clone)]
pub enum Event {
    /// Compile phase progress: `done` of `total` programs (and helper
    /// checkers / interactors) prepared.
    Compiled {
        done: usize,
        total: usize,
    },
    /// Setup finished; `total` tasks will be judged by `workers` workers.
    Started {
        total: usize,
        workers: usize,
    },
    TaskStarted {
        worker: usize,
        task: Task,
    },
    Case {
        worker: usize,
        task: Task,
        subtask: usize,
        case: CaseResult,
    },
    TaskDone {
        worker: usize,
        result: ProblemResult,
    },
    /// Compiling paused (true) because the compile cache is over its limit,
    /// or resumed (false).
    CompilePaused(bool),
    /// Informational message (setup steps, warnings).
    Message(String),
    /// All tasks are judged; `total` of them with time-limit cases are now
    /// re-run one at a time (see `Settings::recheck_tle`).
    RecheckStarted {
        total: usize,
    },
    /// A task re-run by the recheck; `result` replaces the earlier one.
    Rechecked {
        worker: usize,
        result: ProblemResult,
        previous_label: String,
        previous_score: f64,
    },
}

impl Event {
    /// The result to save, if this event carries one.
    pub fn result(&self) -> Option<&ProblemResult> {
        match self {
            Event::TaskDone { result, .. } | Event::Rechecked { result, .. } => Some(result),
            _ => None,
        }
    }
}

pub struct Engine {
    pub dir: ContestDir,
    pub contest: Contest,
    pub settings: Settings,
    pub tools: Toolset,
}

fn file_stamp(h: &mut blake3::Hasher, p: &Path) {
    h.update(p.to_string_lossy().as_bytes());
    if let Ok(m) = std::fs::metadata(p) {
        h.update(&m.len().to_le_bytes());
        if let Ok(t) = m.modified() {
            if let Ok(d) = t.duration_since(std::time::UNIX_EPOCH) {
                h.update(&d.as_nanos().to_le_bytes());
            }
        }
    } else {
        h.update(b"missing");
    }
}

impl Engine {
    pub fn new(dir: ContestDir, contest: Contest, settings: Settings, compilers: Vec<CompilerConfig>) -> Engine {
        Engine { dir, contest, settings, tools: Toolset::new(compilers) }
    }

    /// Fingerprint of a problem's configuration, data files and judge
    /// settings. Results with a different fingerprint are stale. Data files
    /// are fingerprinted by size and modification time, which is cheap even
    /// for gigabytes of data.
    pub fn problem_hash(&self, p: &Problem) -> String {
        let mut h = blake3::Hasher::new();
        h.update(serde_json::to_string(p).unwrap_or_default().as_bytes());
        for (c, v) in &self.tools.compilers {
            h.update(c.args.as_bytes());
            h.update(v.as_bytes());
        }
        h.update(&[self.settings.patch_stack as u8]);
        h.update(&self.settings.output_limit_mb.to_le_bytes());
        let data = |rel: &str| self.dir.data_file(rel);
        for st in &p.subtasks {
            for c in &st.cases {
                file_stamp(&mut h, &data(&c.input));
                file_stamp(&mut h, &data(&c.output));
            }
        }
        let mut extra: Vec<String> = Vec::new();
        if let Some(c) = &p.checker {
            extra.push(c.path.clone());
        }
        if let Some(i) = &p.interactor {
            extra.push(i.clone());
        }
        if let Some(g) = &p.grader {
            extra.push(g.header_path.clone());
            extra.push(g.grader_path.clone());
        }
        if let Some(c) = &p.communication {
            extra.extend(c.interfaces.iter().cloned());
        }
        for e in extra.iter().filter(|e| !e.is_empty()) {
            file_stamp(&mut h, &data(e));
        }
        h.finalize().to_hex()[..32].to_string()
    }

    pub fn problem_hashes(&self) -> HashMap<String, String> {
        self.contest.problems.iter().map(|p| (p.name.clone(), self.problem_hash(p))).collect()
    }

    /// Make compiler installations readable from inside the sandbox.
    pub(crate) fn prepare_toolchains(&self, sink: &(dyn Fn(Event) + Sync)) -> Result<()> {
        let mut done: Vec<PathBuf> = Vec::new();
        for (c, _) in &self.tools.compilers {
            let root = c.toolchain_root();
            if done.contains(&root) {
                continue;
            }
            match sandbox::Toolchain::grant_read(&root) {
                Ok(true) => sink(Event::Message(format!("Granted sandbox read access to {}", root.display()))),
                Ok(false) => {}
                Err(e) => {
                    return Err(anyhow::anyhow!(
                        "Cannot make compiler directory {} readable inside the sandbox ({e}). \
                         Use a compiler in a folder you own (e.g. the portable bundle) or grant \
                         \"ALL APPLICATION PACKAGES\" read access to it.",
                        root.display()
                    ))
                }
            }
            done.push(root);
        }
        Ok(())
    }

    pub(crate) fn testlib_path(&self) -> Result<PathBuf> {
        let p = self.dir.state().join("testlib.h");
        if std::fs::read_to_string(&p).map(|s| s != TESTLIB_H).unwrap_or(true) {
            std::fs::create_dir_all(self.dir.state())?;
            std::fs::write(&p, TESTLIB_H)?;
        }
        Ok(p)
    }

    /// Compile (or locate) a checker / interactor given relative to `data/`.
    fn helper_program(&self, worker: &Worker, rel: &str, testlib: &Path) -> std::result::Result<PathBuf, String> {
        let src = self.dir.data_file(rel);
        if !src.is_file() {
            return Err(format!("{rel} not found"));
        }
        if src.extension().is_some_and(|e| e.eq_ignore_ascii_case("exe")) {
            return Ok(src);
        }
        let Some(lang) = Lang::from_path(&src) else { return Err(format!("{rel}: unsupported file type")) };
        let Some((compiler, version)) = self.tools.get(lang) else {
            return Err(format!("No compiler for {rel}"));
        };
        let name = src.file_name().unwrap().to_string_lossy().into_owned();
        let req = BuildRequest {
            files: vec![(src, name, true), (testlib.to_path_buf(), "testlib.h".into(), false)],
            compiler: compiler.clone(),
            extra_args: String::new(),
        };
        let mut limits = self.settings.build.clone();
        limits.cpu_ms = limits.cpu_ms.max(60_000); // testlib is heavy to compile
        match build(worker, &req, version, &self.dir.build_cache(), &limits) {
            Ok(BuildOutput::Ok { exe, lease, .. }) => {
                // Used for the whole session: never evict it.
                lease.keep_forever();
                Ok(exe)
            }
            Ok(BuildOutput::Failed { log }) => Err(format!("{rel} failed to compile:\n{log}")),
            Err(e) => Err(format!("{rel}: {e:#}")),
        }
    }

    pub(crate) fn problem_tools(
        &self,
        worker: &Worker,
        p: &Problem,
        testlib: &Path,
        sink: &(dyn Fn(Event) + Sync),
    ) -> ProblemTools {
        let mut t = ProblemTools::default();
        if let Some(c) = &p.checker {
            let t0 = std::time::Instant::now();
            t.checker = Some(self.helper_program(worker, &c.path, testlib));
            sink(Event::Message(format!("Prepared checker for {} ({:.1}s)", p.name, t0.elapsed().as_secs_f64())));
        }
        if let Some(i) = &p.interactor {
            let t0 = std::time::Instant::now();
            t.interactor = Some(self.helper_program(worker, i, testlib));
            sink(Event::Message(format!("Prepared interactor for {} ({:.1}s)", p.name, t0.elapsed().as_secs_f64())));
        }
        t
    }

    /// Judge `tasks` in parallel. Every finished task is reported through
    /// `sink` (persisting results is the caller's job). Returns early when
    /// `cancel` is set; unfinished tasks produce no result.
    pub fn run(&self, tasks: Vec<Task>, sink: &(dyn Fn(Event) + Sync), cancel: &AtomicBool) -> Result<()> {
        if tasks.is_empty() {
            sink(Event::Started { total: 0, workers: 0 });
            return Ok(());
        }
        self.prepare_toolchains(sink)?;
        let want =
            if self.settings.workers == 0 { crate::workspace::default_worker_count() } else { self.settings.workers };
        let workers = want.min(tasks.len()).max(1);
        let needs_compile = self.contest.problems.iter().any(|p| p.kind != crate::model::ProblemKind::Answer);
        let overlap = needs_compile && self.settings.overlap_compile;
        let logical = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4);
        // Overlapped: compilers get the cores the pinned judging workers do
        // not use, so they never share a core with a timed program; idle
        // judging workers compile too. Two-phase: compile everything on all
        // logical processors first, then judge.
        let (builders, builder_mask) = if !needs_compile {
            (0, 0)
        } else if overlap {
            let mask = if self.settings.pin_cores { crate::workspace::spare_cpu_mask(workers) } else { 0 };
            let n = if mask != 0 {
                mask.count_ones() as usize
            } else if self.settings.pin_cores && workers <= sandbox::performance_core_masks().len() {
                0 // every core judges; the judging workers compile themselves
            } else {
                logical.saturating_sub(workers)
            };
            (n, mask)
        } else {
            (logical, 0)
        };
        let builders = std::env::var("OIJ_BUILDERS").ok().and_then(|v| v.parse().ok()).unwrap_or(builders).min(32);
        let t_setup = std::time::Instant::now();
        let ws = Workspace::with_builders(workers, self.settings.pin_cores, builders, builder_mask)
            .context("setting up sandbox workers")?;
        if std::env::var_os("OIJ_DEBUG").is_some() {
            eprintln!(
                "sandbox setup: {} workers + {} builders (mask {:#x}, overlap {}) in {} ms",
                workers,
                builders,
                builder_mask,
                overlap,
                t_setup.elapsed().as_millis()
            );
        }

        let testlib = self.testlib_path()?;
        let cells: HashMap<String, OnceLock<ProblemTools>> =
            self.contest.problems.iter().map(|p| (p.name.clone(), OnceLock::new())).collect();
        let problem_tools = |w: &Worker, p: &Problem| -> ProblemTools {
            match cells.get(&p.name) {
                Some(cell) => cell.get_or_init(|| self.problem_tools(w, p, &testlib, sink)).clone(),
                None => ProblemTools::default(),
            }
        };
        let problem_hashes = self.problem_hashes();
        std::fs::create_dir_all(self.dir.build_cache())?;

        let ctx = JudgeContext {
            dir: &self.dir,
            settings: &self.settings,
            tools: &self.tools,
            problem_tools: &problem_tools,
            problem_hashes: &problem_hashes,
            cancel,
        };
        let order = tasks;
        // Helper programs first (they are slow to build), then submissions.
        let mut helpers: Vec<&Problem> = Vec::new();
        if needs_compile && !ws.builders.is_empty() {
            for t in &order {
                if let Some(p) = self.contest.problem(&t.problem) {
                    if (p.checker.is_some() || p.interactor.is_some()) && !helpers.iter().any(|h| h.name == p.name) {
                        helpers.push(p);
                    }
                }
            }
        }
        let compile_total = helpers.len() + order.len();
        let compiled = AtomicUsize::new(0);
        // Results with time-limit cases, re-run alone at the end.
        let recheck: Mutex<Vec<ProblemResult>> = Mutex::new(Vec::new());
        let note_compiled =
            || sink(Event::Compiled { done: compiled.fetch_add(1, Ordering::Relaxed) + 1, total: compile_total });
        let judge_one = |worker: &Worker, task: &Task| {
            let Some(problem) = self.contest.problem(&task.problem) else { return };
            sink(Event::TaskStarted { worker: worker.index(), task: task.clone() });
            let mut on_case = |si: usize, c: &CaseResult| {
                sink(Event::Case { worker: worker.index(), task: task.clone(), subtask: si, case: c.clone() })
            };
            if let Some(result) = judge_problem(&ctx, worker, &task.contestant, problem, &mut on_case) {
                if self.settings.recheck_tle && has_time_limit_case(&result) {
                    recheck.lock().push(result.clone());
                }
                sink(Event::TaskDone { worker: worker.index(), result });
            }
        };
        // Compile cache limit (overlapped mode only: in two-phase mode every
        // program must exist before judging starts).
        let cache_dir = self.dir.build_cache();
        let limit = if overlap { self.settings.cache_limit_mb << 20 } else { 0 };
        crate::compile::reset_released();
        let cache_used = AtomicU64::new(if limit > 0 { crate::compile::cache_usage(&cache_dir).1 } else { 0 });
        let prebuild = |worker: &Worker, task: &Task| -> Option<crate::compile::Lease> {
            let problem = self.contest.problem(&task.problem)?;
            let (lease, added) = crate::judge::prebuild(&ctx, worker, &task.contestant, problem)?;
            cache_used.fetch_add(added, Ordering::Relaxed);
            Some(lease)
        };
        // Is there room in the cache for another program? Over the limit,
        // remove programs that are not in use (judged ones first) down to 90%.
        let room_ns = AtomicU64::new(0); // time spent making room (OIJ_DEBUG)
        let make_room = || -> bool {
            if limit == 0 || cache_used.load(Ordering::Relaxed) <= limit {
                return true;
            }
            let t0 = std::time::Instant::now();
            let _t = TimeGuard(&room_ns, t0);
            let used = crate::compile::cache_usage(&cache_dir).1;
            let freed = if used > limit { crate::compile::evict(&cache_dir, used - limit / 10 * 9) } else { 0 };
            let now = used.saturating_sub(freed);
            cache_used.store(now, Ordering::Relaxed);
            now <= limit
        };
        let paused = AtomicBool::new(false);
        let note_pause = |pause: bool| {
            if paused.swap(pause, Ordering::Relaxed) != pause {
                sink(Event::CompilePaused(pause));
                sink(Event::Message(if pause {
                    format!(
                        "Compile cache is over the limit ({} MB): compiling paused until judged programs are removed",
                        limit >> 20
                    )
                } else {
                    "Compiling resumed".to_string()
                }));
            }
        };

        if overlap {
            sink(Event::Compiled { done: 0, total: compile_total });
            sink(Event::Started { total: order.len(), workers: ws.workers.len() });
            let n = order.len();
            let sched =
                Mutex::new(Schedule { next_helper: 0, next_build: 0, building: 0, judging: 0, ready: VecDeque::new() });
            let wake = Condvar::new();
            std::thread::scope(|s| {
                for builder in &ws.builders {
                    let (sched, wake, helpers, order) = (&sched, &wake, &helpers, &order);
                    let (prebuild, problem_tools, note_compiled) = (&prebuild, &problem_tools, &note_compiled);
                    let (make_room, note_pause) = (&make_room, &note_pause);
                    s.spawn(move || {
                        while !cancel.load(Ordering::Relaxed) {
                            let more = {
                                let q = sched.lock();
                                q.next_helper < helpers.len() || q.next_build < n
                            };
                            if !more {
                                break;
                            }
                            // Cache full: wait for judging to free programs, unless
                            // nothing is in flight that could (then go on regardless).
                            if !make_room() {
                                let mut q = sched.lock();
                                if q.next_helper >= helpers.len()
                                    && !(q.ready.is_empty() && q.building == 0 && q.judging == 0)
                                {
                                    note_pause(true);
                                    wake.wait_for(&mut q, std::time::Duration::from_millis(500));
                                    continue;
                                }
                            } else {
                                note_pause(false);
                            }
                            let job = {
                                let mut q = sched.lock();
                                if q.next_helper < helpers.len() {
                                    q.next_helper += 1;
                                    Err(q.next_helper - 1)
                                } else if q.next_build < n {
                                    q.next_build += 1;
                                    q.building += 1;
                                    Ok(q.next_build - 1)
                                } else {
                                    break;
                                }
                            };
                            match job {
                                Err(h) => {
                                    problem_tools(builder, helpers[h]);
                                }
                                Ok(i) => {
                                    let lease = prebuild(builder, &order[i]);
                                    let mut q = sched.lock();
                                    q.building -= 1;
                                    q.ready.push_back((i, lease));
                                    wake.notify_all();
                                }
                            }
                            note_compiled();
                        }
                        // Let waiting judges see that nothing more is coming.
                        let _q = sched.lock();
                        wake.notify_all();
                    });
                }
                for worker in &ws.workers {
                    let (sched, wake, order) = (&sched, &wake, &order);
                    let (prebuild, judge_one, note_compiled) = (&prebuild, &judge_one, &note_compiled);
                    let (make_room, note_pause) = (&make_room, &note_pause);
                    s.spawn(move || loop {
                        let job = loop {
                            let mut q = sched.lock();
                            if cancel.load(Ordering::Relaxed) {
                                break None;
                            }
                            if let Some((i, lease)) = q.ready.pop_front() {
                                q.judging += 1;
                                break Some((i, lease, false));
                            }
                            // Nothing compiled is waiting: compile the next one here
                            // rather than idle (if the cache has room).
                            if q.next_build < n {
                                let idle = q.building == 0 && q.judging == 0;
                                drop(q);
                                let room = make_room();
                                let mut q = sched.lock();
                                if room || idle {
                                    if room {
                                        note_pause(false);
                                    }
                                    if q.next_build < n {
                                        q.next_build += 1;
                                        q.judging += 1;
                                        break Some((q.next_build - 1, None, true));
                                    }
                                    continue;
                                }
                                note_pause(true);
                                wake.wait_for(&mut q, std::time::Duration::from_millis(500));
                                continue;
                            }
                            if q.building == 0 {
                                break None;
                            }
                            wake.wait_for(&mut q, std::time::Duration::from_millis(200));
                        };
                        let Some((i, mut lease, compile)) = job else { break };
                        if compile {
                            lease = prebuild(worker, &order[i]);
                            note_compiled();
                        }
                        judge_one(worker, &order[i]);
                        // The program may now be evicted; paused compilers can retry.
                        drop(lease);
                        sched.lock().judging -= 1;
                        wake.notify_all();
                    });
                }
            });
            // The last programs were compiled after the final check: trim.
            if limit > 0 {
                let used = crate::compile::cache_usage(&cache_dir).1;
                if used > limit {
                    crate::compile::evict(&cache_dir, used - limit);
                }
            }
            if limit > 0 && std::env::var_os("OIJ_DEBUG").is_some() {
                eprintln!("cache limit: {} ms spent making room", room_ns.load(Ordering::Relaxed) / 1_000_000);
            }
            self.recheck(&ws.workers[0], &ctx, std::mem::take(&mut *recheck.lock()), sink, cancel);
            return Ok(());
        }

        if !ws.builders.is_empty() {
            let next = AtomicUsize::new(0);
            sink(Event::Compiled { done: 0, total: compile_total });
            std::thread::scope(|s| {
                for builder in &ws.builders {
                    let (order, helpers, next, problem_tools) = (&order, &helpers, &next, &problem_tools);
                    let (prebuild, note_compiled) = (&prebuild, &note_compiled);
                    s.spawn(move || {
                        while !cancel.load(Ordering::Relaxed) {
                            let i = next.fetch_add(1, Ordering::Relaxed);
                            if i < helpers.len() {
                                problem_tools(builder, helpers[i]);
                            } else {
                                let Some(task) = order.get(i - helpers.len()) else { break };
                                drop(prebuild(builder, task));
                            }
                            note_compiled();
                        }
                    });
                }
            });
        }
        if cancel.load(Ordering::Relaxed) {
            return Ok(());
        }

        sink(Event::Started { total: order.len(), workers: ws.workers.len() });
        let next_task = AtomicUsize::new(0);
        std::thread::scope(|s| {
            for worker in &ws.workers {
                let (order, next_task, judge_one) = (&order, &next_task, &judge_one);
                s.spawn(move || {
                    while !cancel.load(Ordering::Relaxed) {
                        let Some(task) = order.get(next_task.fetch_add(1, Ordering::Relaxed)) else { break };
                        judge_one(worker, task);
                    }
                });
            }
        });
        self.recheck(&ws.workers[0], &ctx, std::mem::take(&mut *recheck.lock()), sink, cancel);
        Ok(())
    }

    /// Re-run, one task at a time on one worker while nothing else runs,
    /// the time-limit cases of `results`.
    fn recheck(
        &self,
        worker: &Worker,
        ctx: &JudgeContext,
        mut results: Vec<ProblemResult>,
        sink: &(dyn Fn(Event) + Sync),
        cancel: &AtomicBool,
    ) {
        if results.is_empty() || cancel.load(Ordering::Relaxed) {
            return;
        }
        results.sort_by(|a, b| (&a.contestant, &a.problem).cmp(&(&b.contestant, &b.problem)));
        sink(Event::RecheckStarted { total: results.len() });
        sink(Event::Message(format!(
            "Re-running the time-limit cases of {} task(s) one at a time (parallel judging can slow programs down)",
            results.len()
        )));
        let t0 = std::time::Instant::now();
        let mut changed = 0;
        for prev in results {
            if cancel.load(Ordering::Relaxed) {
                return;
            }
            let Some(problem) = self.contest.problem(&prev.problem) else { continue };
            let task = Task { contestant: prev.contestant.clone(), problem: prev.problem.clone() };
            sink(Event::TaskStarted { worker: worker.index(), task: task.clone() });
            let mut on_case = |si: usize, c: &CaseResult| {
                sink(Event::Case { worker: worker.index(), task: task.clone(), subtask: si, case: c.clone() })
            };
            let Some(result) = recheck_problem(ctx, worker, &prev.contestant, problem, &prev, &mut on_case) else {
                return; // cancelled
            };
            if result.score != prev.score {
                changed += 1;
                sink(Event::Message(format!(
                    "Recheck: {} {} {} -> {}",
                    prev.contestant, prev.problem, prev.score, result.score
                )));
            }
            sink(Event::Rechecked {
                worker: worker.index(),
                previous_label: prev.label().to_string(),
                previous_score: prev.score,
                result,
            });
        }
        sink(Event::Message(format!(
            "Recheck finished in {:.1}s: {changed} score(s) changed",
            t0.elapsed().as_secs_f64()
        )));
    }
}

/// Shared state of the overlapped compile / judge pipeline.
struct Schedule {
    /// Next helper (checker / interactor) to build.
    next_helper: usize,
    /// Next task to compile.
    next_build: usize,
    /// Tasks being compiled by compile workers.
    building: usize,
    /// Tasks being compiled or judged by judging workers.
    judging: usize,
    /// Compiled tasks waiting for a judging worker, with a lease that keeps
    /// their program in the cache until judged.
    ready: VecDeque<(usize, Option<crate::compile::Lease>)>,
}

/// Adds the time since `.1` to `.0` when dropped.
struct TimeGuard<'a>(&'a AtomicU64, std::time::Instant);

impl Drop for TimeGuard<'_> {
    fn drop(&mut self) {
        self.0.fetch_add(self.1.elapsed().as_nanos() as u64, Ordering::Relaxed);
    }
}
