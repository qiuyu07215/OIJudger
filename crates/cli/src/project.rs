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
    /// Open a contest directory. Without `contest.toml`, a LemonLime `.cdf`
    /// in the folder is imported, otherwise problems are detected from
    /// `data/`; the configuration is then written.
    pub fn open(root: &Path) -> Result<Project> {
        // A .cdf file itself may be given: open its folder.
        let root = if root.is_file() && root.extension().is_some_and(|e| e.eq_ignore_ascii_case("cdf")) {
            root.parent().unwrap_or(root)
        } else {
            root
        };
        anyhow::ensure!(root.is_dir(), "{} is not a directory", root.display());
        // Checkers and compilers run in other directories: always absolute.
        let root = &absolute(root);
        let dir = ContestDir::new(root);
        let contest = if root.join(CONTEST_FILE).is_file() {
            Contest::load(root)?
        } else if let Some(cdf) = lemon_import::find_cdf(root) {
            let imported = lemon_import::import_file(&cdf)?;
            imported.contest.save(root)?;
            imported.contest
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

/// Optional contestant metadata for group statistics.
pub const GROUPS_FILE: &str = "contestants.csv";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExportFormat {
    Csv,
    Xlsx,
    Html,
    /// HTML without per-case details (much smaller).
    HtmlBrief,
}

impl ExportFormat {
    pub fn parse(s: &str) -> Option<ExportFormat> {
        match s.to_ascii_lowercase().as_str() {
            "csv" => Some(ExportFormat::Csv),
            "xlsx" | "excel" => Some(ExportFormat::Xlsx),
            "html" => Some(ExportFormat::Html),
            "htm" | "html-brief" => Some(ExportFormat::HtmlBrief),
            _ => None,
        }
    }
    pub fn extension(self) -> &'static str {
        match self {
            ExportFormat::Csv => "csv",
            ExportFormat::Xlsx => "xlsx",
            ExportFormat::Html => "html",
            ExportFormat::HtmlBrief => "htm",
        }
    }
}

impl Project {
    pub fn groups(&self) -> (Vec<String>, analysis::Groups) {
        std::fs::read_to_string(self.root().join(GROUPS_FILE)).map(|t| analysis::parse_groups(&t)).unwrap_or_default()
    }

    pub fn stats(&self) -> Result<analysis::ContestStats> {
        let results = self.store.lock().load_all()?;
        let (_, groups) = self.groups();
        Ok(analysis::compute(&self.contest, &self.contestants, &results, &groups))
    }

    pub fn export(&self, format: ExportFormat, path: &Path, lang: &str) -> Result<()> {
        let results = self.store.lock().load_all()?;
        let (_, groups) = self.groups();
        let stats = analysis::compute(&self.contest, &self.contestants, &results, &groups);
        let d = export::ExportData {
            contest: &self.contest,
            contestants: &self.contestants,
            results: &results,
            stats: &stats,
            lang: export::Lang::from_code(lang),
        };
        match format {
            ExportFormat::Csv => std::fs::write(path, export::to_csv(&d))?,
            ExportFormat::Xlsx => export::to_xlsx(&d, path)?,
            ExportFormat::Html => std::fs::write(path, export::to_html(&d, true))?,
            ExportFormat::HtmlBrief => std::fs::write(path, export::to_html(&d, false))?,
        }
        Ok(())
    }

    /// Source text of every contestant for a problem (first source file).
    pub fn sources(&self, problem: &str) -> Vec<(String, String)> {
        let Some(p) = self.contest.problem(problem) else { return Vec::new() };
        let name = match (&p.kind, &p.communication) {
            (judge_core::ProblemKind::Communication, Some(c)) if !c.sources.is_empty() => c.sources[0].clone(),
            _ => p.source_name().to_string(),
        };
        self.contestants
            .iter()
            .filter_map(|c| {
                let path = judge_core::discover::find_source(&self.dir, c, p, &name)?;
                let data = std::fs::read(&path).ok()?;
                Some((c.clone(), String::from_utf8_lossy(&data).into_owned()))
            })
            .collect()
    }

    pub fn similarity(&self, problem: &str, threshold: f64) -> Vec<analysis::similarity::Pair> {
        let subs = self.sources(problem);
        let opt = analysis::similarity::Options { threshold, ..Default::default() };
        analysis::similarity::compare_all(&subs, &opt)
    }
}
