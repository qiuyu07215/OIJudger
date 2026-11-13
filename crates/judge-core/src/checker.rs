//! Special judges (checkers): testlib and LemonLime protocols.
//!
//! Checkers are written by the problem setter and therefore trusted like the
//! test data; they run with job limits (time, memory, no child processes)
//! but without AppContainer isolation, so they can read the data directory
//! directly instead of copying answers around.

use std::fs::File;
use std::path::Path;

use anyhow::Result;

use crate::model::CheckerKind;
use crate::result::Verdict;
use crate::workspace::Worker;

#[derive(Debug, Clone)]
pub struct CheckerOutcome {
    pub verdict: Verdict,
    pub ratio: f64,
    pub message: String,
}

#[derive(Debug, Clone)]
pub struct CheckerLimits {
    pub cpu_ms: u64,
    pub memory_mb: u64,
}

impl Default for CheckerLimits {
    fn default() -> Self {
        CheckerLimits { cpu_ms: 10_000, memory_mb: 2048 }
    }
}

fn read_text(p: &Path, max: usize) -> String {
    let data = std::fs::read(p).unwrap_or_default();
    String::from_utf8_lossy(&data[..data.len().min(max)]).trim().to_string()
}

#[allow(clippy::too_many_arguments)]
pub fn run_checker(
    worker: &Worker,
    exe: &Path,
    kind: CheckerKind,
    input: &Path,
    output: &Path,
    answer: &Path,
    full_score: f64,
    limits: &CheckerLimits,
    runtime_path: &std::ffi::OsStr,
) -> Result<CheckerOutcome> {
    let dir = worker.check_dir();
    worker.clean_dir(&dir, &[])?;
    let out_file = dir.join("__oij_checker_stdout");
    let err_file = dir.join("__oij_checker_stderr");
    let score_file = dir.join("__oij_score");
    let msg_file = dir.join("__oij_message");

    let mut l = sandbox::Limits::program(limits.cpu_ms, limits.memory_mb << 20);
    l.output_bytes = 64 << 20;
    l.watch_files = vec![out_file.clone(), err_file.clone()];
    let mut spec = sandbox::RunSpec::new(exe, &dir, l);
    if !runtime_path.is_empty() {
        spec.env.push(("PATH".into(), runtime_path.into()));
    }
    spec.args = vec![input.into(), output.into(), answer.into()];
    if kind == CheckerKind::Lemon {
        spec.args.push(format!("{}", full_score.round() as i64).into());
        spec.args.push(score_file.clone().into());
        spec.args.push(msg_file.clone().into());
    }
    spec.stdout = sandbox::Stdio::File(File::create(&out_file)?);
    spec.stderr = sandbox::Stdio::File(File::create(&err_file)?);
    let r = sandbox::Container::unisolated().run(spec)?;

    let se = |m: String| CheckerOutcome { verdict: Verdict::SystemError, ratio: 0.0, message: m };
    match r.outcome {
        sandbox::Outcome::TimeLimit | sandbox::Outcome::IdleLimit => {
            return Ok(se("Checker time limit exceeded".into()))
        }
        sandbox::Outcome::MemoryLimit => return Ok(se("Checker memory limit exceeded".into())),
        sandbox::Outcome::OutputLimit => return Ok(se("Checker output limit exceeded".into())),
        _ => {}
    }

    match kind {
        CheckerKind::Testlib => {
            let mut msg = read_text(&err_file, 4096);
            if msg.is_empty() {
                msg = read_text(&out_file, 4096);
            }
            Ok(interpret_testlib(r.exit_code, msg))
        }
        CheckerKind::Lemon => {
            let msg = read_text(&msg_file, 4096);
            let score_text = read_text(&score_file, 256);
            match score_text.split_whitespace().next().and_then(|t| t.parse::<f64>().ok()) {
                Some(score) if score.is_finite() && score >= 0.0 => {
                    let ratio = if full_score > 0.0 { (score / full_score).clamp(0.0, 1.0) } else { 0.0 };
                    let verdict = if ratio >= 1.0 {
                        Verdict::AC
                    } else if ratio > 0.0 {
                        Verdict::PC
                    } else {
                        Verdict::WA
                    };
                    Ok(CheckerOutcome { verdict, ratio, message: msg })
                }
                _ => Ok(se(format!(
                    "Checker produced no valid score (exit {}): {}",
                    sandbox::describe_exit_code(r.exit_code),
                    if msg.is_empty() { read_text(&err_file, 1024) } else { msg }
                ))),
            }
        }
    }
}

/// Map a testlib exit code + message to a verdict.
///
/// testlib writes `ok …`, `wrong answer …`, `points <x> …` or
/// `partially correct (<n>) …` to stderr. The message is checked first
/// because `_pc(n)` exits with code `n` by default, which collides with the
/// WA / PE / FAIL codes for small `n`.
pub fn interpret_testlib(exit_code: u32, msg: String) -> CheckerOutcome {
    let pc = |ratio: f64, msg: String| {
        let ratio = ratio.clamp(0.0, 1.0);
        let verdict = if ratio >= 1.0 {
            Verdict::AC
        } else if ratio > 0.0 {
            Verdict::PC
        } else {
            Verdict::WA
        };
        CheckerOutcome { verdict, ratio, message: msg }
    };
    let lower = msg.to_ascii_lowercase();
    if let Some(rest) = lower.strip_prefix("points ") {
        if let Some(r) = rest.split_whitespace().next().and_then(|t| t.parse::<f64>().ok()) {
            return pc(r, msg);
        }
    }
    if let Some(rest) = lower.strip_prefix("partially correct (") {
        if let Some(n) = rest.split(')').next().and_then(|t| t.trim().parse::<f64>().ok()) {
            return pc(n / 100.0, msg);
        }
    }
    match exit_code {
        0 => CheckerOutcome { verdict: Verdict::AC, ratio: 1.0, message: msg },
        1 | 2 => CheckerOutcome { verdict: Verdict::WA, ratio: 0.0, message: msg },
        3 => CheckerOutcome { verdict: Verdict::SystemError, ratio: 0.0, message: format!("Checker failed: {msg}") },
        7 => match msg.split_whitespace().next().and_then(|t| t.parse::<f64>().ok()) {
            Some(r) => pc(r, msg),
            None => CheckerOutcome {
                verdict: Verdict::SystemError,
                ratio: 0.0,
                message: format!("Cannot parse partial score: {msg}"),
            },
        },
        // Old testlib: _pc(x) exits with 16 + x.
        16..=116 => pc((exit_code - 16) as f64 / 100.0, msg),
        c => CheckerOutcome {
            verdict: Verdict::SystemError,
            ratio: 0.0,
            message: format!("Checker crashed: {} {msg}", sandbox::describe_exit_code(c)),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn testlib_codes() {
        assert_eq!(interpret_testlib(0, "ok".into()).verdict, Verdict::AC);
        assert_eq!(interpret_testlib(1, "wrong".into()).verdict, Verdict::WA);
        let p = interpret_testlib(7, "points 0.4 half done".into());
        assert_eq!(p.verdict, Verdict::PC);
        assert!((p.ratio - 0.4).abs() < 1e-9);
        let p = interpret_testlib(16 + 30, String::new());
        assert!((p.ratio - 0.3).abs() < 1e-9);
        let p = interpret_testlib(1, "partially correct (1) almost nothing".into());
        assert_eq!(p.verdict, Verdict::PC);
        assert!((p.ratio - 0.01).abs() < 1e-9);
        assert_eq!(interpret_testlib(3, "bad".into()).verdict, Verdict::SystemError);
    }
}
