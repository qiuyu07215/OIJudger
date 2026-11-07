//! Judging one (contestant, problem) pair.

use std::collections::HashMap;
use std::fs::File;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

use anyhow::{Context, Result};

use crate::checker::{run_checker, CheckerLimits};
use crate::compare::compare_files;
use crate::compile::{build, BuildLimits, BuildOutput, BuildRequest, CompilerConfig, Lang};
use crate::discover::{find_answer, find_source};
use crate::model::{Case, ContestDir, Problem, ProblemKind};
use crate::result::{now, round_score, CaseResult, ProblemResult, Status, SubtaskResult, Verdict};
use crate::workspace::{retry_io, Worker};

/// Judge-wide settings.
#[derive(Debug, Clone)]
pub struct Settings {
    /// Number of parallel workers; 0 = automatic.
    pub workers: usize,
    /// Pin workers to distinct performance cores.
    pub pin_cores: bool,
    /// Raise the stack reserve of contestant programs to the memory limit.
    pub patch_stack: bool,
    pub output_limit_mb: u64,
    pub build: BuildLimits,
    pub checker: CheckerLimits,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            workers: 0,
            pin_cores: true,
            patch_stack: true,
            output_limit_mb: 256,
            build: BuildLimits::default(),
            checker: CheckerLimits::default(),
        }
    }
}

/// Configured compilers with their version strings.
#[derive(Debug, Clone, Default)]
pub struct Toolset {
    pub compilers: Vec<(CompilerConfig, String)>,
}

impl Toolset {
    pub fn new(configs: Vec<CompilerConfig>) -> Toolset {
        Toolset {
            compilers: configs
                .into_iter()
                .map(|c| {
                    let v = c.version();
                    (c, v)
                })
                .collect(),
        }
    }
    pub fn get(&self, lang: Lang) -> Option<&(CompilerConfig, String)> {
        self.compilers.iter().find(|(c, _)| c.lang == lang)
    }
}

/// Precompiled helper programs of a problem.
#[derive(Debug, Clone, Default)]
pub struct ProblemTools {
    /// Checker executable, or the reason it is unavailable.
    pub checker: Option<std::result::Result<PathBuf, String>>,
    pub interactor: Option<std::result::Result<PathBuf, String>>,
}

pub struct JudgeContext<'a> {
    pub dir: &'a ContestDir,
    pub settings: &'a Settings,
    pub tools: &'a Toolset,
    /// Checker / interactor of a problem, prepared on first use (possibly
    /// by the calling worker; concurrent callers wait for it).
    pub problem_tools: &'a (dyn Fn(&Worker, &Problem) -> ProblemTools + Sync),
    pub problem_hashes: &'a HashMap<String, String>,
    pub cancel: &'a AtomicBool,
}

const STDOUT_NAME: &str = "__oij_stdout";
const STDERR_NAME: &str = "__oij_stderr";

/// Hash of everything a contestant submitted for a problem.
pub fn source_fingerprint(dir: &ContestDir, contestant: &str, problem: &Problem) -> String {
    let mut h = blake3::Hasher::new();
    let mut any = false;
    let mut add = |p: Option<PathBuf>, h: &mut blake3::Hasher| {
        if let Some(p) = p {
            if let Ok(data) = std::fs::read(&p) {
                h.update(p.file_name().unwrap_or_default().to_string_lossy().as_bytes());
                h.update(&(data.len() as u64).to_le_bytes());
                h.update(&data);
                any = true;
            }
        }
    };
    match problem.kind {
        ProblemKind::Answer => {
            for st in &problem.subtasks {
                for c in &st.cases {
                    add(find_answer(dir, contestant, problem, &answer_file_name(problem, c)), &mut h);
                }
            }
        }
        ProblemKind::Communication => {
            let names = problem.communication.as_ref().map(|c| c.sources.clone()).unwrap_or_default();
            for n in names {
                add(find_source(dir, contestant, problem, &n), &mut h);
            }
        }
        _ => add(find_source(dir, contestant, problem, problem.source_name()), &mut h),
    }
    if any {
        h.finalize().to_hex()[..32].to_string()
    } else {
        String::new()
    }
}

