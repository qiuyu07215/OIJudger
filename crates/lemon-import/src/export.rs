//! Write a LemonLime contest file (`.cdf`) with OIJudger's results, so the
//! contest can be opened (and its results viewed or exported) in LemonLime.
//!
//! The task settings come from the contest's original `.cdf` when the
//! problems still match it (LemonLime then sees exactly its own settings);
//! otherwise they are converted from `contest.toml`.

use std::collections::HashMap;
use std::path::Path;

use anyhow::{Context, Result};
use judge_core::model::{CheckerKind, Comparator, Contest, ContestDir, Problem, ProblemKind, Subfolder};
use judge_core::{ProblemResult, Status, Verdict};
use serde_json::{json, Map, Value};

use crate::DEPENDENCE_FLAG;

pub struct Exported {
    /// The `.cdf` JSON.
    pub json: String,
    /// Things LemonLime cannot represent exactly.
    pub warnings: Vec<String>,
}

// LemonLime's enums (src/base/LemonType.hpp, src/core/task.h).
const COMPILE_OK: i64 = 0;
const NO_SOURCE: i64 = 1;
const COMPILE_ERROR: i64 = 2;

fn result_state(v: Verdict) -> i64 {
    match v {
        Verdict::AC => 0,
        Verdict::WA => 1,
        Verdict::PC => 2,
        Verdict::TLE => 3,
        Verdict::MLE => 4,
        Verdict::SystemError => 5, // CannotStartProgram
        Verdict::NoOutput => 6,    // FileError
        Verdict::RE => 7,
        Verdict::Skipped => 11,
        Verdict::OLE => 14,
    }
}

/// Build the `.cdf` for `contest` with `results` for `contestants`.
pub fn to_cdf(contest: &Contest, dir: &ContestDir, contestants: &[String], results: &[ProblemResult]) -> Result<Exported> {
    // (issue, problem): reported once per issue with the problems it affects.
    let mut issues: Vec<(String, String)> = Vec::new();
    let tasks = match original_tasks(contest, dir) {
        Some(t) => t,
        None => contest.problems.iter().map(|p| task_json(p, dir, &mut issues)).collect(),
    };
    let mut warnings: Vec<String> = Vec::new();
    let mut seen: Vec<&str> = Vec::new();
    for (issue, _) in &issues {
        if !seen.contains(&issue.as_str()) {
            seen.push(issue);
            let titles: Vec<&str> = issues.iter().filter(|(i, _)| i == issue).map(|(_, t)| t.as_str()).collect();
            warnings.push(format!("{issue} ({})", titles.join(", ")));
        }
    }
    let by: HashMap<(&str, &str), &ProblemResult> =
        results.iter().map(|r| ((r.contestant.as_str(), r.problem.as_str()), r)).collect();
    let mut rounded = 0usize;
    let people: Vec<Value> = contestants
        .iter()
        .map(|c| {
            let per: Vec<Task> = contest
                .problems
                .iter()
                .map(|p| task_result(dir, c, p, by.get(&(c.as_str(), p.name.as_str())).copied(), &mut rounded))
                .collect();
            contestant_json(c, &per)
        })
        .collect();
    if rounded > 0 {
        warnings.push(format!(
            "LemonLime keeps whole points per test case: {rounded} fractional case score(s) were rounded"
        ));
    }
    let root = json!({
        "version": "1.0",
        "contestTitle": contest.title,
        "tasks": tasks,
        "contestants": people,
    });
    Ok(Exported { json: serde_json::to_string_pretty(&root)?, warnings })
}

/// Write the `.cdf` to `path`.
pub fn write_cdf(path: &Path, contest: &Contest, dir: &ContestDir, contestants: &[String], results: &[ProblemResult]) -> Result<Vec<String>> {
    let e = to_cdf(contest, dir, contestants, results)?;
    std::fs::write(path, e.json).with_context(|| format!("writing {}", path.display()))?;
    Ok(e.warnings)
}

