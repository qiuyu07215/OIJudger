//! Command line interface (`oijudger <command>`).

use std::io::Write;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::time::Instant;

use anyhow::Result;
use clap::{Parser, Subcommand};
use judge_core::{AppSettings, Event};

use crate::project::{absolute, fmt_score, Project, Selection};

#[derive(Parser, Debug)]
#[command(name = "oijudger", version, about = "OIJudger — sandboxed local judge for OI contests")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Subcommand, Debug)]
pub enum Command {
    /// Create contest.toml by detecting problems in <DIR>/data
    Init {
        dir: PathBuf,
        /// Default time limit (ms)
        #[arg(long, default_value_t = 1000)]
        time: u64,
        /// Default memory limit (MiB)
        #[arg(long, default_value_t = 512)]
        memory: u64,
        /// Overwrite an existing contest.toml
        #[arg(long)]
        force: bool,
    },
    /// Judge submissions (by default only those without an up-to-date result)
    Judge {
        dir: PathBuf,
        /// Only these problems
        #[arg(short, long = "problem")]
        problems: Vec<String>,
        /// Only these contestants
        #[arg(short, long = "contestant")]
        contestants: Vec<String>,
        /// Re-judge everything, even up-to-date results
        #[arg(long)]
        all: bool,
        /// Number of parallel workers (0 = automatic)
        #[arg(short = 'j', long)]
        jobs: Option<usize>,
        /// Do not pin workers to CPU cores
        #[arg(long)]
        no_pin: bool,
        /// Print every finished task
        #[arg(short, long)]
        verbose: bool,
    },
    /// Print the ranking table
    Results {
        dir: PathBuf,
        /// Show per-case details for one contestant
        #[arg(long)]
        contestant: Option<String>,
    },
    /// List detected compilers
    Compilers,
    /// Verify sandbox isolation and limits on this machine
    Selftest,
}

pub fn run(cli: Cli) -> Result<i32> {
    sandbox::init_process();
    let settings = AppSettings::load();
    match cli.command {
        Command::Init { dir, time, memory, force } => {
            let cd = judge_core::ContestDir::new(&dir);
            let file = dir.join(judge_core::model::CONTEST_FILE);
            anyhow::ensure!(force || !file.exists(), "{} exists (use --force)", file.display());
            let c = judge_core::Contest {
                title: absolute(&dir).file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default(),
                problems: judge_core::discover::detect_problems(&cd, time, memory),
            };
            std::fs::create_dir_all(cd.source())?;
            c.save(&dir)?;
            println!("Detected {} problem(s):", c.problems.len());
            for p in &c.problems {
                println!(
                    "  {:<16} {:>3} cases{}",
                    p.name,
                    p.case_count(),
                    if p.checker.is_some() { ", checker" } else { "" }
                );
            }
            println!("Contestants: {}", judge_core::discover::list_contestants(&cd).len());
            Ok(0)
        }
        Command::Judge { dir, problems, contestants, all, jobs, no_pin, verbose } => {
            let mut settings = settings;
            if let Some(j) = jobs {
                settings.workers = j;
            }
            if no_pin {
                settings.pin_cores = false;
            }
            let project = Project::open(&dir)?;
            let sel = Selection { problems, contestants, only_stale: !all };
            judge(&project, &settings, &sel, verbose)?;
            print_ranking(&project)?;
            Ok(0)
        }
        Command::Results { dir, contestant } => {
            let project = Project::open(&dir)?;
            match contestant {
                None => print_ranking(&project)?,
                Some(c) => print_details(&project, &c)?,
            }
            Ok(0)
        }
        Command::Compilers => {
            let cs = settings.effective_compilers();
            if cs.is_empty() {
                println!("No compiler found.");
                return Ok(1);
            }
            for c in cs {
                println!("{:?}\t{}\t{}\n\t{}", c.lang, c.path.display(), c.version(), c.args);
            }
            Ok(0)
        }
        Command::Selftest => {
            println!("Sandbox self test");
            let ok = crate::selftest::run(&settings, &mut |r| {
                println!("  [{}] {:<52} {}", if r.passed { "PASS" } else { "FAIL" }, r.name, r.detail);
            })?;
            println!("{}", if ok { "All checks passed." } else { "SOME CHECKS FAILED." });
            Ok(if ok { 0 } else { 1 })
        }
    }
}

