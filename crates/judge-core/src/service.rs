//! Resident judging service for online contests: sandbox workers are set up
//! once and judge submissions from a queue as they arrive.

use std::collections::{HashMap, VecDeque};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, OnceLock};

use anyhow::{Context, Result};
use parking_lot::{Condvar, Mutex};

use crate::compile::Lease;
use crate::engine::{Engine, Event};
use crate::judge::{compile_check, judge_problem, prebuild, JudgeContext, ProblemTools};
use crate::model::Problem;
use crate::result::{CaseResult, ProblemResult};
use crate::workspace::{Worker, Workspace};

/// One submission to judge.
#[derive(Debug, Clone)]
pub struct Job {
    /// Caller's identifier, echoed in events.
    pub id: i64,
    pub contestant: String,
    pub problem: String,
    /// Directory holding `<contestant>/<problem>/<source>` for this submission.
    pub source_root: PathBuf,
    /// Stop at the first failed case (ICPC).
    pub stop_on_failure: bool,
    /// Only compile (OI compile check); see [`crate::judge::compile_check`].
    pub compile_only: bool,
}

#[derive(Debug, Clone)]
pub enum ServiceEvent {
    Started { id: i64 },
    Case { id: i64, subtask: usize, case: CaseResult },
    /// `result` is `None` when judging was cancelled.
    Done { id: i64, result: Option<ProblemResult> },
    Message(String),
}

pub type ServiceSink = Arc<dyn Fn(ServiceEvent) + Send + Sync>;

struct Item {
    job: Job,
    /// A compile worker is compiling it (or has compiled it).
    prebuilding: bool,
    lease: Option<Lease>,
}

struct Shared {
    queue: Mutex<VecDeque<Item>>,
    wake: Condvar,
    /// Finish the running jobs, then stop.
    stop: AtomicBool,
    /// Abort running jobs.
    cancel: AtomicBool,
    running: AtomicUsize,
    /// Contestants with a job being judged right now (fair queueing).
    busy: Mutex<Vec<String>>,
}

