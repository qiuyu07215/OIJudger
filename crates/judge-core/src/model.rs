//! Contest configuration (`contest.toml`).
//!
//! Directory layout follows LemonLime:
//!
//! ```text
//! <contest>/contest.toml
//! <contest>/data/...                      test data, checkers, graders
//! <contest>/source/<contestant>/[<problem>/]<source>.cpp
//! <contest>/.oij/                         results database, build cache
//! ```

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

pub const CONTEST_FILE: &str = "contest.toml";

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Contest {
    #[serde(default)]
    pub title: String,
    #[serde(default, rename = "problem")]
    pub problems: Vec<Problem>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum ProblemKind {
    /// Compile and run against test data.
    #[default]
    Traditional,
    /// Contestants submit output files directly.
    Answer,
    /// NOI style: contestant code is compiled together with a grader and an
    /// interaction library header; or, with `interactor`, a testlib
    /// interactor runs as a second process connected through pipes.
    Interactive,
    /// Several contestant source files and interface files compiled together.
    Communication,
}

/// Where to look for a contestant's source file.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum Subfolder {
    /// `source/<c>/<problem>/<src>` first, then `source/<c>/<src>`.
    #[default]
    Auto,
    /// Only `source/<c>/<problem>/<src>`.
    Inside,
    /// Only `source/<c>/<src>`.
    Outside,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum Comparator {
    /// Line by line; trailing whitespace on each line and trailing blank lines
    /// are ignored (LemonLime's default mode).
    #[default]
    Lines,
    /// Line by line, exact (line endings normalised).
    Strict,
    /// Whitespace separated tokens.
    Tokens,
    /// Tokens; numbers compared with absolute / relative error `real_eps`.
    Real,
    /// Byte for byte.
    Binary,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum CheckerKind {
    /// `checker <input> <output> <answer>`; verdict from exit code, partial
    /// score from `quitp` / `_pc`.
    #[default]
    Testlib,
    /// LemonLime / Cena style:
    /// `checker <input> <output> <answer> <full score> <score file> <message file>`.
    Lemon,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Checker {
    /// Path relative to `data/`; `.cpp` files are compiled (with testlib.h).
    pub path: String,
    #[serde(default)]
    pub kind: CheckerKind,
}

/// NOI style interaction library compiled into the contestant program.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct Grader {
    /// Header file in `data/` (copied as `header_name`).
    #[serde(default)]
    pub header_path: String,
    /// Name the contestant code includes, e.g. `"interaction.h"`.
    #[serde(default)]
    pub header_name: String,
    /// Grader implementation (`grader.cpp`) in `data/`.
    #[serde(default)]
    pub grader_path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct Communication {
    /// Contestant source files (names, looked up like a normal source).
    #[serde(default)]
    pub sources: Vec<String>,
    /// Interface files from `data/` compiled alongside.
    #[serde(default)]
    pub interfaces: Vec<String>,
    /// LemonLime "CommunicationExec": every source becomes its own program,
    /// `grader.*` from `interfaces` is compiled separately and run; it starts
    /// the contestant programs itself (all inside the same sandbox).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub multi_program: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Case {
    /// Input file, relative to `data/`.
    pub input: String,
    /// Answer file, relative to `data/`.
    pub output: String,
}

/// LemonLime calls this a "测试点": a group of cases sharing score and
/// limits; its score is the minimum over its cases.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Subtask {
    pub score: f64,
    pub time_ms: u64,
    pub memory_mb: u64,
    #[serde(default)]
    pub cases: Vec<Case>,
    /// 1-based indices of earlier subtasks this one depends on.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub depends: Vec<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Problem {
    /// Identifier: default data directory, source name and file names.
    pub name: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub title: String,
    #[serde(default)]
    pub kind: ProblemKind,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub source_name: String,
    #[serde(default)]
    pub subfolder: Subfolder,
    /// Read stdin / write stdout instead of files.
    #[serde(default)]
    pub stdio: bool,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub input_file: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub output_file: String,
    #[serde(default)]
    pub comparator: Comparator,
    #[serde(default = "default_eps")]
    pub real_eps: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub checker: Option<Checker>,
    /// Extension of submitted answer files (answer problems).
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub answer_ext: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub grader: Option<Grader>,
    /// testlib interactor in `data/` (two-process interactive problems).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub interactor: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub communication: Option<Communication>,
    /// Extra compiler arguments for this problem (appended).
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub compile_args: String,
    #[serde(default, rename = "subtask")]
    pub subtasks: Vec<Subtask>,
}

fn default_eps() -> f64 {
    1e-6
}

impl Problem {
    pub fn new(name: &str) -> Problem {
        Problem {
            name: name.to_string(),
            title: String::new(),
            kind: ProblemKind::Traditional,
            source_name: String::new(),
            subfolder: Subfolder::Auto,
            stdio: false,
            input_file: String::new(),
            output_file: String::new(),
            comparator: Comparator::Lines,
            real_eps: default_eps(),
            checker: None,
            answer_ext: String::new(),
            grader: None,
            interactor: None,
            communication: None,
            compile_args: String::new(),
            subtasks: Vec::new(),
        }
    }
    pub fn display_title(&self) -> &str {
        if self.title.is_empty() {
            &self.name
        } else {
            &self.title
        }
    }
    pub fn source_name(&self) -> &str {
        if self.source_name.is_empty() {
            &self.name
        } else {
            &self.source_name
        }
    }
    pub fn input_file(&self) -> String {
        if self.input_file.is_empty() {
            format!("{}.in", self.name)
        } else {
            self.input_file.clone()
        }
    }
    pub fn output_file(&self) -> String {
        if self.output_file.is_empty() {
            format!("{}.out", self.name)
        } else {
            self.output_file.clone()
        }
    }
    pub fn answer_ext(&self) -> &str {
        if self.answer_ext.is_empty() {
            "out"
        } else {
            self.answer_ext.trim_start_matches('.')
        }
    }
    pub fn full_score(&self) -> f64 {
        self.subtasks.iter().map(|s| s.score).sum()
    }
    pub fn max_memory_mb(&self) -> u64 {
        self.subtasks.iter().map(|s| s.memory_mb).max().unwrap_or(256)
    }
    pub fn case_count(&self) -> usize {
        self.subtasks.iter().map(|s| s.cases.len()).sum()
    }
}

impl Contest {
    pub fn load(dir: &Path) -> Result<Contest> {
        let path = dir.join(CONTEST_FILE);
        let text = std::fs::read_to_string(&path).with_context(|| format!("reading {}", path.display()))?;
        let c: Contest = toml::from_str(&text).with_context(|| format!("parsing {}", path.display()))?;
        c.validate()?;
        Ok(c)
    }

    pub fn save(&self, dir: &Path) -> Result<()> {
        let text = toml::to_string_pretty(self)?;
        let path = dir.join(CONTEST_FILE);
        let tmp = dir.join(".contest.toml.tmp");
        std::fs::write(&tmp, text).with_context(|| format!("writing {}", tmp.display()))?;
        std::fs::rename(&tmp, &path).with_context(|| format!("writing {}", path.display()))?;
        Ok(())
    }

    pub fn validate(&self) -> Result<()> {
        let mut seen = std::collections::HashSet::new();
        for p in &self.problems {
            anyhow::ensure!(!p.name.trim().is_empty(), "problem with empty name");
            anyhow::ensure!(seen.insert(p.name.to_lowercase()), "duplicate problem name {}", p.name);
            for (i, s) in p.subtasks.iter().enumerate() {
                for &d in &s.depends {
                    anyhow::ensure!(
                        d >= 1 && d <= i,
                        "problem {}: subtask {} depends on {}, which is not an earlier subtask",
                        p.name,
                        i + 1,
                        d
                    );
                }
            }
        }
        Ok(())
    }

    pub fn problem(&self, name: &str) -> Option<&Problem> {
        self.problems.iter().find(|p| p.name == name)
    }

    pub fn full_score(&self) -> f64 {
        self.problems.iter().map(|p| p.full_score()).sum()
    }
}

/// Paths of a contest directory.
#[derive(Debug, Clone)]
pub struct ContestDir {
    pub root: PathBuf,
}

impl ContestDir {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        ContestDir { root: root.into() }
    }
    pub fn data(&self) -> PathBuf {
        self.root.join("data")
    }
    pub fn source(&self) -> PathBuf {
        self.root.join("source")
    }
    pub fn state(&self) -> PathBuf {
        self.root.join(".oij")
    }
    pub fn build_cache(&self) -> PathBuf {
        self.state().join("build")
    }
    pub fn results_db(&self) -> PathBuf {
        self.state().join("results.db")
    }
    pub fn data_file(&self, rel: &str) -> PathBuf {
        self.data().join(rel)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_toml() {
        let text = r#"
title = "Demo"

[[problem]]
name = "sum"
stdio = true

[[problem.subtask]]
score = 50
time_ms = 1000
memory_mb = 256
cases = [{ input = "sum/1.in", output = "sum/1.out" }]

[[problem.subtask]]
score = 50
time_ms = 1000
memory_mb = 256
depends = [1]
cases = [{ input = "sum/2.in", output = "sum/2.out" }]
"#;
        let c: Contest = toml::from_str(text).unwrap();
        c.validate().unwrap();
        assert_eq!(c.problems[0].full_score(), 100.0);
        assert_eq!(c.problems[0].input_file(), "sum.in");
        let back: Contest = toml::from_str(&toml::to_string_pretty(&c).unwrap()).unwrap();
        assert_eq!(back.problems[0], c.problems[0]);
    }

    #[test]
    fn rejects_forward_dependency() {
        let mut c = Contest::default();
        let mut p = Problem::new("a");
        p.subtasks.push(Subtask { score: 10.0, time_ms: 1000, memory_mb: 256, cases: vec![], depends: vec![1] });
        c.problems.push(p);
        assert!(c.validate().is_err());
    }
}
