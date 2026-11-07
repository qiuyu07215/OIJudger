//! OIJudger judging engine.

pub mod checker;
pub mod compare;
pub mod compile;
pub mod config;
pub mod discover;
pub mod engine;
pub mod judge;
pub mod model;
pub mod result;
pub mod service;
pub mod workspace;

pub use compile::{detect_compilers, CompilerConfig, Lang};
pub use config::AppSettings;
pub use engine::{Engine, Event, Task};
pub use judge::{source_fingerprint, Settings};
pub use model::{Contest, ContestDir, Problem, ProblemKind};
pub use result::{CaseResult, ProblemResult, Status, SubtaskResult, Verdict};
pub use service::{Job, JudgeService, ServiceEvent, ServiceSink};
