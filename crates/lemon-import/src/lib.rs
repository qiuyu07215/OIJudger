//! Import LemonLime contest files (`.cdf`).
//!
//! LemonLime writes the contest as JSON whose keys are its C++ member names
//! (`contestTitle`, `tasks[].testCases[].inputFiles`, ...). Subtask
//! dependencies are encoded as extra `inputFiles` entries of the form
//! `"<n>_lemon_SUbtaskDEPENDENCE_fLAg"`. Only problem configuration is
//! imported; results are re-judged. The legacy binary (Qt `QDataStream`)
//! format of very old Lemon versions is not supported.

use std::path::Path;

use anyhow::{bail, Context, Result};
use judge_core::model::{
    Case, Checker, CheckerKind, Communication, Comparator, Contest, Grader, Problem, ProblemKind, Subfolder, Subtask,
};
use serde_json::Value;

const DEPENDENCE_FLAG: &str = "_lemon_SUbtaskDEPENDENCE_fLAg";

pub struct Imported {
    pub contest: Contest,
    /// Things that could not be mapped exactly.
    pub warnings: Vec<String>,
}

fn s<'a>(v: &'a Value, k: &str) -> &'a str {
    v.get(k).and_then(Value::as_str).unwrap_or("")
}
fn b(v: &Value, k: &str) -> bool {
    v.get(k).and_then(Value::as_bool).unwrap_or(false)
}
fn n(v: &Value, k: &str) -> f64 {
    v.get(k).and_then(Value::as_f64).unwrap_or(0.0)
}
fn strs(v: &Value, k: &str) -> Vec<String> {
    v.get(k)
        .and_then(Value::as_array)
        .map(|a| a.iter().filter_map(|x| x.as_str().map(norm)).collect())
        .unwrap_or_default()
}
fn norm(p: &str) -> String {
    p.replace('\\', "/")
}

/// A problem name usable as an identifier (LemonLime titles may contain
/// spaces or CJK characters; the source file name is the stable key).
fn ident(source: &str, title: &str, index: usize) -> String {
    let base = if source.trim().is_empty() { title.trim() } else { source.trim() };
    let cleaned: String =
        base.chars().map(|c| if c.is_alphanumeric() || c == '_' || c == '-' { c } else { '_' }).collect();
    if cleaned.is_empty() {
        format!("problem{}", index + 1)
    } else {
        cleaned
    }
}

