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

#[derive(Debug, Clone, Serialize, Default)]
pub struct ProgressView {
    pub running: bool,
    pub total: usize,
    pub done: usize,
    pub workers: Vec<Option<WorkerView>>,
    /// Finished tasks by label (AC, WA, CE, ...).
    pub counts: BTreeMap<String, usize>,
    pub elapsed_ms: u64,
    /// Cells finished since the previous emission.
    pub updates: Vec<CellView>,
    pub messages: Vec<String>,
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
            Event::Case { worker, .. } => {
                if let Some(Some(w)) = self.view.workers.get_mut(*worker) {
                    w.cases_done += 1;
                }
            }
            Event::TaskDone { worker, result } => {
                self.view.done += 1;
                *self.view.counts.entry(result.label().to_string()).or_default() += 1;
                if let Some(w) = self.view.workers.get_mut(*worker) {
                    *w = None;
                }
                self.view.updates.push(cell_from_result(result));
            }
        }
    }

    /// Snapshot for the UI; clears the incremental parts.
    pub fn take(&mut self) -> ProgressView {
        self.dirty = false;
        self.view.elapsed_ms = self.started.elapsed().as_millis() as u64;
        let v = self.view.clone();
        self.view.updates.clear();
        self.view.messages.clear();
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
        stale: false,
    }
}