fn answer_file_name(problem: &Problem, case: &Case) -> String {
    let file = Path::new(&case.input).file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
    format!("{file}.{}", problem.answer_ext())
}

fn tail(path: &Path, max: usize) -> String {
    let data = std::fs::read(path).unwrap_or_default();
    let start = data.len().saturating_sub(max);
    String::from_utf8_lossy(&data[start..]).trim().to_string()
}

fn case_result(case: &Case, verdict: Verdict, ratio: f64, message: impl Into<String>) -> CaseResult {
    CaseResult { input: case.input.clone(), verdict, ratio, time_ms: 0, memory_kb: 0, message: message.into() }
}

pub fn judge_problem(
    ctx: &JudgeContext,
    worker: &Worker,
    contestant: &str,
    problem: &Problem,
    on_case: &mut dyn FnMut(usize, &CaseResult),
) -> Option<ProblemResult> {
    let full = problem.full_score();
    let mut res = match judge_inner(ctx, worker, contestant, problem, on_case) {
        Ok(Some(r)) => r,
        Ok(None) => return None,
        Err(e) => ProblemResult::empty(contestant, &problem.name, full, Status::SystemError, format!("{e:#}")),
    };
    res.source_hash = source_fingerprint(ctx.dir, contestant, problem);
    res.problem_hash = ctx.problem_hashes.get(&problem.name).cloned().unwrap_or_default();
    res.judged_at = now();
    Some(res)
}

enum Runner {
    /// Contestant program at `run_dir/<exe>`.
    Program(String),
    /// Answer files submitted by the contestant.
    Answers,
}

