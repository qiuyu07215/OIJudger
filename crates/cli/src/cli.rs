//! Command line interface (`oijudger <command>`).

use std::io::Write;
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::time::Instant;

use anyhow::Result;
use clap::{Parser, Subcommand};
use judge_core::{AppSettings, Event};

use crate::project::{absolute, fmt_score, ExportFormat, Project, Selection};

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
        /// Compile everything before judging starts (default: judge each
        /// program as soon as it is compiled)
        #[arg(long)]
        two_phase: bool,
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
    /// Export results (csv, xlsx, html, htm = html without details)
    Export {
        dir: PathBuf,
        #[arg(short, long, default_value = "html")]
        format: String,
        /// Output file (default: <DIR>/results.<ext>)
        #[arg(short, long)]
        output: Option<PathBuf>,
        /// Language of the export: zh-CN or en (default: from settings)
        #[arg(long)]
        lang: Option<String>,
    },
    /// Print score statistics
    Stats { dir: PathBuf },
    /// Find suspiciously similar submissions
    Similarity {
        dir: PathBuf,
        /// Only this problem (default: all)
        #[arg(short, long)]
        problem: Option<String>,
        /// Minimum similarity in percent
        #[arg(short, long, default_value_t = 60.0)]
        threshold: f64,
    },
    /// Convert a LemonLime contest (.cdf) into contest.toml next to it
    ImportLemon {
        cdf: PathBuf,
        /// Overwrite an existing contest.toml
        #[arg(long)]
        force: bool,
    },
    /// List detected compilers
    Compilers,
    /// Verify sandbox isolation and limits on this machine
    Selftest,
}

pub fn run(cli: Cli) -> Result<i32> {
    sandbox::init_process();
    if sandbox::integrity_level().is_some_and(|l| l < sandbox::INTEGRITY_MEDIUM) {
        anyhow::bail!(
            "OIJudger is running at Low integrity (its folder probably carries a Low integrity label, common for \
             folders managed by sandboxing tools); it cannot write results or set up the judging sandbox there. \
             Extract OIJudger into a normal folder and run it from there."
        );
    }
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
        Command::Judge { dir, problems, contestants, all, jobs, no_pin, two_phase, verbose } => {
            let mut settings = settings;
            if let Some(j) = jobs {
                settings.workers = j;
            }
            if no_pin {
                settings.pin_cores = false;
            }
            if two_phase {
                settings.overlap_compile = false;
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
        Command::Export { dir, format, output, lang } => {
            let fmt = ExportFormat::parse(&format).ok_or_else(|| anyhow::anyhow!("unknown format {format}"))?;
            let project = Project::open(&dir)?;
            let out = output.unwrap_or_else(|| project.root().join(format!("results.{}", fmt.extension())));
            project.export(fmt, &out, lang.as_deref().unwrap_or(&settings.language))?;
            println!("Wrote {}", out.display());
            Ok(0)
        }
        Command::Stats { dir } => {
            let project = Project::open(&dir)?;
            let st = project.stats()?;
            println!(
                "{:<16} {:>6} {:>7} {:>8} {:>7} {:>7} {:>6} {:>6} {:>7}",
                "", "full", "subm.", "mean", "median", "std", "full#", "zero#", "discr."
            );
            let row = |name: &str, s: &analysis::stats::Summary| {
                println!(
                    "{:<16} {:>6} {:>7} {:>8.2} {:>7} {:>7.2} {:>6} {:>6} {:>7.3}",
                    truncate(name, 16),
                    fmt_score(s.full),
                    s.submitted,
                    s.mean,
                    fmt_score(s.median),
                    s.std_dev,
                    s.full_count,
                    s.zero_count,
                    s.discrimination
                )
            };
            row("(total)", &st.overall);
            for p in &st.problems {
                row(&p.name, &p.summary);
            }
            Ok(0)
        }
        Command::Similarity { dir, problem, threshold } => {
            let project = Project::open(&dir)?;
            let names: Vec<String> = match problem {
                Some(p) => vec![p],
                None => project.contest.problems.iter().map(|p| p.name.clone()).collect(),
            };
            let mut any = false;
            for p in names {
                for pair in project.similarity(&p, threshold / 100.0) {
                    any = true;
                    println!(
                        "{:<12} {:>5.1}%  {} <-> {}  ({} shared fingerprints)",
                        p,
                        pair.similarity * 100.0,
                        pair.a,
                        pair.b,
                        pair.shared
                    );
                }
            }
            if !any {
                println!("No pairs above {threshold}%.");
            }
            Ok(0)
        }
        Command::ImportLemon { cdf, force } => {
            let dir = cdf.parent().map(PathBuf::from).unwrap_or_else(|| PathBuf::from("."));
            let file = dir.join(judge_core::model::CONTEST_FILE);
            anyhow::ensure!(force || !file.exists(), "{} exists (use --force)", file.display());
            let im = lemon_import::import_file(&cdf)?;
            im.contest.save(&dir)?;
            println!("Imported {} problem(s) into {}", im.contest.problems.len(), file.display());
            for w in im.warnings {
                println!("  warning: {w}");
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
    let start = Instant::now();
    // (compiled, to compile, judged, to judge)
    let state = std::sync::Mutex::new((0usize, 0usize, 0usize, 0usize));
    let stderr = std::sync::Mutex::new(std::io::stderr());
    let status = |st: &(usize, usize, usize, usize)| {
        let mut parts = Vec::new();
        if st.1 > 0 && st.0 < st.1 {
            parts.push(format!("compile {}/{}", st.0, st.1));
        }
        if st.3 > 0 {
            parts.push(format!("judge {}/{}", st.2, st.3));
        }
        format!("\r[{}]{:10}", parts.join("] ["), "")
    };
    let n = project.judge(
        settings,
        sel,
        &|ev| {
            let mut st = state.lock().unwrap();
            let mut e = stderr.lock().unwrap();
            match ev {
                Event::Compiled { done, total } => {
                    st.0 = st.0.max(*done);
                    st.1 = *total;
                    if *done == 0 {
                        let _ = writeln!(e, "Compiling {total} program(s)…");
                    } else if done == total {
                        let _ = writeln!(
                            e,
                            "\rCompiled {total} program(s) in {:.1}s{:30}",
                            start.elapsed().as_secs_f64(),
                            ""
                        );
                    } else if !verbose {
                        let _ = write!(e, "{}", status(&st));
                    }
                }
                Event::Started { total: t, workers } => {
                    st.3 = *t;
                    let _ = writeln!(e, "Judging {t} task(s) with {workers} worker(s)…");
                }
                Event::Message(m) => {
                    let _ = writeln!(e, "\r{m}");
                }
                Event::TaskDone { result, .. } => {
                    st.2 += 1;
                    if verbose {
                        let _ = writeln!(
                            e,
                            "[{}/{}] {:<16} {:<12} {:>6}/{:<4} {}",
                            st.2,
                            st.3,
                            result.contestant,
                            result.problem,
                            fmt_score(result.score),
                            fmt_score(result.full),
                            result.label()
                        );
                    } else {
                        let _ = write!(e, "{}", status(&st));
                    }
                }
                _ => {}
            }
        },
        &cancel,
    )?;
    eprintln!("\rJudged {n} task(s) in {:.1}s{:40}", start.elapsed().as_secs_f64(), "");
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
