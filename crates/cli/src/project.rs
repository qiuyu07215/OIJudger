//! An opened contest: configuration, contestants and stored results.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;

use anyhow::{Context, Result};
use judge_core::discover::{detect_problems, list_contestants};
use judge_core::model::CONTEST_FILE;
use judge_core::{AppSettings, Contest, ContestDir, Engine, Event, ProblemResult, Task};
use parking_lot::Mutex;
use store::{Store, Summary};

pub struct Project {
    pub dir: ContestDir,
    pub contest: Contest,
    pub contestants: Vec<String>,
    pub store: Mutex<Store>,
}

/// Which tasks to judge.
#[derive(Debug, Clone, Default)]
pub struct Selection {
    /// Empty = all problems.
    pub problems: Vec<String>,
    /// Empty = all contestants.
    pub contestants: Vec<String>,
    /// Only tasks without an up-to-date result.
    pub only_stale: bool,
}

impl Project {
    /// Open a contest directory. Without `contest.toml`, problems are detected
    /// from `data/` and the configuration is written.
    pub fn open(root: &Path) -> Result<Project> {
        anyhow::ensure!(root.is_dir(), "{} is not a directory", root.display());
        // Checkers and compilers run in other directories: always absolute.
        let root = &absolute(root);
        let dir = ContestDir::new(root);
        let contest = if root.join(CONTEST_FILE).is_file() {
            Contest::load(root)?
        } else {
            let c = Contest {
                title: root.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default(),
                problems: detect_problems(&dir, 1000, 512),
            };
            std::fs::create_dir_all(dir.source()).ok();
            std::fs::create_dir_all(dir.data()).ok();
            c.save(root)?;
            c
        };
        let store = Store::open(&dir.results_db()).context("opening results database")?;
        let contestants = list_contestants(&dir);
        Ok(Project { dir, contest, contestants, store: Mutex::new(store) })
    }

    pub fn root(&self) -> &Path {
        &self.dir.root
    }

    pub fn reload(&mut self) -> Result<()> {
        if self.root().join(CONTEST_FILE).is_file() {
            self.contest = Contest::load(&self.dir.root)?;
        }
        self.contestants = list_contestants(&self.dir);
        Ok(())
    }

    pub fn save_contest(&self) -> Result<()> {
        self.contest.save(&self.dir.root)
    }

    pub fn engine(&self, settings: &AppSettings) -> Engine {
        Engine::new(self.dir.clone(), self.contest.clone(), settings.judge_settings(), settings.effective_compilers())
    }

    pub fn summaries(&self) -> Result<Vec<Summary>> {
        self.store.lock().summaries()
    }

    pub fn result(&self, contestant: &str, problem: &str) -> Result<Option<ProblemResult>> {
        self.store.lock().get(contestant, problem)
    }

    /// Tasks matching `sel`. Staleness compares stored fingerprints with the
    /// current sources, data and settings.
    pub fn tasks(&self, engine: &Engine, sel: &Selection) -> Result<Vec<Task>> {
        let stored: HashMap<(String, String), Summary> = if sel.only_stale {
            self.summaries()?.into_iter().map(|s| ((s.contestant.clone(), s.problem.clone()), s)).collect()
        } else {
            HashMap::new()
        };
        let hashes = if sel.only_stale { engine.problem_hashes() } else { HashMap::new() };
        let mut out = Vec::new();
        for c in &self.contestants {
            if !sel.contestants.is_empty() && !sel.contestants.contains(c) {
                continue;
            }
            for p in &self.contest.problems {
                if !sel.problems.is_empty() && !sel.problems.contains(&p.name) {
                    continue;
                }
                if sel.only_stale {
                    if let Some(s) = stored.get(&(c.clone(), p.name.clone())) {
                        let fresh = hashes.get(&p.name) == Some(&s.problem_hash)
                            && judge_core::source_fingerprint(&self.dir, c, p) == s.source_hash;
                        if fresh {
                            continue;
                        }
                    }
                }
                out.push(Task { contestant: c.clone(), problem: p.name.clone() });
            }
        }
        Ok(out)
    }

    /// Judge the selected tasks, saving every result as it finishes.
    pub fn judge(
        &self,
        settings: &AppSettings,
        sel: &Selection,
        sink: &(dyn Fn(&Event) + Sync),
        cancel: &AtomicBool,
    ) -> Result<usize> {
        let engine = self.engine(settings);
        anyhow::ensure!(
            !engine.tools.compilers.is_empty()
                || self.contest.problems.iter().all(|p| p.kind == judge_core::ProblemKind::Answer),
            "No C/C++ compiler found. Install MinGW (g++) or set the compiler path in settings."
        );
        let tasks = self.tasks(&engine, sel)?;
        let n = tasks.len();
        let save_err: Mutex<Option<anyhow::Error>> = Mutex::new(None);
        engine.run(
            tasks,
            &|ev: Event| {
                if let Event::TaskDone { result, .. } = &ev {
                    if let Err(e) = self.store.lock().save(result) {
                        save_err.lock().get_or_insert(e);
                    }
                }
                sink(&ev);
            },
            cancel,
        )?;
        if let Some(e) = save_err.into_inner() {
            return Err(e.context("saving results"));
        }
        Ok(n)
    }

    /// Total score per contestant, ranked (ties share a rank).
    pub fn ranking(&self) -> Result<Vec<RankRow>> {
        let sums = self.summaries()?;
        let mut by: HashMap<&str, HashMap<&str, &Summary>> = HashMap::new();
        for s in &sums {
            by.entry(s.contestant.as_str()).or_default().insert(s.problem.as_str(), s);
        }
        let mut rows: Vec<RankRow> = self
            .contestants
            .iter()
            .map(|c| {
                let m = by.get(c.as_str());
                let cells: Vec<Option<Summary>> = self
                    .contest
                    .problems
                    .iter()
                    .map(|p| m.and_then(|m| m.get(p.name.as_str())).map(|s| (*s).clone()))
                    .collect();
                let total = cells.iter().flatten().map(|s| s.score).sum::<f64>();
                let time = cells.iter().flatten().map(|s| s.time_ms).sum::<u64>();
                RankRow { rank: 0, contestant: c.clone(), total, time_ms: time, cells }
            })
            .collect();
        rows.sort_by(|a, b| {
            b.total
                .partial_cmp(&a.total)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| judge_core::discover::natural_cmp(&a.contestant, &b.contestant))
        });
        for i in 0..rows.len() {
            rows[i].rank =
                if i > 0 && (rows[i].total - rows[i - 1].total).abs() < 1e-9 { rows[i - 1].rank } else { i + 1 };
        }
        Ok(rows)
    }
}

#[derive(Debug, Clone)]
pub struct RankRow {
    pub rank: usize,
    pub contestant: String,
    pub total: f64,
    pub time_ms: u64,
    /// One entry per problem, in contest order.
    pub cells: Vec<Option<Summary>>,
}

pub fn fmt_score(x: f64) -> String {
    if (x - x.round()).abs() < 1e-9 {
        format!("{}", x.round() as i64)
    } else {
        let s = format!("{x:.2}");
        s.trim_end_matches('0').trim_end_matches('.').to_string()
    }
}

pub fn absolute(p: &Path) -> PathBuf {
    std::fs::canonicalize(p)
        .map(|p| {
            // Strip the \\?\ prefix for friendlier display.
            let s = p.to_string_lossy();
            PathBuf::from(s.strip_prefix(r"\\?\").unwrap_or(&s).to_string())
        })
        .unwrap_or_else(|_| p.to_path_buf())
}