fn judge_inner(
    ctx: &JudgeContext,
    worker: &Worker,
    contestant: &str,
    problem: &Problem,
    on_case: &mut dyn FnMut(usize, &CaseResult),
) -> Result<Option<ProblemResult>> {
    let full = problem.full_score();
    let empty = |status, msg: String| Ok(Some(ProblemResult::empty(contestant, &problem.name, full, status, msg)));

    let tools = (ctx.problem_tools)(worker, problem);
    if let Some(Err(e)) = &tools.checker {
        return empty(Status::SystemError, format!("Checker unavailable: {e}"));
    }
    if let Some(Err(e)) = &tools.interactor {
        return empty(Status::SystemError, format!("Interactor unavailable: {e}"));
    }

    let mut compile_log = String::new();
    let runner = if problem.kind == ProblemKind::Answer {
        let any = problem
            .subtasks
            .iter()
            .flat_map(|s| &s.cases)
            .any(|c| find_answer(ctx.dir, contestant, problem, &answer_file_name(problem, c)).is_some());
        if !any {
            return empty(Status::NoSource, "No answer files found".into());
        }
        Runner::Answers
    } else {
        let Some(req) = build_request(ctx, contestant, problem)? else {
            return empty(Status::NoSource, "Source file not found".into());
        };
        let req = match req {
            Ok(r) => r,
            Err(msg) => return empty(Status::SystemError, msg),
        };
        let (_, version) = ctx.tools.get(req.compiler.lang).expect("checked in build_request");
        match build(worker, &req, version, &ctx.dir.build_cache(), &ctx.settings.build)? {
            BuildOutput::Failed { log } => return empty(Status::CompileError, log),
            BuildOutput::Ok { exe, log } => {
                compile_log = log;
                let run_dir = worker.fresh_run_dir()?;
                let name = format!("{}.exe", problem.source_name());
                let dst = run_dir.join(&name);
                retry_io(|| std::fs::copy(&exe, &dst)).with_context(|| format!("copying {}", exe.display()))?;
                if ctx.settings.patch_stack {
                    sandbox::set_stack_reserve(&dst, problem.max_memory_mb() << 20)?;
                }
                Runner::Program(name)
            }
        }
    };

    let mut effective: Vec<f64> = Vec::with_capacity(problem.subtasks.len());
    let mut subtasks = Vec::with_capacity(problem.subtasks.len());
    let mut time_ms = 0u64;
    for (si, st) in problem.subtasks.iter().enumerate() {
        let dep = st.depends.iter().filter_map(|&d| effective.get(d - 1)).fold(1.0f64, |a, &b| a.min(b));
        let mut own = if st.cases.is_empty() { 0.0 } else { 1.0f64 };
        let mut cases = Vec::with_capacity(st.cases.len());
        for case in &st.cases {
            if ctx.cancel.load(Ordering::Relaxed) {
                return Ok(None);
            }
            if dep <= 0.0 {
                cases.push(case_result(case, Verdict::Skipped, 0.0, "Dependency failed"));
                continue;
            }
            if own <= 0.0 {
                cases.push(case_result(case, Verdict::Skipped, 0.0, ""));
                continue;
            }
            let r = match &runner {
                Runner::Program(exe) => run_case(ctx, worker, problem, &tools, exe, st.score, st, case),
                Runner::Answers => answer_case(ctx, worker, contestant, problem, &tools, st.score, case),
            }
            .unwrap_or_else(|e| case_result(case, Verdict::SystemError, 0.0, format!("{e:#}")));
            own = own.min(r.ratio);
            time_ms += r.time_ms;
            on_case(si, &r);
            cases.push(r);
        }
        let eff = own.min(dep);
        effective.push(eff);
        subtasks.push(SubtaskResult { score: round_score(st.score * eff), full: st.score, cases });
    }
    let score = round_score(subtasks.iter().map(|s| s.score).sum());
    Ok(Some(ProblemResult {
        contestant: contestant.into(),
        problem: problem.name.clone(),
        status: Status::Judged,
        score,
        full,
        time_ms,
        message: compile_log.trim().to_string(),
        subtasks,
        source_hash: String::new(),
        problem_hash: String::new(),
        judged_at: 0,
    }))
}

/// `Ok(None)`: no source. `Ok(Some(Err))`: configuration problem.
/// Compile ahead of judging (the compile pipeline); results land in the
/// build cache, errors are reported later by the judging worker.
pub(crate) fn prebuild(ctx: &JudgeContext, worker: &Worker, contestant: &str, problem: &Problem) {
    if problem.kind == ProblemKind::Answer || ctx.cancel.load(Ordering::Relaxed) {
        return;
    }
    if let Ok(Some(Ok(req))) = build_request(ctx, contestant, problem) {
        if let Some((_, version)) = ctx.tools.get(req.compiler.lang) {
            let _ = build(worker, &req, version, &ctx.dir.build_cache(), &ctx.settings.build);
        }
    }
}

