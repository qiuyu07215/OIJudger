//! End-to-end: judge examples/demo-contest and compare with expected.json.
#![cfg(windows)]

use std::collections::HashMap;
use std::sync::atomic::AtomicBool;

use judge_core::AppSettings;
use oij::project::{Project, Selection};

#[test]
fn demo_contest_scores() {
    sandbox::init_process();
    let src = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/demo-contest");
    // Work on a copy so the test never touches the checked-in example.
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("demo");
    copy_dir(&src, &root);
    let _ = std::fs::remove_dir_all(root.join(".oij"));

    let project = Project::open(&root).unwrap();
    let mut settings = AppSettings::default();
    settings.workers = 4;
    let cancel = AtomicBool::new(false);
    let n = project.judge(&settings, &Selection::default(), &|_| {}, &cancel).unwrap();
    assert_eq!(n, 30);

    let expected: HashMap<String, HashMap<String, f64>> =
        serde_json::from_str(&std::fs::read_to_string(root.join("expected.json")).unwrap()).unwrap();
    let mut errors = Vec::new();
    for s in project.summaries().unwrap() {
        let want = expected[&s.contestant][&s.problem];
        if (s.score - want).abs() > 1e-9 {
            errors.push(format!("{}/{}: got {} ({}), want {want}", s.contestant, s.problem, s.score, s.label));
        }
    }
    assert!(errors.is_empty(), "{}", errors.join("\n"));

    // Nothing is stale right after judging.
    let engine = project.engine(&settings);
    let stale = project.tasks(&engine, &Selection { only_stale: true, ..Default::default() }).unwrap();
    assert!(stale.is_empty(), "{stale:?}");

    // Editing a source makes exactly that task stale.
    let f = root.join("source/bob/sum.cpp");
    let text = r#"#include <cstdio>
int main() { long long a, b; scanf("%lld%lld", &a, &b); printf("%lld\n", a + b); }
"#;
    std::fs::write(&f, text).unwrap();
    let stale = project.tasks(&engine, &Selection { only_stale: true, ..Default::default() }).unwrap();
    assert_eq!(stale.len(), 1);
    project.judge(&settings, &Selection { only_stale: true, ..Default::default() }, &|_| {}, &cancel).unwrap();
    let r = project.result("bob", "sum").unwrap().unwrap();
    assert_eq!(r.score, 100.0, "{r:#?}");
}

fn copy_dir(from: &std::path::Path, to: &std::path::Path) {
    std::fs::create_dir_all(to).unwrap();
    for e in std::fs::read_dir(from).unwrap() {
        let e = e.unwrap();
        let p = e.path();
        if e.file_type().unwrap().is_dir() {
            copy_dir(&p, &to.join(e.file_name()));
        } else {
            std::fs::copy(&p, to.join(e.file_name())).unwrap();
        }
    }
}
