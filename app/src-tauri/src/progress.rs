//! Judging progress, batched for the UI: the engine may finish thousands of
//! cases per second, so events are aggregated and emitted at most every
//! 100 ms.

use std::collections::BTreeMap;
use std::time::Instant;

use judge_core::{Event, ProblemResult};
use serde::Serialize;

use crate::commands::CellView;

#[derive(Debug, Clone, Serialize, Default)]
pub struct WorkerView {
    pub contestant: String,
    pub problem: String,
    pub cases_done: usize,
}

/// One line of the judging log (like LemonLime's judging dialog).
#[derive(Debug, Clone, Serialize)]
pub struct LogLine {
    pub contestant: String,
    pub problem: String,
    /// 1-based test point, 0 for the per-task summary line.
    pub subtask: usize,
    pub input: String,
    /// Case verdict, or the task label for summary lines.
    pub verdict: String,
    pub time_ms: u64,
    pub memory_kb: u64,
    /// Summary lines: score / full.
    pub score: Option<(f64, f64)>,
    pub message: String,
}

/// Keep at most this many unsent log lines between emissions.
const LOG_BACKLOG: usize = 400;

#[derive(Debug, Clone, Serialize, Default)]
pub struct ProgressView {
    pub running: bool,
    /// Compile phase: programs prepared / to prepare.
    pub compile_done: usize,
    pub compile_total: usize,
    pub total: usize,
    pub done: usize,
    pub workers: Vec<Option<WorkerView>>,
    /// Finished tasks by label (AC, WA, CE, ...).
    pub counts: BTreeMap<String, usize>,
    pub elapsed_ms: u64,
    /// Cells finished since the previous emission.
    pub updates: Vec<CellView>,
    pub messages: Vec<String>,
    /// Log lines since the previous emission.
    pub log: Vec<LogLine>,
    /// Set when judging ended with an error.
    pub error: Option<String>,
}

pub struct Progress {
    pub view: ProgressView,
    pub started: Instant,
    pub dirty: bool,
}

impl Progress {
    pub fn new() -> Progress {
        Progress { view: ProgressView { running: true, ..Default::default() }, started: Instant::now(), dirty: true }
    }

    pub fn on_event(&mut self, ev: &Event) {
        self.dirty = true;
        match ev {
            Event::Compiled { done, total } => {
                self.view.compile_done = self.view.compile_done.max(*done);
                self.view.compile_total = *total;
            }
            Event::Started { total, workers } => {
                self.view.total = *total;
                self.view.workers = vec![None; *workers];
            }
            Event::Message(m) => self.view.messages.push(m.clone()),
            Event::TaskStarted { worker, task } => {
                if let Some(w) = self.view.workers.get_mut(*worker) {
                    *w = Some(WorkerView {
                        contestant: task.contestant.clone(),
                        problem: task.problem.clone(),
                        cases_done: 0,
                    });
                }
            }
            Event::Case { worker, task, subtask, case } => {
                if let Some(Some(w)) = self.view.workers.get_mut(*worker) {
                    w.cases_done += 1;
                }
                self.push_log(LogLine {
                    contestant: task.contestant.clone(),
                    problem: task.problem.clone(),
                    subtask: subtask + 1,
                    input: case.input.clone(),
                    verdict: case.verdict.short().to_string(),
                    time_ms: case.time_ms,
                    memory_kb: case.memory_kb,
                    score: None,
                    message: case.message.lines().next().unwrap_or("").chars().take(160).collect(),
                });
            }
            Event::TaskDone { worker, result } => {
                self.view.done += 1;
                *self.view.counts.entry(result.label().to_string()).or_default() += 1;
                if let Some(w) = self.view.workers.get_mut(*worker) {
                    *w = None;
                }
                self.view.updates.push(cell_from_result(result));
                self.push_log(LogLine {
                    contestant: result.contestant.clone(),
                    problem: result.problem.clone(),
                    subtask: 0,
                    input: String::new(),
                    verdict: result.label().to_string(),
                    time_ms: result.time_ms,
                    memory_kb: 0,
                    score: Some((result.score, result.full)),
                    message: if result.status == judge_core::Status::Judged {
                        String::new()
                    } else {
                        result.message.lines().next().unwrap_or("").chars().take(160).collect()
                    },
                });
            }
        }
    }

    fn push_log(&mut self, l: LogLine) {
        if self.view.log.len() >= LOG_BACKLOG {
            self.view.log.remove(0);
        }
        self.view.log.push(l);
    }

    /// Snapshot for the UI; clears the incremental parts.
    pub fn take(&mut self) -> ProgressView {
        self.dirty = false;
        self.view.elapsed_ms = self.started.elapsed().as_millis() as u64;
        let v = self.view.clone();
        self.view.updates.clear();
        self.view.messages.clear();
        self.view.log.clear();
        v
    }
}

pub fn cell_from_result(r: &ProblemResult) -> CellView {
    CellView {
        contestant: r.contestant.clone(),
        problem: r.problem.clone(),
        score: r.score,
        full: r.full,
        label: r.label().to_string(),
        status: format!("{:?}", r.status),
        time_ms: r.time_ms,
        judged_at: r.judged_at,
        stale: false,
    }
}