pub(crate) fn build_request(
    ctx: &JudgeContext,
    contestant: &str,
    problem: &Problem,
) -> Result<Option<std::result::Result<BuildRequest, String>>> {
    let data = |rel: &str| ctx.dir.data_file(rel);
    let file_name = |p: &Path| p.file_name().unwrap_or_default().to_string_lossy().into_owned();
    let mut files: Vec<(PathBuf, String, bool)> = Vec::new();
    let lang;
    match problem.kind {
        ProblemKind::Communication => {
            let comm = problem.communication.clone().unwrap_or_default();
            let mut main_lang = None;
            for n in &comm.sources {
                let Some(src) = find_source(ctx.dir, contestant, problem, n) else { return Ok(None) };
                main_lang = main_lang.or(Lang::from_path(&src));
                files.push((src.clone(), file_name(&src), Lang::from_path(&src).is_some()));
            }
            if files.is_empty() {
                return Ok(Some(Err("Communication problem has no source files configured".into())));
            }
            for i in &comm.interfaces {
                let p = data(i);
                if !p.is_file() {
                    return Ok(Some(Err(format!("Interface file not found: {i}"))));
                }
                files.push((p.clone(), file_name(&p), Lang::from_path(&p).is_some()));
            }
            lang = main_lang.unwrap_or(Lang::Cpp);
        }
        _ => {
            let Some(src) = find_source(ctx.dir, contestant, problem, problem.source_name()) else {
                return Ok(None);
            };
            lang = Lang::from_path(&src).unwrap_or(Lang::Cpp);
            let ext = src.extension().unwrap_or_default().to_string_lossy().to_ascii_lowercase();
            files.push((src, format!("{}.{ext}", problem.source_name()), true));
            if problem.kind == ProblemKind::Interactive {
                if let Some(g) = &problem.grader {
                    if !g.header_path.is_empty() {
                        let p = data(&g.header_path);
                        if !p.is_file() {
                            return Ok(Some(Err(format!("Interaction header not found: {}", g.header_path))));
                        }
                        let name = if g.header_name.is_empty() { file_name(&p) } else { g.header_name.clone() };
                        files.push((p, name, false));
                    }
                    if !g.grader_path.is_empty() {
                        let p = data(&g.grader_path);
                        if !p.is_file() {
                            return Ok(Some(Err(format!("Grader not found: {}", g.grader_path))));
                        }
                        files.push((p.clone(), file_name(&p), true));
                    }
                }
            }
        }
    }
    let Some((compiler, _)) = ctx.tools.get(lang) else {
        return Ok(Some(Err(format!("No compiler configured for {lang:?}"))));
    };
    Ok(Some(Ok(BuildRequest { files, compiler: compiler.clone(), extra_args: problem.compile_args.clone() })))
}

/// Score a produced output file (checker or built-in comparator).
fn evaluate(
    ctx: &JudgeContext,
    worker: &Worker,
    problem: &Problem,
    tools: &ProblemTools,
    input: &Path,
    output: &Path,
    answer: &Path,
    full: f64,
) -> Result<(Verdict, f64, String)> {
    if let Some(Ok(checker)) = &tools.checker {
        let kind = problem.checker.as_ref().map(|c| c.kind).unwrap_or_default();
        let r = run_checker(worker, checker, kind, input, output, answer, full, &ctx.settings.checker)?;
        return Ok((r.verdict, r.ratio, r.message));
    }
    let c = compare_files(problem.comparator, problem.real_eps, output, answer)
        .with_context(|| format!("comparing with {}", answer.display()))?;
    Ok(if c.ok { (Verdict::AC, 1.0, String::new()) } else { (Verdict::WA, 0.0, c.message) })
}

