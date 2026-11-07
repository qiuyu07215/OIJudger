//! Online contest end to end on a copy of examples/demo-contest: submit real
//! solutions through the resident judging service and check verdicts,
//! scoreboards, formats, rejudging and write-back into `source/`.
#![cfg(windows)]

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use judge_core::model::{ContestFormat, OiFeedback};
use judge_core::AppSettings;
use online::{Account, JudgingState, OnlineContest, Outcome, Role, SubmitError, Viewer};

fn copy_dir(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).unwrap();
    for e in std::fs::read_dir(from).unwrap().flatten() {
        let p = e.path();
        let d = to.join(e.file_name());
        if p.is_dir() {
            copy_dir(&p, &d);
        } else {
            std::fs::copy(&p, &d).unwrap();
        }
    }
}

fn account(name: &str) -> Account {
    Account {
        id: 0,
        username: name.into(),
        display_name: format!("Team {name}"),
        group_name: String::new(),
        seat: String::new(),
        role: Role::Contestant,
        ip_lock: String::new(),
        disabled: false,
    }
}

fn wait_done(c: &OnlineContest, sid: i64) -> online::Submission {
    let t0 = Instant::now();
    loop {
        let s = c.db.submission(sid).unwrap().unwrap();
        if s.state == JudgingState::Done {
            return s;
        }
        assert!(t0.elapsed() < Duration::from_secs(120), "submission {sid} not judged: {s:?}");
        std::thread::sleep(Duration::from_millis(100));
    }
}

