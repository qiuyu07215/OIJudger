//! Judging results.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Verdict {
    /// Accepted.
    AC,
    /// Partially correct.
    PC,
    /// Wrong answer.
    WA,
    /// Time limit exceeded.
    TLE,
    /// Memory limit exceeded.
    MLE,
    /// Output limit exceeded.
    OLE,
    /// Runtime error.
    RE,
    /// Output file not found (file IO problems / answer files).
    NoOutput,
    /// Not run because a dependency or an earlier case already scored 0.
    Skipped,
    /// The checker failed or another judge-side error.
    SystemError,
}

impl Verdict {
    pub fn short(self) -> &'static str {
        match self {
            Verdict::AC => "AC",
            Verdict::PC => "PC",
            Verdict::WA => "WA",
            Verdict::TLE => "TLE",
            Verdict::MLE => "MLE",
            Verdict::OLE => "OLE",
            Verdict::RE => "RE",
            Verdict::NoOutput => "NO",
            Verdict::Skipped => "SKIP",
            Verdict::SystemError => "SE",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CaseResult {
    pub input: String,
    pub verdict: Verdict,
    /// Fraction of the subtask score earned by this case (0..=1).
    pub ratio: f64,
    pub time_ms: u64,
    pub memory_kb: u64,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SubtaskResult {
    pub score: f64,
    pub full: f64,
    pub cases: Vec<CaseResult>,
}

impl SubtaskResult {
    /// The most significant verdict in this subtask.
    pub fn verdict(&self) -> Verdict {
        let mut worst = Verdict::AC;
        for c in &self.cases {
            if c.verdict == Verdict::Skipped {
                continue;
            }
            if c.verdict != Verdict::AC && (worst == Verdict::AC || worst == Verdict::PC) {
                worst = c.verdict;
            }
        }
        if worst == Verdict::AC && self.cases.iter().all(|c| c.verdict == Verdict::Skipped) && !self.cases.is_empty() {
            return Verdict::Skipped;
        }
        worst
    }
}

/// Overall state of one (contestant, problem) pair.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum Status {
    /// Compiled and judged; see subtasks.
    Judged,
    /// No source / answer files found.
    NoSource,
    /// Compilation error.
    CompileError,
    /// The judge itself failed (bad config, missing data, ...).
    SystemError,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ProblemResult {
    pub contestant: String,
    pub problem: String,
    pub status: Status,
    pub score: f64,
    pub full: f64,
    /// Sum of case run times.
    pub time_ms: u64,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub message: String,
    #[serde(default)]
    pub subtasks: Vec<SubtaskResult>,
    /// Fingerprints used to detect stale results.
    #[serde(default)]
    pub source_hash: String,
    #[serde(default)]
    pub problem_hash: String,
    /// Unix seconds.
    #[serde(default)]
    pub judged_at: i64,
}

impl ProblemResult {
    pub fn empty(contestant: &str, problem: &str, full: f64, status: Status, message: impl Into<String>) -> Self {
        ProblemResult {
            contestant: contestant.into(),
            problem: problem.into(),
            status,
            score: 0.0,
            full,
            time_ms: 0,
            message: message.into(),
            subtasks: Vec::new(),
            source_hash: String::new(),
            problem_hash: String::new(),
            judged_at: now(),
        }
    }

    /// Short label for result tables: "AC", "CE", "WA", ...
    pub fn label(&self) -> &'static str {
        match self.status {
            Status::NoSource => "Missing",
            Status::CompileError => "CE",
            Status::SystemError => "SE",
            Status::Judged => {
                if self.score >= self.full && self.full > 0.0 {
                    "AC"
                } else {
                    let mut v = Verdict::AC;
                    for s in &self.subtasks {
                        let sv = s.verdict();
                        if sv != Verdict::AC && sv != Verdict::Skipped {
                            v = sv;
                            break;
                        }
                    }
                    v.short()
                }
            }
        }
    }
}

pub fn now() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0)
}

/// Round a score for display and storage (avoids 99.99999999).
pub fn round_score(x: f64) -> f64 {
    (x * 1e6).round() / 1e6
}