/// Judges queued jobs until shut down.
pub struct JudgeService {
    shared: Arc<Shared>,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl JudgeService {
    /// Set up the sandbox workers and start judging. Fails if the workers
    /// cannot be created.
    pub fn start(engine: Engine, sink: ServiceSink) -> Result<JudgeService> {
        let shared = Arc::new(Shared {
            queue: Mutex::new(VecDeque::new()),
            wake: Condvar::new(),
            stop: AtomicBool::new(false),
            cancel: AtomicBool::new(false),
            running: AtomicUsize::new(0),
            busy: Mutex::new(Vec::new()),
        });
        let (ready_tx, ready_rx) = std::sync::mpsc::channel::<Result<()>>();
        let sh = shared.clone();
        let thread = std::thread::Builder::new()
            .name("oij-service".into())
            .spawn(move || {
                if let Err(e) = run(&engine, &sh, &sink, &ready_tx) {
                    let _ = ready_tx.send(Err(e));
                }
            })
            .context("starting judging service")?;
        match ready_rx.recv() {
            Ok(Ok(())) => Ok(JudgeService { shared, thread: Some(thread) }),
            Ok(Err(e)) => {
                let _ = thread.join();
                Err(e)
            }
            Err(_) => {
                let _ = thread.join();
                anyhow::bail!("judging service stopped during setup")
            }
        }
    }

    pub fn submit(&self, job: Job) {
        self.shared.queue.lock().push_back(Item { job, prebuilding: false, lease: None });
        self.shared.wake.notify_all();
    }

    /// Jobs waiting in the queue.
    pub fn queued(&self) -> usize {
        self.shared.queue.lock().len()
    }

    /// Jobs being judged right now.
    pub fn running(&self) -> usize {
        self.shared.running.load(Ordering::Relaxed)
    }

    /// Remove a queued (not yet started) job; returns whether it was queued.
    pub fn withdraw(&self, id: i64) -> bool {
        let mut q = self.shared.queue.lock();
        let before = q.len();
        q.retain(|i| i.job.id != id);
        q.len() != before
    }

    /// Stop the service. With `abort`, running jobs are cancelled (they
    /// report `Done { result: None }`); otherwise they finish first. Jobs
    /// still queued are returned.
    pub fn shutdown(mut self, abort: bool) -> Vec<Job> {
        self.stop(abort)
    }

    fn stop(&mut self, abort: bool) -> Vec<Job> {
        let left: Vec<Job> = self.shared.queue.lock().drain(..).map(|i| i.job).collect();
        self.shared.stop.store(true, Ordering::Relaxed);
        if abort {
            self.shared.cancel.store(true, Ordering::Relaxed);
        }
        self.shared.wake.notify_all();
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
        left
    }
}

impl Drop for JudgeService {
    fn drop(&mut self) {
        if self.thread.is_some() {
            self.stop(true);
        }
    }
}

fn run(
    engine: &Engine,
    shared: &Shared,
    sink: &ServiceSink,
    ready: &std::sync::mpsc::Sender<Result<()>>,
) -> Result<()> {
    let event_sink = |ev: Event| {
        if let Event::Message(m) = ev {
            sink(ServiceEvent::Message(m));
        }
    };
    engine.prepare_toolchains(&event_sink)?;
    let workers =
        if engine.settings.workers == 0 { crate::workspace::default_worker_count() } else { engine.settings.workers };
    // Compile workers on the cores not used for timing (see Engine::run).
    let mask = if engine.settings.pin_cores { crate::workspace::spare_cpu_mask(workers) } else { 0 };
    let builders = (mask.count_ones() as usize).min(16);
    let ws = Workspace::with_builders(workers, engine.settings.pin_cores, builders, mask)
        .context("setting up sandbox workers")?;
    let testlib = engine.testlib_path()?;
    let cells: HashMap<String, OnceLock<ProblemTools>> =
        engine.contest.problems.iter().map(|p| (p.name.clone(), OnceLock::new())).collect();
    let problem_tools = |w: &Worker, p: &Problem| -> ProblemTools {
        match cells.get(&p.name) {
            Some(cell) => cell.get_or_init(|| engine.problem_tools(w, p, &testlib, &event_sink)).clone(),
            None => ProblemTools::default(),
        }
    };
    let problem_hashes = engine.problem_hashes();
    std::fs::create_dir_all(engine.dir.build_cache())?;
    let _ = ready.send(Ok(()));

    // A judging context for one job: its own source root and settings.
    let with_ctx = |job: &Job, f: &mut dyn FnMut(&JudgeContext)| {
        let dir = engine.dir.with_source_root(&job.source_root);
        let mut settings = engine.settings.clone();
        settings.stop_on_failure = job.stop_on_failure;
        let ctx = JudgeContext {
            dir: &dir,
            settings: &settings,
            tools: &engine.tools,
            problem_tools: &problem_tools,
            problem_hashes: &problem_hashes,
            cancel: &shared.cancel,
        };
        f(&ctx);
    };

    std::thread::scope(|s| {
        for builder in &ws.builders {
            let with_ctx = &with_ctx;
            s.spawn(move || loop {
                // Compile the oldest queued job nobody has started on.
                let job = {
                    let mut q = shared.queue.lock();
                    loop {
                        if shared.stop.load(Ordering::Relaxed) {
                            return;
                        }
                        if let Some(item) = q.iter_mut().find(|i| !i.prebuilding) {
                            item.prebuilding = true;
                            break item.job.clone();
                        }
                        shared.wake.wait_for(&mut q, std::time::Duration::from_millis(500));
                    }
                };
                let Some(problem) = engine.contest.problem(&job.problem) else { continue };
                let mut lease = None;
                with_ctx(&job, &mut |ctx| lease = prebuild(ctx, builder, &job.contestant, problem).map(|(l, _)| l));
                if let Some(item) = shared.queue.lock().iter_mut().find(|i| i.job.id == job.id) {
                    item.lease = lease;
                }
            });
        }
        for worker in &ws.workers {
            let with_ctx = &with_ctx;
            s.spawn(move || loop {
                let item = {
                    let mut q = shared.queue.lock();
                    loop {
                        if shared.stop.load(Ordering::Relaxed) {
                            return;
                        }
                        // Fair queueing (as DOMjudge): prefer the oldest job of a
                        // contestant with nothing being judged and no older job
                        // waiting; otherwise simply the oldest job.
                        let pick = {
                            let busy = shared.busy.lock();
                            (0..q.len()).find(|&i| {
                                let c = &q[i].job.contestant;
                                !busy.contains(c) && !q.iter().take(i).any(|o| &o.job.contestant == c)
                            })
                        };
                        if let Some(item) = pick.and_then(|i| q.remove(i)).or_else(|| q.pop_front()) {
                            shared.busy.lock().push(item.job.contestant.clone());
                            break item;
                        }
                        shared.wake.wait_for(&mut q, std::time::Duration::from_millis(500));
                    }
                };
                shared.running.fetch_add(1, Ordering::Relaxed);
                let job = &item.job;
                sink(ServiceEvent::Started { id: job.id });
                let result = match engine.contest.problem(&job.problem) {
                    Some(problem) => {
                        let mut result = None;
                        with_ctx(job, &mut |ctx| {
                            if job.compile_only {
                                result = Some(compile_check(ctx, worker, &job.contestant, problem));
                                return;
                            }
                            let mut on_case = |si: usize, c: &CaseResult| {
                                sink(ServiceEvent::Case { id: job.id, subtask: si, case: c.clone() })
                            };
                            result = judge_problem(ctx, worker, &job.contestant, problem, &mut on_case);
                        });
                        result
                    }
                    None => Some(ProblemResult::empty(
                        &job.contestant,
                        &job.problem,
                        0.0,
                        crate::result::Status::SystemError,
                        format!("Unknown problem {}", job.problem),
                    )),
                };
                drop(item.lease);
                {
                    let mut busy = shared.busy.lock();
                    if let Some(i) = busy.iter().position(|c| *c == job.contestant) {
                        busy.remove(i);
                    }
                }
                shared.running.fetch_sub(1, Ordering::Relaxed);
                sink(ServiceEvent::Done { id: job.id, result });
            });
        }
    });
    Ok(())
}