#[allow(clippy::too_many_arguments)]
fn run_case(
    ctx: &JudgeContext,
    worker: &Worker,
    problem: &Problem,
    tools: &ProblemTools,
    exe: &str,
    full: f64,
    st: &crate::model::Subtask,
    case: &Case,
) -> Result<CaseResult> {
    let input = ctx.dir.data_file(&case.input);
    let answer = ctx.dir.data_file(&case.output);
    if !input.is_file() {
        return Ok(case_result(case, Verdict::SystemError, 0.0, format!("Input file not found: {}", case.input)));
    }
    if !answer.is_file() {
        return Ok(case_result(case, Verdict::SystemError, 0.0, format!("Answer file not found: {}", case.output)));
    }
    let run_dir = worker.run_dir();
    worker.clean_dir(&run_dir, &[exe])?;

    let mut limits = sandbox::Limits::program(st.time_ms, st.memory_mb << 20);
    limits.output_bytes = ctx.settings.output_limit_mb << 20;
    limits.affinity = worker.affinity();
    let stderr_path = run_dir.join(STDERR_NAME);
    let mut spec = sandbox::RunSpec::new(run_dir.join(exe), &run_dir, limits);
    spec.stderr = sandbox::Stdio::File(File::create(&stderr_path)?);
    spec.limits.watch_files.push(stderr_path.clone());

    if let Some(Ok(interactor)) = &tools.interactor {
        return run_interactive(ctx, worker, problem, tools, interactor, spec, full, case, &input, &answer);
    }

    let output = if problem.stdio {
        let out = run_dir.join(STDOUT_NAME);
        spec.stdin = sandbox::Stdio::File(File::open(&input)?);
        spec.stdout = sandbox::Stdio::File(File::create(&out)?);
        out
    } else {
        let dst = run_dir.join(problem.input_file());
        retry_io(|| std::fs::copy(&input, &dst)).with_context(|| format!("copying {}", case.input))?;
        run_dir.join(problem.output_file())
    };
    spec.limits.watch_files.push(output.clone());

    let r = worker.container().run(spec)?;
    if std::env::var_os("OIJ_DEBUG").is_some() {
        eprintln!(
            "[w{}] run {}: {:?} wall {} ms cpu {} ms",
            worker.index(),
            case.input,
            r.outcome,
            r.wall_ms,
            r.cpu_ms
        );
    }
    let mut cr = CaseResult {
        input: case.input.clone(),
        verdict: Verdict::AC,
        ratio: 0.0,
        time_ms: r.cpu_ms,
        memory_kb: r.peak_memory_bytes / 1024,
        message: String::new(),
    };
    if let Some(v) = program_failure(&r) {
        cr.verdict = v;
        cr.message = failure_message(&r, &stderr_path);
        return Ok(cr);
    }
    if !output.is_file() {
        cr.verdict = Verdict::NoOutput;
        cr.message = format!("Output file {} not found", problem.output_file());
        return Ok(cr);
    }
    let (v, ratio, msg) = evaluate(ctx, worker, problem, tools, &input, &output, &answer, full)?;
    cr.verdict = v;
    cr.ratio = ratio;
    cr.message = msg;
    Ok(cr)
}

fn program_failure(r: &sandbox::RunResult) -> Option<Verdict> {
    use sandbox::Outcome::*;
    match r.outcome {
        Ok => None,
        TimeLimit | IdleLimit => Some(Verdict::TLE),
        MemoryLimit => Some(Verdict::MLE),
        OutputLimit => Some(Verdict::OLE),
        NonZeroExit | Crashed | ProcessLimit => Some(Verdict::RE),
    }
}

fn failure_message(r: &sandbox::RunResult, stderr: &Path) -> String {
    let mut m = r.describe();
    if matches!(r.outcome, sandbox::Outcome::NonZeroExit | sandbox::Outcome::Crashed) {
        let t = tail(stderr, 512);
        if !t.is_empty() {
            m.push_str("\n");
            m.push_str(&t);
        }
    }
    m
}

