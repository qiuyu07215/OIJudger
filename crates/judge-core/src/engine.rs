//! Parallel judging of many (contestant, problem) tasks.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::OnceLock;

use anyhow::{Context, Result};

use crate::compile::{build, BuildOutput, BuildRequest, CompilerConfig, Lang};
use crate::judge::{judge_problem, JudgeContext, ProblemTools, Settings, Toolset};
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
    /// Informational message (setup steps, warnings).
    Message(String),
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
    fn prepare_toolchains(&self, sink: &(dyn Fn(Event) + Sync)) -> Result<()> {
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

    fn testlib_path(&self) -> Result<PathBuf> {
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
            Ok(BuildOutput::Ok { exe, .. }) => Ok(exe),
            Ok(BuildOutput::Failed { log }) => Err(format!("{rel} failed to compile:\n{log}")),
            Err(e) => Err(format!("{rel}: {e:#}")),
        }
    }

    fn problem_tools(
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
        let ws = Workspace::new(want.min(tasks.len()).max(1), self.settings.pin_cores)
            .context("setting up sandbox workers")?;

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
        sink(Event::Started { total: tasks.len(), workers: ws.workers.len() });

        let (tx, rx) = crossbeam_channel::unbounded::<Task>();
        for t in tasks {
            tx.send(t).unwrap();
        }
        drop(tx);

        std::thread::scope(|s| {
            for worker in &ws.workers {
                let rx = rx.clone();
                let ctx = &ctx;
                s.spawn(move || {
                    while let Ok(task) = rx.recv() {
                        if cancel.load(Ordering::Relaxed) {
                            break;
                        }
                        let Some(problem) = self.contest.problem(&task.problem) else { continue };
                        sink(Event::TaskStarted { worker: worker.index(), task: task.clone() });
                        let mut on_case = |si: usize, c: &CaseResult| {
                            sink(Event::Case {
                                worker: worker.index(),
                                task: task.clone(),
                                subtask: si,
                                case: c.clone(),
                            })
                        };
                        if let Some(result) = judge_problem(ctx, worker, &task.contestant, problem, &mut on_case) {
                            sink(Event::TaskDone { worker: worker.index(), result });
                        }
                    }
                });
            }
        });
        Ok(())
    }
}