/// The original `.cdf`'s tasks, if the contest's problems are still what
/// importing it gives (same names, kinds and test data / limits).
fn original_tasks(contest: &Contest, dir: &ContestDir) -> Option<Vec<Value>> {
    let path = crate::find_cdf(&dir.root)?;
    let text = std::fs::read(&path).ok()?;
    let text = String::from_utf8_lossy(text.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(&text)).into_owned();
    let root: Value = serde_json::from_str(&text).ok()?;
    let tasks = root.get("tasks")?.as_array()?.clone();
    let imported = crate::parse_with(&text, &|_, _| None).ok()?.contest;
    let same = imported.problems.len() == contest.problems.len()
        && imported.problems.iter().zip(&contest.problems).all(|(a, b)| {
            a.name == b.name && a.kind == b.kind && a.subtasks == b.subtasks && a.stdio == b.stdio
        });
    same.then_some(tasks)
}

/// LemonLime task settings converted from an OIJudger problem.
fn task_json(p: &Problem, dir: &ContestDir, issues: &mut Vec<(String, String)>) -> Value {
    let title = if p.title.is_empty() { p.name.clone() } else { p.title.clone() };
    let mut warn = |w: String| issues.push((w, title.clone()));
    let (task_type, mut extra) = match p.kind {
        ProblemKind::Traditional => (0, Map::new()),
        ProblemKind::Answer => (1, Map::new()),
        ProblemKind::Interactive => {
            let mut m = Map::new();
            match &p.grader {
                Some(g) => {
                    m.insert("interactor".into(), json!(g.header_path));
                    m.insert("interactorName".into(), json!(g.header_name));
                    m.insert("grader".into(), json!(g.grader_path));
                }
                None => warn("LemonLime has no separate interactor program; exported as an interaction task without one".into()),
            }
            (2, m)
        }
        ProblemKind::Communication => {
            let c = p.communication.clone().unwrap_or_default();
            let mut m = Map::new();
            m.insert("sourceFilesName".into(), json!(c.sources));
            m.insert("sourceFilesPath".into(), json!(c.sources));
            m.insert("graderFilesPath".into(), json!(c.interfaces));
            let names: Vec<String> =
                c.interfaces.iter().map(|i| Path::new(i).file_name().unwrap_or_default().to_string_lossy().into_owned()).collect();
            m.insert("graderFilesName".into(), json!(names));
            (if c.multi_program { 4 } else { 3 }, m)
        }
    };
    let (mode, spj, precision) = match (&p.checker, p.comparator) {
        (Some(c), _) => (if c.kind == CheckerKind::Lemon { 4 } else { 5 }, c.path.clone(), 3),
        (None, Comparator::Strict) => (0, String::new(), 3),
        (None, Comparator::IgnoreSpaces) => (1, String::new(), 3),
        (None, Comparator::Real) => (3, String::new(), (-p.real_eps.log10()).round().max(0.0) as i64),
        (None, other) => {
            warn(format!(
                "comparison \"{}\" has no LemonLime equivalent; exported as \"ignore extra spaces\"",
                serde_json::to_value(other).ok().and_then(|v| v.as_str().map(String::from)).unwrap_or_default()
            ));
            (1, String::new(), 3)
        }
    };
    let cases: Vec<Value> = p
        .subtasks
        .iter()
        .map(|st| {
            let mut inputs: Vec<String> = st.cases.iter().map(|c| c.input.clone()).collect();
            inputs.extend(st.depends.iter().map(|d| format!("{d}{DEPENDENCE_FLAG}")));
            json!({
                "fullScore": st.score.round() as i64,
                "timeLimit": st.time_ms,
                "memoryLimit": st.memory_mb,
                "inputFiles": inputs,
                "outputFiles": st.cases.iter().map(|c| c.output.clone()).collect::<Vec<_>>(),
            })
        })
        .collect();
    if p.subtasks.iter().any(|s| s.score.fract() != 0.0) {
        warn("LemonLime keeps whole points per test case; fractional scores were rounded".into());
    }
    let subfolder = match p.subfolder {
        Subfolder::Inside => true,
        Subfolder::Outside => false,
        // Whichever layout the contestants used.
        Subfolder::Auto => {
            let src = dir.source();
            std::fs::read_dir(&src).ok().into_iter().flatten().flatten().any(|e| e.path().join(p.source_name()).is_dir())
        }
    };
    let mut m = Map::new();
    m.insert("problemTitle".into(), json!(title));
    m.insert("sourceFileName".into(), json!(p.source_name()));
    m.insert("inputFileName".into(), json!(p.input_file()));
    m.insert("outputFileName".into(), json!(p.output_file()));
    m.insert("standardInputCheck".into(), json!(p.stdio));
    m.insert("standardOutputCheck".into(), json!(p.stdio));
    m.insert("taskType".into(), json!(task_type));
    m.insert("subFolderCheck".into(), json!(subfolder));
    m.insert("comparisonMode".into(), json!(mode));
    m.insert("diffArguments".into(), json!(""));
    m.insert("realPrecision".into(), json!(precision));
    m.insert("specialJudge".into(), json!(spj));
    m.append(&mut extra);
    // Compiler configurations are named entries of LemonLime's own settings;
    // LemonLime picks a valid one when the name is unknown.
    m.insert("compilerConfiguration".into(), json!({}));
    m.insert("answerFileExtension".into(), json!(p.answer_ext()));
    m.insert("testCases".into(), json!(cases));
    Value::Object(m)
}