pub fn parse(json: &str) -> Result<Imported> {
    let root: Value = serde_json::from_str(json).context("not a LemonLime JSON contest file")?;
    let mut warnings = Vec::new();
    let mut contest = Contest { title: s(&root, "contestTitle").to_string(), problems: Vec::new() };
    let tasks = root.get("tasks").and_then(Value::as_array).cloned().unwrap_or_default();
    for (ti, t) in tasks.iter().enumerate() {
        let title = s(t, "problemTitle").to_string();
        let source = s(t, "sourceFileName").to_string();
        let mut name = ident(&source, &title, ti);
        while contest.problems.iter().any(|p| p.name.eq_ignore_ascii_case(&name)) {
            name.push('_');
        }
        let mut p = Problem::new(&name);
        if title != name {
            p.title = title.clone();
        }
        if !source.is_empty() && source != name {
            p.source_name = source.clone();
        }
        p.subfolder = if b(t, "subFolderCheck") { Subfolder::Inside } else { Subfolder::Outside };
        p.stdio = b(t, "standardInputCheck") || b(t, "standardOutputCheck");
        if b(t, "standardInputCheck") != b(t, "standardOutputCheck") {
            warnings.push(format!("{title}: mixed standard / file IO is imported as standard IO"));
        }
        p.input_file = s(t, "inputFileName").to_string();
        p.output_file = s(t, "outputFileName").to_string();
        // Keep the defaults implicit so renaming the problem keeps them in sync.
        if p.input_file == format!("{name}.in") {
            p.input_file.clear();
        }
        if p.output_file == format!("{name}.out") {
            p.output_file.clear();
        }

        let spj = norm(s(t, "specialJudge"));
        match t.get("comparisonMode").and_then(Value::as_i64).unwrap_or(1) {
            0 => p.comparator = Comparator::Strict,
            1 => p.comparator = Comparator::Lines,
            2 => {
                p.comparator = Comparator::Lines;
                warnings.push(format!("{title}: external diff mode is replaced by line comparison"));
            }
            3 => {
                p.comparator = Comparator::Real;
                let digits = n(t, "realPrecision");
                p.real_eps = if digits > 0.0 { 10f64.powf(-digits) } else { 1e-6 };
            }
            4 => p.checker = Some(Checker { path: spj.clone(), kind: CheckerKind::Lemon }),
            5 => p.checker = Some(Checker { path: spj.clone(), kind: CheckerKind::Testlib }),
            m => warnings.push(format!("{title}: unknown comparison mode {m}")),
        }

        match t.get("taskType").and_then(Value::as_i64).unwrap_or(0) {
            0 => p.kind = ProblemKind::Traditional,
            1 => {
                p.kind = ProblemKind::Answer;
                p.answer_ext = s(t, "answerFileExtension").to_string();
            }
            2 => {
                p.kind = ProblemKind::Interactive;
                p.grader = Some(Grader {
                    header_path: norm(s(t, "interactor")),
                    header_name: s(t, "interactorName").to_string(),
                    grader_path: norm(s(t, "grader")),
                });
            }
            k @ (3 | 4) => {
                p.kind = ProblemKind::Communication;
                p.communication =
                    Some(Communication { sources: strs(t, "sourceFilesName"), interfaces: strs(t, "graderFilesPath") });
                if k == 4 {
                    warnings.push(format!(
                        "{title}: multi-program communication is imported as single-program communication"
                    ));
                }
            }
            k => warnings.push(format!("{title}: unknown task type {k}")),
        }

        let cases = t.get("testCases").and_then(Value::as_array).cloned().unwrap_or_default();
        for (ci, c) in cases.iter().enumerate() {
            let mut inputs = Vec::new();
            let mut depends = Vec::new();
            for x in c.get("inputFiles").and_then(Value::as_array).into_iter().flatten().filter_map(Value::as_str) {
                match x.strip_suffix(DEPENDENCE_FLAG) {
                    Some(d) => match d.parse::<usize>() {
                        Ok(d) if d >= 1 && d <= ci => depends.push(d),
                        _ => warnings.push(format!("{title} #{}: ignored invalid dependency {d}", ci + 1)),
                    },
                    None => inputs.push(norm(x)),
                }
            }
            let outputs = strs(c, "outputFiles");
            if inputs.len() != outputs.len() {
                warnings.push(format!("{title} #{}: {} inputs but {} outputs", ci + 1, inputs.len(), outputs.len()));
            }
            p.subtasks.push(Subtask {
                score: n(c, "fullScore"),
                time_ms: n(c, "timeLimit").max(1.0) as u64,
                memory_mb: n(c, "memoryLimit").max(1.0) as u64,
                cases: inputs.into_iter().zip(outputs).map(|(input, output)| Case { input, output }).collect(),
                depends,
            });
        }
        contest.problems.push(p);
    }
    contest.validate()?;
    Ok(Imported { contest, warnings })
}

pub fn import_file(path: &Path) -> Result<Imported> {
    let data = std::fs::read(path).with_context(|| format!("reading {}", path.display()))?;
    match data.iter().find(|c| !c.is_ascii_whitespace()) {
        Some(b'{') | Some(b'[') => {}
        _ => bail!(
            "{} uses the old binary Lemon format; open it once in LemonLime and save it again to convert it to JSON",
            path.display()
        ),
    }
    let text = String::from_utf8_lossy(data.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(&data)).into_owned();
    parse(&text)
}

