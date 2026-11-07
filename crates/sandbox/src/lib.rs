//! OIJudger process sandbox.
//!
//! On Windows every untrusted program (contestant code, compilers working on
//! contestant code, checkers) runs inside:
//!
//! * an **AppContainer** with *no* capabilities — the kernel refuses every
//!   network operation (including loopback) and the process can only open
//!   files whose ACL explicitly grants its container SID;
//! * a **Job Object** that enforces the memory limit in the kernel, caps the
//!   number of processes, applies UI restrictions (no clipboard, desktop
//!   switching, global atoms, ...) and kills everything when the run ends.
//!
//! CPU time is read from job accounting (user + kernel), wall time is a
//! backstop against programs that sleep or block.

mod pe;
mod status;

#[cfg(windows)]
mod win;

use std::ffi::OsString;
use std::fs::File;
use std::path::{Path, PathBuf};

pub use pe::set_stack_reserve;
pub use win::acl::reset_inherited as reset_acl;
pub use status::describe_exit_code;

#[cfg(windows)]
pub use win::cpu::{cores as cpu_cores, performance_core_masks, Core as CpuCore};
#[cfg(windows)]
pub use win::{init_process, integrity_level, pipe, Child, Container, Toolchain, INTEGRITY_MEDIUM};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("{context}: {source}")]
    Io {
        context: String,
        #[source]
        source: std::io::Error,
    },
    #[error("{0}")]
    Other(String),
}

impl Error {
    pub(crate) fn last_os(context: impl Into<String>) -> Self {
        Error::Io { context: context.into(), source: std::io::Error::last_os_error() }
    }
    pub(crate) fn os(context: impl Into<String>, code: u32) -> Self {
        Error::Io { context: context.into(), source: std::io::Error::from_raw_os_error(code as i32) }
    }
}

pub type Result<T> = std::result::Result<T, Error>;

/// Resource limits for one run.
#[derive(Debug, Clone)]
pub struct Limits {
    /// CPU time (user + kernel) in milliseconds.
    pub cpu_ms: u64,
    /// Wall clock time in milliseconds.
    pub wall_ms: u64,
    /// Committed memory limit in bytes; 0 = unlimited.
    pub memory_bytes: u64,
    /// Apply `memory_bytes` to the whole job (all processes together)
    /// instead of each process. Used for compilers.
    pub memory_per_job: bool,
    /// Maximum number of simultaneously alive processes (1 = no children).
    pub max_processes: u32,
    /// Files whose combined size may not exceed `output_bytes`.
    pub watch_files: Vec<PathBuf>,
    /// Output size limit in bytes; 0 = unlimited.
    pub output_bytes: u64,
    /// Restrict the job to these logical processors (group 0), 0 = any.
    pub affinity: usize,
}

impl Limits {
    /// Typical limits for a contestant program: wall = 2 × cpu + 1s.
    pub fn program(cpu_ms: u64, memory_bytes: u64) -> Self {
        Limits {
            cpu_ms,
            wall_ms: cpu_ms * 2 + 1000,
            memory_bytes,
            memory_per_job: false,
            max_processes: 1,
            watch_files: Vec::new(),
            output_bytes: 0,
            affinity: 0,
        }
    }
}

/// Where a standard stream of the child goes.
#[derive(Debug)]
pub enum Stdio {
    /// The NUL device.
    Null,
    /// An already opened file or pipe end. Read-only handles are fine for
    /// stdin: the child gets the handle, not the path, so files outside the
    /// container (e.g. test data) can be fed without copying.
    File(File),
}

/// Everything needed to start one sandboxed process.
#[derive(Debug)]
pub struct RunSpec {
    pub program: PathBuf,
    pub args: Vec<OsString>,
    pub cwd: PathBuf,
    /// Extra environment variables. A minimal base (SystemRoot, TEMP=cwd, …)
    /// is always provided; the parent's environment is never inherited.
    pub env: Vec<(OsString, OsString)>,
    pub stdin: Stdio,
    pub stdout: Stdio,
    pub stderr: Stdio,
    pub limits: Limits,
}

impl RunSpec {
    pub fn new(program: impl AsRef<Path>, cwd: impl AsRef<Path>, limits: Limits) -> Self {
        RunSpec {
            program: program.as_ref().to_path_buf(),
            args: Vec::new(),
            cwd: cwd.as_ref().to_path_buf(),
            env: Vec::new(),
            stdin: Stdio::Null,
            stdout: Stdio::Null,
            stderr: Stdio::Null,
            limits,
        }
    }
    pub fn arg(mut self, a: impl Into<OsString>) -> Self {
        self.args.push(a.into());
        self
    }
    pub fn env(mut self, k: impl Into<OsString>, v: impl Into<OsString>) -> Self {
        self.env.push((k.into(), v.into()));
        self
    }
}

/// How a sandboxed run ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    /// Exited normally with code 0.
    Ok,
    /// Exited with a non-zero code that does not look like a crash.
    NonZeroExit,
    /// Terminated by an exception / NTSTATUS failure code.
    Crashed,
    /// CPU time exceeded.
    TimeLimit,
    /// Wall clock exceeded while CPU time stayed below the limit (sleeping,
    /// blocked on input, deadlocked interaction, ...).
    IdleLimit,
    MemoryLimit,
    OutputLimit,
    /// Tried to create more processes than allowed.
    ProcessLimit,
}

#[derive(Debug, Clone)]
pub struct RunResult {
    pub outcome: Outcome,
    pub exit_code: u32,
    pub cpu_ms: u64,
    pub wall_ms: u64,
    pub peak_memory_bytes: u64,
}

impl RunResult {
    pub fn describe(&self) -> String {
        match self.outcome {
            Outcome::Ok => "OK".into(),
            Outcome::NonZeroExit | Outcome::Crashed => describe_exit_code(self.exit_code),
            Outcome::TimeLimit => "Time limit exceeded".into(),
            Outcome::IdleLimit => "Wall time limit exceeded (idle / blocked)".into(),
            Outcome::MemoryLimit => "Memory limit exceeded".into(),
            Outcome::OutputLimit => "Output limit exceeded".into(),
            Outcome::ProcessLimit => "Tried to create a child process".into(),
        }
    }
}