/// One task of one contestant, in LemonLime's layout.
struct Task {
    judged: bool,
    source: String,
    compile_message: String,
    compile_state: i64,
    /// Per test case (subtask), per input: (input name, score, result, time, memory, message).
    cases: Vec<Vec<(String, i64, i64, i64, i64, String)>>,
}

fn task_result(dir: &ContestDir, contestant: &str, p: &Problem, r: Option<&ProblemResult>, rounded: &mut usize) -> Task {
    let mut t = Task {
        judged: false,
        source: String::new(),
        compile_message: String::new(),
        compile_state: NO_SOURCE,
        cases: Vec::new(),
    };
    let Some(r) = r else { return t };
    let source_name = match (&p.kind, &p.communication) {
        (ProblemKind::Answer, _) => None,
        (ProblemKind::Communication, Some(c)) => c.sources.first().cloned(),
        _ => Some(p.source_name().to_string()),
    };
    t.source = source_name
        .and_then(|n| judge_core::discover::find_source(dir, contestant, p, &n))
        .and_then(|f| f.file_name().map(|n| n.to_string_lossy().into_owned()))
        .unwrap_or_default();
    match r.status {
        Status::NoSource => t.judged = true,
        Status::CompileError => {
            t.judged = true;
            t.compile_state = COMPILE_ERROR;
            t.compile_message = r.message.clone();
        }
        // LemonLime has nothing like it: leave the task unjudged there.
        Status::SystemError => {}
        Status::Judged => {
            t.judged = true;
            t.compile_state = COMPILE_OK;
            t.compile_message = r.message.clone();
            for (st, sr) in p.subtasks.iter().zip(&r.subtasks) {
                let cases = st
                    .cases
                    .iter()
                    .zip(&sr.cases)
                    .map(|(c, cr)| {
                        let points = cr.ratio * st.score;
                        if points.fract().abs() > 1e-9 {
                            *rounded += 1;
                        }
                        let ran = cr.verdict != Verdict::Skipped;
                        let name = Path::new(&c.input).file_name().unwrap_or_default().to_string_lossy().into_owned();
                        (
                            name,
                            points.round() as i64,
                            result_state(cr.verdict),
                            if ran { cr.time_ms as i64 } else { -1 },
                            if ran { (cr.memory_kb * 1024) as i64 } else { -1 },
                            cr.message.clone(),
                        )
                    })
                    .collect();
                t.cases.push(cases);
            }
        }
    }
    t
}

