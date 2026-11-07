//! Finding contestants, their source files and problems on disk.

use std::cmp::Ordering;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use crate::compile::Lang;
use crate::model::{Case, ContestDir, Problem, Subfolder, Subtask};

/// Compare strings so that "a2" sorts before "a10".
pub fn natural_cmp(a: &str, b: &str) -> Ordering {
    let (mut x, mut y) = (a.as_bytes(), b.as_bytes());
    while !x.is_empty() && !y.is_empty() {
        if x[0].is_ascii_digit() && y[0].is_ascii_digit() {
            let nx = x.iter().take_while(|c| c.is_ascii_digit()).count();
            let ny = y.iter().take_while(|c| c.is_ascii_digit()).count();
            let (dx, dy) = (&x[..nx], &y[..ny]);
            let tx = dx.iter().skip_while(|&&c| c == b'0').count();
            let ty = dy.iter().skip_while(|&&c| c == b'0').count();
            let o = tx.cmp(&ty).then_with(|| dx[nx - tx..].cmp(&dy[ny - ty..])).then(nx.cmp(&ny));
            if o != Ordering::Equal {
                return o;
            }
            x = &x[nx..];
            y = &y[ny..];
        } else {
            let o = x[0].to_ascii_lowercase().cmp(&y[0].to_ascii_lowercase());
            if o != Ordering::Equal {
                return o;
            }
            x = &x[1..];
            y = &y[1..];
        }
    }
    x.len().cmp(&y.len()).then_with(|| a.cmp(b))
}

/// Contestant names = directories in `source/`.
pub fn list_contestants(dir: &ContestDir) -> Vec<String> {
    let mut v: Vec<String> = std::fs::read_dir(dir.source())
        .map(|it| {
            it.flatten()
                .filter(|e| e.file_type().map(|t| t.is_dir()).unwrap_or(false))
                .filter_map(|e| e.file_name().into_string().ok())
                .filter(|n| !n.starts_with('.'))
                .collect()
        })
        .unwrap_or_default();
    v.sort_by(|a, b| natural_cmp(a, b));
    v
}

fn candidate_dirs(dir: &ContestDir, contestant: &str, problem: &Problem) -> Vec<PathBuf> {
    let base = dir.source().join(contestant);
    let inside = base.join(&problem.name);
    let inside_src = base.join(problem.source_name());
    let mut v = Vec::new();
    match problem.subfolder {
        Subfolder::Inside => {
            v.push(inside);
            v.push(inside_src);
        }
        Subfolder::Outside => v.push(base),
        Subfolder::Auto => {
            v.push(inside);
            v.push(inside_src);
            v.push(base);
        }
    }
    v.dedup();
    v
}

/// Locate a source file named `name` (without extension) for a contestant.
pub fn find_source(dir: &ContestDir, contestant: &str, problem: &Problem, name: &str) -> Option<PathBuf> {
    // `name` may already carry an extension (communication problems).
    let given = Path::new(name);
    for d in candidate_dirs(dir, contestant, problem) {
        if Lang::from_path(given).is_some() {
            let p = d.join(name);
            if p.is_file() {
                return Some(p);
            }
            continue;
        }
        for ext in Lang::SOURCE_EXTS {
            let p = d.join(format!("{name}.{ext}"));
            if p.is_file() {
                return Some(p);
            }
        }
    }
    None
}

/// Locate a submitted answer file (answer problems).
pub fn find_answer(dir: &ContestDir, contestant: &str, problem: &Problem, file_name: &str) -> Option<PathBuf> {
    candidate_dirs(dir, contestant, problem).into_iter().map(|d| d.join(file_name)).find(|p| p.is_file())
}

/// Split `total` into `n` integer parts; the remainder goes to the last parts
/// (LemonLime's rule).
pub fn split_score(total: u64, n: usize) -> Vec<f64> {
    if n == 0 {
        return Vec::new();
    }
    let base = total / n as u64;
    let rem = (total % n as u64) as usize;
    (0..n).map(|i| (base + if i >= n - rem { 1 } else { 0 }) as f64).collect()
}