#[test]
fn online_contest_end_to_end() {
    sandbox::init_process();
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/demo-contest");
    let tmp = tempfile::tempdir().unwrap();
    let root: PathBuf = tmp.path().join("demo");
    copy_dir(&src, &root);
    let _ = std::fs::remove_dir_all(root.join(".oij"));
    let read = |rel: &str| std::fs::read(src.join("source").join(rel)).unwrap();

    let mut app = AppSettings::default();
    app.workers = 2;
    let notices = Arc::new(parking_lot::Mutex::new(Vec::new()));
    let n2 = notices.clone();
    let c = OnlineContest::open(&root, app, Arc::new(move |n| n2.lock().push(n))).unwrap();

    // ICPC contest that started a minute ago.
    let mut cfg = c.config();
    cfg.format = ContestFormat::Icpc;
    cfg.start = online::clock::format_local(online::now_ms() - 60_000);
    cfg.submit_interval_s = 0;
    c.set_config(cfg.clone()).unwrap();
    assert!(c.ensure_admin().unwrap().is_some());
    assert!(c.ensure_admin().unwrap().is_none());
    let (alice_id, _) = c.create_account(account("alice"), Some("pw".into())).unwrap();
    let (bob_id, _) = c.create_account(account("bob"), None).unwrap();
    let alice = c.db.account(alice_id).unwrap().unwrap();
    let bob = c.db.account(bob_id).unwrap().unwrap();
    c.start_judging().unwrap();

    // Refusals.
    assert_eq!(c.submit(&alice, "nope", "cpp", b"x", "").unwrap(), Err(SubmitError::UnknownProblem));
    assert_eq!(c.submit(&alice, "sum", "py", b"x", "").unwrap(), Err(SubmitError::UnsupportedLanguage));
    assert_eq!(c.submit(&alice, "sum", "cpp", b"  \n", "").unwrap(), Err(SubmitError::Empty));
    assert_eq!(c.submit(&alice, "relay", "cpp", b"int main(){}", "").unwrap(), Err(SubmitError::UnsupportedProblem));

    let a_sum = c.submit(&alice, "sum", "cpp", &read("alice/sum/sum.cpp"), "1.2.3.4").unwrap().unwrap();
    let b_sum = c.submit(&bob, "sum", "cpp", &read("bob/sum.cpp"), "").unwrap().unwrap();
    let a_guess = c.submit(&alice, "guess", "cpp", &read("alice/guess/guess.cpp"), "").unwrap().unwrap();
    let b_ce = c.submit(&bob, "guess", "cpp", b"int main( { }", "").unwrap().unwrap();

    let s = wait_done(&c, a_sum);
    assert_eq!((s.outcome, s.label.as_str()), (Outcome::Accepted, "AC"), "{s:?}");
    let s = wait_done(&c, b_sum);
    assert_eq!(s.outcome, Outcome::Rejected, "{s:?}");
    // ICPC stops at the first failure: later cases are skipped.
    let detail: judge_core::ProblemResult =
        serde_json::from_str(&c.db.judging_detail(s.judging_id.unwrap()).unwrap()).unwrap();
    assert!(detail.subtasks.iter().flat_map(|t| &t.cases).any(|k| k.verdict == judge_core::Verdict::Skipped));
    // Subtasks cut short by the early stop earn nothing.
    for st in &detail.subtasks {
        if st.cases.iter().any(|k| k.verdict == judge_core::Verdict::Skipped) {
            assert_eq!(st.score, 0.0, "{detail:?}");
        }
    }
    assert_eq!(wait_done(&c, a_guess).outcome, Outcome::Accepted);
    assert_eq!(wait_done(&c, b_ce).outcome, Outcome::CompileError);

    let board = c.scoreboard(Viewer::Public).unwrap();
    assert_eq!(board.rows[0].username, "alice");
    assert_eq!((board.rows[0].row.solved, board.rows[1].row.solved), (2, 0));
    assert_eq!(c.db.balloons().unwrap().len(), 2);
    assert!(c.db.balloons().unwrap().iter().all(|b| b.first));

    // Latest submissions are written back into source/ for offline tools.
    let wb = std::fs::read(root.join("source/bob/guess/guess.cpp")).unwrap();
    assert_eq!(wb, b"int main( { }");
    assert!(c.source(&c.db.submission(a_sum).unwrap().unwrap()).unwrap().contains("main"));

    // IOI: rejudge with full scoring; bob's sum gets partial points.
    let mut ioi = cfg.clone();
    ioi.format = ContestFormat::Ioi;
    c.set_config(ioi).unwrap();
    assert_eq!(c.rejudge(&[b_sum], "test").unwrap(), 1);
    let s = wait_done(&c, b_sum);
    assert!((s.score - 60.0).abs() < 1e-9, "{s:?}");
    let board = c.scoreboard(Viewer::Contestant(bob_id)).unwrap();
    let bob_row = board.rows.iter().find(|r| r.username == "bob").unwrap();
    assert!((bob_row.row.total - 60.0).abs() < 1e-9);

    // OI with compile check: only "compiles / does not compile".
    let mut oi = cfg.clone();
    oi.format = ContestFormat::Oi;
    oi.oi_feedback = OiFeedback::Compile;
    c.set_config(oi).unwrap();
    let ok = c.submit(&alice, "sum", "cpp", &read("alice/sum/sum.cpp"), "").unwrap().unwrap();
    let bad = c.submit(&bob, "sum", "cpp", b"int main( {", "").unwrap().unwrap();
    let s = wait_done(&c, ok);
    assert_eq!((s.outcome, s.label.as_str()), (Outcome::Pending, "OK"));
    assert_eq!(wait_done(&c, bad).outcome, Outcome::CompileError);
    assert!(c.scoreboard(Viewer::Public).unwrap().hidden);

    // After the contest: judge last submissions offline, then publish.
    let n = c.judge_offline(&|_| {}, &std::sync::atomic::AtomicBool::new(false)).unwrap();
    assert_eq!(n, 2 * c.problems().len());
    let jury = c.scoreboard(Viewer::Jury).unwrap();
    let a = jury.rows.iter().find(|r| r.username == "alice").unwrap();
    assert!(a.row.cells[0].score == Some(100.0), "{a:?}");
    c.publish_results(true, "test").unwrap();
    assert!(!c.scoreboard(Viewer::Public).unwrap().hidden);

    c.stop_judging();
    assert!(!notices.lock().is_empty());
}