/// testlib interactor connected to the contestant through two pipes.
#[allow(clippy::too_many_arguments)]
fn run_interactive(
    ctx: &JudgeContext,
    worker: &Worker,
    problem: &Problem,
    tools: &ProblemTools,
    interactor: &Path,
    mut spec: sandbox::RunSpec,
    full: f64,
    case: &Case,
    input: &Path,
    answer: &Path,
) -> Result<CaseResult> {
    let check_dir = worker.check_dir();
    worker.clean_dir(&check_dir, &[])?;
    let tout = check_dir.join("__oij_tout");
    let ierr = check_dir.join("__oij_interactor_stderr");

    let (to_contestant_r, to_contestant_w) = sandbox::pipe()?;
    let (to_interactor_r, to_interactor_w) = sandbox::pipe()?;
    spec.stdin = sandbox::Stdio::File(to_contestant_r);
    spec.stdout = sandbox::Stdio::File(to_interactor_w);

    let mut il = sandbox::Limits::program(ctx.settings.checker.cpu_ms, ctx.settings.checker.memory_mb << 20);
    il.wall_ms = spec.limits.wall_ms + 2000;
    il.output_bytes = 256 << 20;
    il.watch_files = vec![tout.clone(), ierr.clone()];
    let mut ispec = sandbox::RunSpec::new(interactor, &check_dir, il);
    ispec.args = vec![input.into(), tout.clone().into(), answer.into()];
    ispec.stdin = sandbox::Stdio::File(to_interactor_r);
    ispec.stdout = sandbox::Stdio::File(to_contestant_w);
    ispec.stderr = sandbox::Stdio::File(File::create(&ierr)?);

    let stderr_path = worker.run_dir().join(STDERR_NAME);
    let ichild = sandbox::Container::unisolated().spawn(ispec)?;
    let cchild = match worker.container().spawn(spec) {
        Ok(c) => c,
        Err(e) => {
            ichild.kill();
            return Err(e.into());
        }
    };
    let (cr, ir) = std::thread::scope(|s| {
        let h = s.spawn(move || ichild.wait());
        let c = cchild.wait();
        (c, h.join().expect("interactor wait thread"))
    });

    let mut res = CaseResult {
        input: case.input.clone(),
        verdict: Verdict::AC,
        ratio: 0.0,
        time_ms: cr.cpu_ms,
        memory_kb: cr.peak_memory_bytes / 1024,
        message: String::new(),
    };
    let interactor_ok = matches!(ir.outcome, sandbox::Outcome::Ok | sandbox::Outcome::NonZeroExit);
    let iv = interactor_ok.then(|| crate::checker::interpret_testlib(ir.exit_code, tail(&ierr, 4096)));
    // Whoever stopped first decides: an interactor that rejected the
    // dialogue (WA) is not masked by the contestant hanging afterwards, and a
    // contestant killed for TLE / RE is not masked by the interactor then
    // complaining about unexpected EOF.
    let interactor_first = ir.wall_ms + 20 < cr.wall_ms;

    let contestant_failed = program_failure(&cr);
    if let Some(iv) = &iv {
        if iv.verdict != Verdict::AC && (interactor_first || contestant_failed.is_none()) {
            res.verdict = iv.verdict;
            res.ratio = iv.ratio;
            res.message = iv.message.clone();
            return Ok(res);
        }
    }
    if let Some(v) = contestant_failed {
        res.verdict = v;
        res.message = failure_message(&cr, &stderr_path);
        return Ok(res);
    }
    let Some(iv) = iv else {
        res.verdict = Verdict::SystemError;
        res.message = format!("Interactor: {}", ir.describe());
        return Ok(res);
    };
    if tools.checker.is_some() {
        let (v, ratio, msg) = evaluate(ctx, worker, problem, tools, input, &tout, answer, full)?;
        res.verdict = v;
        res.ratio = ratio;
        res.message = msg;
    } else {
        res.ratio = iv.ratio;
        res.message = iv.message;
    }
    Ok(res)
}

fn answer_case(
    ctx: &JudgeContext,
    worker: &Worker,
    contestant: &str,
    problem: &Problem,
    tools: &ProblemTools,
    full: f64,
    case: &Case,
) -> Result<CaseResult> {
    let input = ctx.dir.data_file(&case.input);
    let answer = ctx.dir.data_file(&case.output);
    let name = answer_file_name(problem, case);
    let Some(output) = find_answer(ctx.dir, contestant, problem, &name) else {
        return Ok(case_result(case, Verdict::NoOutput, 0.0, format!("{name} not submitted")));
    };
    if !answer.is_file() {
        return Ok(case_result(case, Verdict::SystemError, 0.0, format!("Answer file not found: {}", case.output)));
    }
    let (v, ratio, msg) = evaluate(ctx, worker, problem, tools, &input, &output, &answer, full)?;
    Ok(case_result(case, v, ratio, msg))
}