fn judge(project: &Project, settings: &AppSettings, sel: &Selection, verbose: bool) -> Result<()> {
    let cancel = AtomicBool::new(false);
    let total = AtomicUsize::new(0);
    let done = AtomicUsize::new(0);
    let start = Instant::now();
    let stderr = std::sync::Mutex::new(std::io::stderr());
    let n = project.judge(
        settings,
        sel,
        &|ev| {
            let mut e = stderr.lock().unwrap();
            match ev {
                Event::Started { total: t, workers } => {
                    total.store(*t, Ordering::Relaxed);
                    let _ = writeln!(e, "Judging {t} task(s) with {workers} worker(s)…");
                }
                Event::Message(m) => {
                    let _ = writeln!(e, "{m}");
                }
                Event::TaskDone { result, .. } => {
                    let d = done.fetch_add(1, Ordering::Relaxed) + 1;
                    let t = total.load(Ordering::Relaxed);
                    if verbose {
                        let _ = writeln!(
                            e,
                            "[{d}/{t}] {:<16} {:<12} {:>6}/{:<4} {}",
                            result.contestant,
                            result.problem,
                            fmt_score(result.score),
                            fmt_score(result.full),
                            result.label()
                        );
                    } else {
                        let _ = write!(e, "\r[{d}/{t}] {:<40}", format!("{}/{}", result.contestant, result.problem));
                    }
                }
                _ => {}
            }
        },
        &cancel,
    )?;
    eprintln!("\rJudged {n} task(s) in {:.1}s{:30}", start.elapsed().as_secs_f64(), "");
    Ok(())
}

fn print_ranking(project: &Project) -> Result<()> {
    let rows = project.ranking()?;
    let probs = &project.contest.problems;
    print!("{:>4}  {:<16} {:>7}", "#", "Contestant", "Total");
    for p in probs {
        print!(" {:>10}", truncate(&p.name, 10));
    }
    println!();
    for r in rows {
        print!("{:>4}  {:<16} {:>7}", r.rank, truncate(&r.contestant, 16), fmt_score(r.total));
        for c in &r.cells {
            match c {
                Some(s) if s.status == judge_core::Status::Judged => print!(" {:>10}", fmt_score(s.score)),
                Some(s) => print!(" {:>10}", s.label),
                None => print!(" {:>10}", "-"),
            }
        }
        println!();
    }
    Ok(())
}

fn print_details(project: &Project, contestant: &str) -> Result<()> {
    for p in &project.contest.problems {
        let Some(r) = project.result(contestant, &p.name)? else {
            println!("{}: not judged", p.name);
            continue;
        };
        println!("{}: {}/{} {}", p.name, fmt_score(r.score), fmt_score(r.full), r.label());
        if !r.message.is_empty() && r.status != judge_core::Status::Judged {
            for l in r.message.lines().take(20) {
                println!("    {l}");
            }
        }
        for (i, s) in r.subtasks.iter().enumerate() {
            for c in &s.cases {
                println!(
                    "  #{:<3} {:<24} {:<5} {:>6} ms {:>8} KiB  {}",
                    i + 1,
                    truncate(&c.input, 24),
                    c.verdict.short(),
                    c.time_ms,
                    c.memory_kb,
                    c.message.lines().next().unwrap_or("")
                );
            }
        }
    }
    Ok(())
}

fn truncate(s: &str, n: usize) -> String {
    if s.chars().count() <= n {
        s.to_string()
    } else {
        s.chars().take(n - 1).chain(std::iter::once('…')).collect()
    }
}