/// Pair input and answer files in one directory by base name.
pub fn pair_cases(dir: &Path, rel_prefix: &str, in_exts: &[&str], out_exts: &[&str]) -> Vec<Case> {
    let mut inputs: BTreeMap<String, String> = BTreeMap::new();
    let mut outputs: BTreeMap<String, String> = BTreeMap::new();
    let Ok(entries) = std::fs::read_dir(dir) else { return Vec::new() };
    for e in entries.flatten() {
        if !e.file_type().map(|t| t.is_file()).unwrap_or(false) {
            continue;
        }
        let Ok(name) = e.file_name().into_string() else { continue };
        let Some((stem, ext)) = name.rsplit_once('.') else { continue };
        if in_exts.iter().any(|x| x.eq_ignore_ascii_case(ext)) {
            inputs.insert(stem.to_string(), name.clone());
        } else if out_exts.iter().any(|x| x.eq_ignore_ascii_case(ext)) {
            outputs.entry(stem.to_string()).or_insert(name.clone());
        }
    }
    let mut cases: Vec<(String, Case)> = inputs
        .into_iter()
        .filter_map(|(stem, input)| {
            let output = outputs.get(&stem)?;
            Some((stem, Case { input: format!("{rel_prefix}{input}"), output: format!("{rel_prefix}{output}") }))
        })
        .collect();
    cases.sort_by(|a, b| natural_cmp(&a.0, &b.0));
    cases.into_iter().map(|(_, c)| c).collect()
}

const CHECKER_NAMES: [&str; 6] = ["checker", "chk", "spj", "check", "special_judge", "judge"];

/// Detect problems from `data/<problem>/` directories (one case per subtask,
/// 100 points split evenly) — the same rules as LemonLime's
/// "auto add problems".
pub fn detect_problems(dir: &ContestDir, time_ms: u64, memory_mb: u64) -> Vec<Problem> {
    let mut names: Vec<String> = std::fs::read_dir(dir.data())
        .map(|it| {
            it.flatten()
                .filter(|e| e.file_type().map(|t| t.is_dir()).unwrap_or(false))
                .filter_map(|e| e.file_name().into_string().ok())
                .filter(|n| !n.starts_with('.'))
                .collect()
        })
        .unwrap_or_default();
    names.sort_by(|a, b| natural_cmp(a, b));
    let mut out = Vec::new();
    for name in names {
        let pdir = dir.data().join(&name);
        let cases = pair_cases(&pdir, &format!("{name}/"), &["in"], &["out", "ans"]);
        if cases.is_empty() {
            continue;
        }
        let mut p = Problem::new(&name);
        let scores = split_score(100, cases.len());
        p.subtasks = cases
            .into_iter()
            .zip(scores)
            .map(|(c, score)| Subtask { score, time_ms, memory_mb, cases: vec![c], depends: Vec::new() })
            .collect();
        'chk: for n in CHECKER_NAMES {
            for ext in ["cpp", "cc", "exe"] {
                if pdir.join(format!("{n}.{ext}")).is_file() {
                    p.checker = Some(crate::model::Checker {
                        path: format!("{name}/{n}.{ext}"),
                        kind: crate::model::CheckerKind::Testlib,
                    });
                    break 'chk;
                }
            }
        }
        out.push(p);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn natural() {
        let mut v = vec!["a10", "a2", "a1", "b", "A3"];
        v.sort_by(|a, b| natural_cmp(a, b));
        assert_eq!(v, vec!["a1", "a2", "A3", "a10", "b"]);
    }

    #[test]
    fn split() {
        assert_eq!(split_score(100, 3), vec![33.0, 33.0, 34.0]);
        assert_eq!(split_score(100, 6), vec![16.0, 16.0, 17.0, 17.0, 17.0, 17.0]);
        assert_eq!(split_score(10, 10).iter().sum::<f64>(), 10.0);
    }

    #[test]
    fn detect() {
        let tmp = tempfile::tempdir().unwrap();
        let d = ContestDir::new(tmp.path());
        let p = d.data().join("sum");
        std::fs::create_dir_all(&p).unwrap();
        for i in [1, 2, 10] {
            std::fs::write(p.join(format!("sum{i}.in")), "1 2").unwrap();
            std::fs::write(p.join(format!("sum{i}.ans")), "3").unwrap();
        }
        std::fs::write(p.join("lonely.in"), "").unwrap();
        std::fs::write(p.join("checker.cpp"), "").unwrap();
        let probs = detect_problems(&d, 1000, 256);
        assert_eq!(probs.len(), 1);
        let pr = &probs[0];
        assert_eq!(pr.subtasks.len(), 3);
        assert_eq!(pr.subtasks[2].cases[0].input, "sum/sum10.in");
        assert_eq!(pr.full_score(), 100.0);
        assert_eq!(pr.checker.as_ref().unwrap().path, "sum/checker.cpp");
    }
}
