//! DOMjudge rules that need no real judging: too-late submissions, ignored
//! submissions, the team's own row during the freeze, rejudge selection.

use std::path::Path;
use std::sync::Arc;

use judge_core::model::ContestFormat;
use judge_core::AppSettings;
use online::clock::format_local;
use online::{Account, JudgingState, OnlineContest, Outcome, Role, SubmitError, Viewer};

fn contest() -> (tempfile::TempDir, Arc<OnlineContest>) {
    let tmp = tempfile::tempdir().unwrap();
    std::fs::write(
        tmp.path().join("contest.toml"),
        "title = \"T\"\n[[problem]]\nname = \"a\"\nstdio = true\n[[problem.subtask]]\nscore = 100\ntime_ms = 1000\nmemory_mb = 64\n\
         [[problem]]\nname = \"b\"\nstdio = true\n[[problem.subtask]]\nscore = 100\ntime_ms = 1000\nmemory_mb = 64\n",
    )
    .unwrap();
    let c = OnlineContest::open(tmp.path(), AppSettings::default(), Arc::new(|_| {})).unwrap();
    (tmp, c)
}

fn team(c: &OnlineContest, name: &str) -> Account {
    let a = Account {
        id: 0,
        username: name.into(),
        display_name: String::new(),
        group_name: String::new(),
        seat: String::new(),
        role: Role::Contestant,
        ip_lock: String::new(),
        disabled: false,
    };
    let (id, _) = c.create_account(a, Some("x".into())).unwrap();
    c.db.account(id).unwrap().unwrap()
}

/// Record a finished judging directly (no judging service needed).
fn judged(c: &OnlineContest, acc: &Account, problem: &str, at_ms: i64, outcome: Outcome) -> i64 {
    let (sid, jid) = c.db.add_submission(acc.id, problem, "cpp", "a.cpp", 10, at_ms, "").unwrap();
    let label = if outcome == Outcome::Accepted { "AC" } else { "WA" };
    c.db.judging_done(jid, JudgingState::Done, outcome, label, 0.0, 100.0, &[], 1, "", at_ms).unwrap();
    sid
}

#[test]
fn too_late_is_stored_not_judged() {
    let (_t, c) = contest();
    let mut cfg = c.config();
    cfg.start = format_local(online::now_ms() - 400 * 60_000);
    cfg.duration_min = 300;
    c.set_config(cfg.clone()).unwrap();
    let a = team(&c, "t1");
    let id = c.submit(&a, "a", "cpp", b"int main(){}", "").unwrap().unwrap();
    let s = c.db.submission(id).unwrap().unwrap();
    assert_eq!((s.state, s.label.as_str()), (JudgingState::Done, "TOO-LATE"));
    // Not counted anywhere, and not copied into source/.
    assert_eq!(c.scoreboard(Viewer::Jury).unwrap().rows[0].row.cells[0].attempts, 0);
    assert!(!c.dir.contest_source().join("t1").exists());
    // Before the start: refused.
    cfg.start = format_local(online::now_ms() + 60_000);
    c.set_config(cfg).unwrap();
    assert_eq!(c.submit(&a, "a", "cpp", b"int main(){}", "").unwrap(), Err(SubmitError::NotRunning));
}

