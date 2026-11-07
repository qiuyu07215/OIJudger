//! Per-worker sandbox identities and scratch directories.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::OnceLock;

use anyhow::{Context, Result};
use parking_lot::Mutex;

/// Delete directories on a background thread. Antivirus scanners can block
/// deleting a freshly written executable for seconds; judging never waits.
pub fn defer_delete(path: PathBuf) {
    static TX: OnceLock<std::sync::Mutex<std::sync::mpsc::Sender<PathBuf>>> = OnceLock::new();
    let tx = TX.get_or_init(|| {
        let (tx, rx) = std::sync::mpsc::channel::<PathBuf>();
        std::thread::Builder::new()
            .name("oij-deleter".into())
            .spawn(move || {
                for p in rx {
                    for i in 0..50u64 {
                        if std::fs::remove_dir_all(&p).is_ok() || !p.exists() {
                            break;
                        }
                        std::thread::sleep(std::time::Duration::from_millis(20 * (i + 1)));
                    }
                }
            })
            .expect("spawn deleter thread");
        std::sync::Mutex::new(tx)
    });
    let _ = tx.lock().map(|t| t.send(path));
}

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
    /// Current per-task subdirectories of `run/` and `build/`.
    run_cur: Mutex<PathBuf>,
    build_cur: Mutex<PathBuf>,
    seq: AtomicU64,
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
    /// Current compilation scratch directory.
    pub fn build_dir(&self) -> PathBuf {
        self.build_cur.lock().clone()
    }
    /// Current directory the contestant program runs in.
    pub fn run_dir(&self) -> PathBuf {
        self.run_cur.lock().clone()
    }
    /// Switch to a new, empty run directory; the previous one is deleted in
    /// the background.
    pub fn fresh_run_dir(&self) -> Result<PathBuf> {
        self.rotate(&self.run_cur, "run", "t")
    }
    /// Switch to a new, empty build directory.
    pub fn fresh_build_dir(&self) -> Result<PathBuf> {
        self.rotate(&self.build_cur, "build", "b")
    }
    fn rotate(&self, cur: &Mutex<PathBuf>, base: &str, prefix: &str) -> Result<PathBuf> {
        let n = self.seq.fetch_add(1, Ordering::Relaxed);
        let dir = self.root.join(base).join(format!("{prefix}{n}"));
        std::fs::create_dir_all(&dir).with_context(|| format!("creating {}", dir.display()))?;
        let old = std::mem::replace(&mut *cur.lock(), dir.clone());
        defer_delete(old);
        Ok(dir)
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

/// Retry a file operation that may fail transiently because an antivirus
/// scanner or a just-terminated process still holds the file.
pub fn retry_io<T>(mut f: impl FnMut() -> std::io::Result<T>) -> std::io::Result<T> {
    let mut delay = 5;
    for _ in 0..8 {
        match f() {
            Ok(v) => return Ok(v),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Err(e),
            Err(_) => {
                std::thread::sleep(std::time::Duration::from_millis(delay));
                delay = (delay * 2).min(200);
            }
        }
    }
    f()
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
    /// Extra workers that only compile (the compile pipeline).
    pub builders: Vec<Worker>,
    root: PathBuf,
    /// Unique per workspace; part of the directory and profile names.
    tag: String,
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

/// Logical processors (group 0) that compilers may use while `judges`
/// pinned judging workers are timing programs: every core except the
/// judging cores, including their SMT siblings (a compiler on the sibling
/// would slow the timed program down). 0 when there is none or the judging
/// workers cannot be pinned.
pub fn spare_cpu_mask(judges: usize) -> usize {
    let masks = sandbox::performance_core_masks();
    if judges > masks.len() {
        return 0;
    }
    let used = &masks[..judges];
    sandbox::cpu_cores()
        .into_iter()
        .filter(|c| c.group == 0 && c.mask & used.iter().fold(0, |a, m| a | m) == 0)
        .fold(0, |a, c| a | c.mask)
}

impl Workspace {
    /// Create `count` judging workers (0 = default) and no compile workers.
    pub fn new(count: usize, pin: bool) -> Result<Workspace> {
        Self::with_builders(count, pin, 0, 0)
    }

    /// `count` judging workers plus `builders` compile workers. Judging
    /// worker `i` is pinned to the i-th performance core when `pin` is set;
    /// compile workers run on `builder_affinity` (0 = any processor).
    pub fn with_builders(count: usize, pin: bool, builders: usize, builder_affinity: usize) -> Result<Workspace> {
        if sandbox::integrity_level().is_some_and(|l| l < sandbox::INTEGRITY_MEDIUM) {
            anyhow::bail!(
                "OIJudger is running at Low integrity, so it cannot set up the judging sandbox. \
                 This happens when the program's folder carries a Low integrity label (common for folders \
                 managed by sandboxing tools). Extract OIJudger into a normal folder and run it from there."
            );
        }
        let count = if count == 0 { default_worker_count() } else { count };
        let base = work_base();
        cleanup_stale(&base);
        // Several workspaces can live in one process (an online contest's
        // judging service and an offline run): each gets its own tag.
        static INSTANCE: AtomicU64 = AtomicU64::new(0);
        let tag = format!("{}-{}", std::process::id(), INSTANCE.fetch_add(1, Ordering::Relaxed));
        let root = base.join(&tag);
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
        let pinned = pin && count <= masks.len();
        let mut workers = Vec::with_capacity(count);
        let mut extra = Vec::with_capacity(builders);
        for i in 0..count + builders {
            let affinity = match (i < count, pinned) {
                (true, true) => masks[i],
                (true, false) => 0,
                (false, _) => builder_affinity,
            };
            let w = Worker::create(&root, &tag, i, affinity)?;
            if i < count {
                workers.push(w);
            } else {
                extra.push(w);
            }
        }
        Ok(Workspace { workers, builders: extra, root, tag, lock: Some(lock) })
    }
}

impl Worker {
    fn create(root: &Path, tag: &str, i: usize, affinity: usize) -> Result<Worker> {
        // Per-workspace names: another workspace (in this or another
        // process) must not share container identities, and thus file
        // access, with this one.
        let container = sandbox::Container::open(&run_profile(tag, i))
            .with_context(|| format!("creating sandbox for worker {i}"))?;
        let build_container = sandbox::Container::open_scratch(&build_profile(tag, i))
            .with_context(|| format!("creating build sandbox for worker {i}"))?;
        let wroot = root.join(format!("w{i}"));
        for d in ["build", "run", "check"] {
            std::fs::create_dir_all(wroot.join(d))?;
        }
        let run = wroot.join("run");
        container.grant_dir(&run).with_context(|| format!("granting {}", run.display()))?;
        let build = wroot.join("build");
        build_container.grant_dir(&build).with_context(|| format!("granting {}", build.display()))?;
        let (run0, build0) = (run.join("t0"), build.join("b0"));
        std::fs::create_dir_all(&run0)?;
        std::fs::create_dir_all(&build0)?;
        Ok(Worker {
            index: i,
            container,
            build_container,
            root: wroot,
            affinity,
            run_cur: Mutex::new(run0),
            build_cur: Mutex::new(build0),
            seq: AtomicU64::new(1),
        })
    }
}

fn run_profile(tag: &str, i: usize) -> String {
    format!("OIJudger.{tag}.W{i}")
}

fn build_profile(tag: &str, i: usize) -> String {
    format!("OIJudger.{tag}.B{i}")
}

fn delete_profiles(tag: &str, count: usize) {
    for i in 0..count {
        sandbox::Container::delete_profile(&run_profile(tag, i));
        sandbox::Container::delete_profile(&build_profile(tag, i));
    }
}

/// Upper bound on workers when cleaning up after a crashed instance.
const MAX_WORKERS: usize = 64;

impl Drop for Workspace {
    fn drop(&mut self) {
        let n = self.workers.len() + self.builders.len();
        self.workers.clear();
        self.builders.clear();
        delete_profiles(&self.tag, n);
        drop(self.lock.take());
        defer_delete(self.root.clone());
    }
}

/// Remove scratch directories left behind by crashed sessions.
fn cleanup_stale(base: &Path) {
    let Ok(entries) = std::fs::read_dir(base) else { return };
    for e in entries.flatten() {
        // A live instance keeps its lock file open without sharing, so it
        // cannot be deleted; only then is the directory really stale.
        let lock = e.path().join(LOCK_FILE);
        if lock.exists() && std::fs::remove_file(&lock).is_err() {
            continue;
        }
        let _ = std::fs::remove_dir_all(e.path());
        // Directory names are the workspace tags ("<pid>-<n>"; older
        // versions used "<pid>"), which are also in the profile names.
        let tag = e.file_name().to_string_lossy().into_owned();
        if tag.chars().all(|c| c.is_ascii_digit() || c == '-') {
            delete_profiles(&tag, MAX_WORKERS);
        }
    }
}
