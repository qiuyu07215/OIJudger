//! Per-worker sandbox identities and scratch directories.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

/// One judging worker: its own AppContainer and scratch directory, so
/// concurrently running programs can never see each other's files.
pub struct Worker {
    index: usize,
    /// Runs contestant programs; may only touch `run/`.
    container: sandbox::Container,
    /// Runs the compiler; may only touch `build/` (and its own TEMP).
    build_container: sandbox::Container,
    root: PathBuf,
    affinity: usize,
}

impl Worker {
    pub fn index(&self) -> usize {
        self.index
    }
    pub fn container(&self) -> &sandbox::Container {
        &self.container
    }
    pub fn build_container(&self) -> &sandbox::Container {
        &self.build_container
    }
    pub fn affinity(&self) -> usize {
        self.affinity
    }
    pub fn root(&self) -> &Path {
        &self.root
    }
    /// Compilation scratch directory.
    pub fn build_dir(&self) -> PathBuf {
        self.root.join("build")
    }
    /// Directory the contestant program runs in.
    pub fn run_dir(&self) -> PathBuf {
        self.root.join("run")
    }
    /// Scratch directory for checkers / interactors.
    pub fn check_dir(&self) -> PathBuf {
        self.root.join("check")
    }

    /// Remove everything in `dir` except the names in `keep`.
    pub fn clean_dir(&self, dir: &Path, keep: &[&str]) -> Result<()> {
        if !dir.exists() {
            std::fs::create_dir_all(dir)?;
            return Ok(());
        }
        for e in std::fs::read_dir(dir)? {
            let e = e?;
            let name = e.file_name();
            if keep.iter().any(|k| name.eq_ignore_ascii_case(k)) {
                continue;
            }
            let p = e.path();
            let ft = e.file_type()?;
            let r = if ft.is_dir() { std::fs::remove_dir_all(&p) } else { remove_file_retry(&p) };
            r.with_context(|| format!("cleaning {}", p.display()))?;
        }
        Ok(())
    }
}

/// A just-terminated process may hold its files for a few milliseconds.
fn remove_file_retry(p: &Path) -> std::io::Result<()> {
    let mut last = None;
    for i in 0..20 {
        match std::fs::remove_file(p) {
            Ok(()) => return Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            Err(e) => {
                // Read-only files created by the program.
                if let Ok(m) = std::fs::metadata(p) {
                    let mut perm = m.permissions();
                    #[allow(clippy::permissions_set_readonly_false)]
                    perm.set_readonly(false);
                    let _ = std::fs::set_permissions(p, perm);
                }
                last = Some(e);
                std::thread::sleep(std::time::Duration::from_millis(5 * (i + 1)));
            }
        }
    }
    Err(last.unwrap())
}

/// All workers of one judging session.
pub struct Workspace {
    pub workers: Vec<Worker>,
    root: PathBuf,
    /// Held open without sharing so other instances know we are alive.
    lock: Option<std::fs::File>,
}

const LOCK_FILE: &str = ".lock";

pub fn work_base() -> PathBuf {
    let base = std::env::var_os("LOCALAPPDATA").map(PathBuf::from).unwrap_or_else(std::env::temp_dir);
    base.join("OIJudger").join("work")
}

/// Default number of workers: physical performance cores on hybrid CPUs,
/// otherwise three quarters of the physical cores (at least one).
pub fn default_worker_count() -> usize {
    let cores = sandbox::cpu_cores();
    if cores.is_empty() {
        return std::thread::available_parallelism().map(|n| (n.get() / 2).max(1)).unwrap_or(1);
    }
    let best = cores.iter().map(|c| c.efficiency_class).max().unwrap_or(0);
    let hybrid = cores.iter().any(|c| c.efficiency_class != best);
    let perf = cores.iter().filter(|c| c.efficiency_class == best).count();
    if hybrid {
        perf.max(1)
    } else {
        (perf * 3 / 4).max(1)
    }
}

impl Workspace {
    /// Create `count` workers (0 = default). Worker `i` is pinned to the
    /// i-th performance core when `pin` is set and enough cores exist.
    pub fn new(count: usize, pin: bool) -> Result<Workspace> {
        let count = if count == 0 { default_worker_count() } else { count };
        let base = work_base();
        cleanup_stale(&base);
        let root = base.join(format!("{}", std::process::id()));
        std::fs::create_dir_all(&root).with_context(|| format!("creating {}", root.display()))?;
        let lock = {
            use std::os::windows::fs::OpenOptionsExt;
            std::fs::OpenOptions::new()
                .write(true)
                .create(true)
                .truncate(true)
                .share_mode(0)
                .open(root.join(LOCK_FILE))?
        };
        let masks = sandbox::performance_core_masks();
        let mut workers = Vec::with_capacity(count);
        for i in 0..count {
            // Per-process names: a second running instance must not share
            // container identities (and thus file access) with this one.
            let pid = std::process::id();
            let container = sandbox::Container::open(&run_profile(pid, i))
                .with_context(|| format!("creating sandbox for worker {i}"))?;
            let build_container = sandbox::Container::open_scratch(&build_profile(pid, i))
                .with_context(|| format!("creating build sandbox for worker {i}"))?;
            let wroot = root.join(format!("w{i}"));
            for d in ["build", "run", "check"] {
                std::fs::create_dir_all(wroot.join(d))?;
            }
            let run = wroot.join("run");
            container.grant_dir(&run).with_context(|| format!("granting {}", run.display()))?;
            let build = wroot.join("build");
            build_container.grant_dir(&build).with_context(|| format!("granting {}", build.display()))?;
            let affinity = if pin && count <= masks.len() { masks[i] } else { 0 };
            workers.push(Worker { index: i, container, build_container, root: wroot, affinity });
        }
        Ok(Workspace { workers, root, lock: Some(lock) })
    }
}

fn run_profile(pid: u32, i: usize) -> String {
    format!("OIJudger.{pid}.W{i}")
}

fn build_profile(pid: u32, i: usize) -> String {
    format!("OIJudger.{pid}.B{i}")
}

fn delete_profiles(pid: u32, count: usize) {
    for i in 0..count {
        sandbox::Container::delete_profile(&run_profile(pid, i));
        sandbox::Container::delete_profile(&build_profile(pid, i));
    }
}

/// Upper bound on workers when cleaning up after a crashed instance.
const MAX_WORKERS: usize = 64;

impl Drop for Workspace {
    fn drop(&mut self) {
        let n = self.workers.len();
        self.workers.clear();
        delete_profiles(std::process::id(), n);
        drop(self.lock.take());
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

/// Remove scratch directories left behind by crashed sessions.
fn cleanup_stale(base: &Path) {
    let Ok(entries) = std::fs::read_dir(base) else { return };
    let me = std::process::id().to_string();
    for e in entries.flatten() {
        if e.file_name().to_string_lossy() == me {
            continue;
        }
        // A live instance keeps its lock file open without sharing, so it
        // cannot be deleted; only then is the directory really stale.
        let lock = e.path().join(LOCK_FILE);
        if lock.exists() && std::fs::remove_file(&lock).is_err() {
            continue;
        }
        let _ = std::fs::remove_dir_all(e.path());
        if let Ok(pid) = e.file_name().to_string_lossy().parse::<u32>() {
            delete_profiles(pid, MAX_WORKERS);
        }
    }
}