#[test]
fn ignore_rejudge_selection_and_frozen_team_row() {
    let (_t, c) = contest();
    let mut cfg = c.config();
    cfg.format = ContestFormat::Icpc;
    let start = online::now_ms() - 250 * 60_000;
    cfg.start = format_local(start);
    cfg.duration_min = 300;
    cfg.freeze_min = 60; // frozen since minute 240
    c.set_config(cfg).unwrap();
    let t1 = team(&c, "t1");
    let t2 = team(&c, "t2");
    let min = |m: i64| start + m * 60_000 + 1000;

    let s1 = judged(&c, &t1, "a", min(10), Outcome::Rejected);
    let s2 = judged(&c, &t1, "a", min(20), Outcome::Accepted);
    let s3 = judged(&c, &t1, "b", min(245), Outcome::Accepted); // during the freeze
    judged(&c, &t2, "a", min(30), Outcome::Accepted);

    // Public board: t1's frozen accept is pending; t1 and t2 both have one problem.
    let public = c.scoreboard(Viewer::Public).unwrap();
    assert!(public.frozen);
    let p1 = public.rows.iter().find(|r| r.username == "t1").unwrap();
    assert_eq!((p1.row.solved, p1.row.cells[1].pending), (1, 1));

    // Team page row: own results keep updating, rank unknown.
    let own = c.team_row(t1.id).unwrap().unwrap();
    assert_eq!(own.rows.len(), 1);
    assert_eq!((own.rows[0].row.solved, own.rows[0].row.rank), (2, 0));

    // Ignoring the rejected attempt removes its penalty.
    let before = c.scoreboard(Viewer::Jury).unwrap();
    let pen = |b: &online::Scoreboard| b.rows.iter().find(|r| r.username == "t1").unwrap().row.penalty;
    assert_eq!(pen(&before), 20 + 20 + 245);
    c.set_ignored(s1, true, "jury").unwrap();
    assert_eq!(pen(&c.scoreboard(Viewer::Jury).unwrap()), 20 + 245);
    assert!(c.db.submission(s1).unwrap().unwrap().ignored);

    // Rejudge selection: accepted ones only on request, ignored never.
    assert!(c.rejudge_ids(Some("a"), Some(t1.id), false).unwrap().is_empty());
    assert_eq!(c.rejudge_ids(Some("a"), Some(t1.id), true).unwrap(), vec![s2]);
    assert_eq!(c.rejudge_ids(None, Some(t1.id), true).unwrap(), vec![s2, s3]);

    // Problemset statistics exclude frozen submissions for contestants.
    let stats = c.problem_stats(false, 10).unwrap();
    let b = stats.iter().find(|(p, _)| p == "b").unwrap();
    assert_eq!(b.1.iter().map(|x| x.0).sum::<u32>(), 0);
    let stats = c.problem_stats(true, 10).unwrap();
    let b = stats.iter().find(|(p, _)| p == "b").unwrap();
    assert_eq!(b.1.iter().map(|x| x.0).sum::<u32>(), 1);
    let _ = Path::new("");
}

/// Scoreboards are cached; every change that affects them must show at once.
#[test]
fn cached_boards_follow_changes() {
    let (_t, c) = contest();
    let mut cfg = c.config();
    cfg.format = ContestFormat::Icpc;
    let start = online::now_ms() - 250 * 60_000;
    cfg.start = format_local(start);
    cfg.duration_min = 300;
    cfg.freeze_min = 60;
    c.set_config(cfg).unwrap();
    let t1 = team(&c, "t1");
    let min = |m: i64| start + m * 60_000 + 1000;
    let s1 = judged(&c, &t1, "a", min(10), Outcome::Accepted);
    let s2 = judged(&c, &t1, "b", min(245), Outcome::Accepted);
    c.accounts_changed(); // the direct database writes above bypass the contest

    assert_eq!(c.scoreboard(Viewer::Public).unwrap().rows.len(), 1);
    let row = |c: &OnlineContest| c.team_row(t1.id).unwrap().unwrap().rows[0].row.solved;
    assert_eq!(row(&c), 2);

    // New account: on the board straight away.
    let t2 = team(&c, "t2");
    assert_eq!(c.scoreboard(Viewer::Public).unwrap().rows.len(), 2);
    assert_eq!(c.scoreboard(Viewer::Jury).unwrap().rows.len(), 2);

    // Disabled account (changed in the database, then announced): gone.
    let mut a = t2.clone();
    a.disabled = true;
    c.db.update_account(&a).unwrap();
    c.accounts_changed();
    assert_eq!(c.scoreboard(Viewer::Public).unwrap().rows.len(), 1);

    // Ignoring submissions updates the public board and the frozen team row.
    c.set_ignored(s1, true, "jury").unwrap();
    assert_eq!(c.scoreboard(Viewer::Public).unwrap().rows[0].row.solved, 0);
    assert_eq!(row(&c), 1);
    c.set_ignored(s2, true, "jury").unwrap();
    assert_eq!(row(&c), 0);

    // Unfreezing shows the hidden result to everyone.
    c.set_ignored(s2, false, "jury").unwrap();
    assert_eq!(c.scoreboard(Viewer::Public).unwrap().rows[0].row.cells[1].pending, 1);
    c.unfreeze(true, "jury").unwrap();
    let b = c.scoreboard(Viewer::Public).unwrap();
    assert!(!b.frozen);
    assert_eq!(b.rows[0].row.solved, 1);
    assert_eq!(c.team_row(t1.id).unwrap().unwrap().rows[0].row.rank, 1);
}