/// The single `.cdf` file in a directory, if there is exactly one.
pub fn find_cdf(dir: &Path) -> Option<std::path::PathBuf> {
    let mut found: Vec<_> = std::fs::read_dir(dir)
        .ok()?
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|e| e.eq_ignore_ascii_case("cdf")) && p.is_file())
        .collect();
    if found.len() == 1 {
        found.pop()
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"{
      "version": "1.0",
      "contestTitle": "Lemon Day 1",
      "tasks": [
        {
          "problemTitle": "跳跃", "sourceFileName": "jump", "inputFileName": "jump.in", "outputFileName": "jump.out",
          "standardInputCheck": false, "standardOutputCheck": false, "taskType": 0, "subFolderCheck": true,
          "comparisonMode": 1, "diffArguments": "", "realPrecision": 3, "specialJudge": "",
          "compilerConfiguration": {"g++": "default"}, "answerFileExtension": "out",
          "testCases": [
            {"fullScore": 30, "timeLimit": 1000, "memoryLimit": 256, "inputFiles": ["jump/1.in"], "outputFiles": ["jump/1.out"]},
            {"fullScore": 70, "timeLimit": 2000, "memoryLimit": 512,
             "inputFiles": ["jump\\2.in", "jump\\3.in", "1_lemon_SUbtaskDEPENDENCE_fLAg"], "outputFiles": ["jump/2.out", "jump/3.out"]}
          ]
        },
        {
          "problemTitle": "geo", "sourceFileName": "geo", "inputFileName": "geo.in", "outputFileName": "geo.out",
          "standardInputCheck": true, "standardOutputCheck": true, "taskType": 0, "subFolderCheck": false,
          "comparisonMode": 5, "realPrecision": 3, "specialJudge": "geo\\chk.exe",
          "testCases": [{"fullScore": 100, "timeLimit": 1000, "memoryLimit": 256, "inputFiles": ["geo/1.in"], "outputFiles": ["geo/1.ans"]}]
        },
        {
          "problemTitle": "Q", "sourceFileName": "q", "taskType": 2, "comparisonMode": 4, "specialJudge": "q/spj.exe",
          "interactor": "q\\lib.h", "interactorName": "q.h", "grader": "q/grader.cpp",
          "testCases": []
        }
      ],
      "contestants": []
    }"#;

    #[test]
    fn imports_sample() {
        let im = parse(SAMPLE).unwrap();
        let c = &im.contest;
        assert_eq!(c.title, "Lemon Day 1");
        assert_eq!(c.problems.len(), 3);
        let jump = &c.problems[0];
        assert_eq!(jump.name, "jump");
        assert_eq!(jump.title, "跳跃");
        assert_eq!(jump.subfolder, Subfolder::Inside);
        assert_eq!(jump.subtasks[1].cases[1].input, "jump/3.in");
        assert_eq!(jump.subtasks[1].depends, vec![1]);
        assert_eq!(jump.subtasks[1].memory_mb, 512);
        assert_eq!(jump.full_score(), 100.0);
        let geo = &c.problems[1];
        assert!(geo.stdio);
        assert_eq!(geo.checker.as_ref().unwrap().path, "geo/chk.exe");
        assert_eq!(geo.checker.as_ref().unwrap().kind, CheckerKind::Testlib);
        let q = &c.problems[2];
        assert_eq!(q.kind, ProblemKind::Interactive);
        assert_eq!(q.grader.as_ref().unwrap().header_name, "q.h");
        assert_eq!(q.checker.as_ref().unwrap().kind, CheckerKind::Lemon);
    }

    #[test]
    fn rejects_binary() {
        let tmp = std::env::temp_dir().join("oij_lemon_binary_test.cdf");
        std::fs::write(&tmp, [0x00, 0x11, 0x22, 0x33]).unwrap();
        assert!(import_file(&tmp).is_err());
        let _ = std::fs::remove_file(tmp);
    }
}