fn contestant_json(name: &str, tasks: &[Task]) -> Value {
    let pick = |f: &dyn Fn(&(String, i64, i64, i64, i64, String)) -> Value| -> Value {
        Value::Array(
            tasks
                .iter()
                .map(|t| Value::Array(t.cases.iter().map(|c| Value::Array(c.iter().map(f).collect())).collect()))
                .collect(),
        )
    };
    let now = judge_core::result::now();
    json!({
        "contestantName": name,
        "checkJudged": tasks.iter().map(|t| t.judged).collect::<Vec<_>>(),
        "sourceFile": tasks.iter().map(|t| t.source.clone()).collect::<Vec<_>>(),
        "compileMesaage": tasks.iter().map(|t| t.compile_message.clone()).collect::<Vec<_>>(),
        "inputFiles": pick(&|c| json!(c.0)),
        "message": pick(&|c| json!(c.5)),
        "score": pick(&|c| json!(c.1)),
        "timeUsed": pick(&|c| json!(c.3)),
        "memoryUsed": pick(&|c| json!(c.4)),
        // Qt: Julian day, milliseconds since midnight, Qt::UTC (1).
        "judgingTime_date": 2_440_588 + now.div_euclid(86_400),
        "judgingTime_time": now.rem_euclid(86_400) * 1000,
        "judgingTime_timespec": 1,
        "compileState": tasks.iter().map(|t| t.compile_state).collect::<Vec<_>>(),
        "result": pick(&|c| json!(c.2)),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use judge_core::model::{Case, Subtask};
    use judge_core::{CaseResult, SubtaskResult};

    fn problem() -> Problem {
        let mut p = Problem::new("sum");
        p.title = "A + B".into();
        p.comparator = Comparator::IgnoreSpaces;
        let st = |score: f64, inputs: &[&str], depends: Vec<usize>| Subtask {
            score,
            time_ms: 1000,
            memory_mb: 256,
            cases: inputs.iter().map(|i| Case { input: format!("sum/{i}.in"), output: format!("sum/{i}.out") }).collect(),
            depends,
        };
        p.subtasks = vec![st(40.0, &["1"], vec![]), st(60.0, &["2", "3"], vec![1])];
        p
    }

    fn case(input: &str, verdict: Verdict, ratio: f64) -> CaseResult {
        CaseResult { input: input.into(), verdict, ratio, time_ms: 12, memory_kb: 2048, message: String::new() }
    }

    #[test]
    fn generated_tasks_round_trip() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = ContestDir::new(tmp.path());
        let contest = Contest { title: "T".into(), online: None, problems: vec![problem()] };
        let mut r = ProblemResult::empty("alice", "sum", 100.0, Status::Judged, "");
        r.score = 70.0;
        r.subtasks = vec![
            SubtaskResult { score: 40.0, full: 40.0, cases: vec![case("sum/1.in", Verdict::AC, 1.0)] },
            SubtaskResult {
                score: 30.0,
                full: 60.0,
                cases: vec![case("sum/2.in", Verdict::PC, 0.5), case("sum/3.in", Verdict::AC, 1.0)],
            },
        ];
        let ce = ProblemResult::empty("bob", "sum", 100.0, Status::CompileError, "error: x");
        let e = to_cdf(&contest, &dir, &["alice".into(), "bob".into(), "carol".into()], &[r, ce]).unwrap();

        // Our importer reads the tasks back unchanged.
        let back = crate::parse_with(&e.json, &|_, _| None).unwrap().contest;
        assert_eq!(back.problems[0].subtasks, contest.problems[0].subtasks);
        assert_eq!(back.problems[0].comparator, Comparator::IgnoreSpaces);
        assert_eq!(back.problems[0].title, "A + B");

        let v: Value = serde_json::from_str(&e.json).unwrap();
        let alice = &v["contestants"][0];
        assert_eq!(alice["checkJudged"], json!([true]));
        assert_eq!(alice["compileState"], json!([COMPILE_OK]));
        assert_eq!(alice["score"], json!([[[40], [30, 60]]]));
        assert_eq!(alice["result"], json!([[[0], [2, 0]]]));
        assert_eq!(alice["inputFiles"], json!([[["1.in"], ["2.in", "3.in"]]]));
        assert_eq!(alice["memoryUsed"][0][0][0], json!(2048 * 1024));
        let bob = &v["contestants"][1];
        assert_eq!(bob["compileState"], json!([COMPILE_ERROR]));
        assert_eq!(bob["compileMesaage"], json!(["error: x"]));
        assert_eq!(bob["score"], json!([[]]));
        let carol = &v["contestants"][2];
        assert_eq!(carol["checkJudged"], json!([false]));
    }
}
