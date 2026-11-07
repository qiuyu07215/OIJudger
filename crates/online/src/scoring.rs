//! Scoreboards for ICPC and IOI contests (OI boards come from the offline
//! results after the contest), and the ICPC resolver ("滚榜").
//!
//! Everything here is a pure function of the submissions, so it is easy to
//! test and cheap to recompute: a contest of 200 teams with a few thousand
//! submissions takes well under a millisecond.

use std::collections::HashMap;

use judge_core::model::IoiScoring;
use serde::{Deserialize, Serialize};

/// Result of one submission as far as scoring is concerned.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Outcome {
    /// Queued or being judged (or hidden by the freeze).
    Pending,
    Accepted,
    /// Wrong answer, time limit, runtime error, ...
    Rejected,
    CompileError,
    /// System error: not counted until rejudged.
    Error,
}

#[derive(Debug, Clone)]
pub struct Sub {
    pub id: i64,
    pub account: i64,
    pub problem: String,
    /// Contest time of the submission in milliseconds.
    pub time_ms: i64,
    pub outcome: Outcome,
    /// Points (IOI).
    pub score: f64,
    /// Points per subtask (IOI, `max_subtask`).
    pub subtask_scores: Vec<f64>,
    /// Hidden by the scoreboard freeze for this viewer.
    pub hidden: bool,
}

/// One cell of a scoreboard.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct Cell {
    /// Counted submissions (ICPC: rejected ones before the accept, plus the accept).
    pub attempts: u32,
    /// Submissions whose result is not known (pending or frozen).
    pub pending: u32,
    pub solved: bool,
    /// ICPC: minute of the accept.
    pub time_min: i64,
    /// ICPC: first team to solve this problem.
    pub first: bool,
    /// IOI / OI points; `None` if nothing was judged.
    pub score: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Row {
    pub account: i64,
    pub rank: u32,
    /// ICPC
    pub solved: u32,
    pub penalty: i64,
    /// Contest time (ms) of the last accept, for tie-breaking.
    pub last_ac_ms: i64,
    /// IOI / OI
    pub total: f64,
    /// One per problem, in contest order.
    pub cells: Vec<Cell>,
}

#[derive(Debug, Clone)]
pub struct IcpcRules {
    pub penalty_min: i64,
    pub compile_error_penalty: bool,
}

fn group<'a>(subs: &'a [Sub]) -> HashMap<(i64, &'a str), Vec<&'a Sub>> {
    let mut m: HashMap<(i64, &str), Vec<&Sub>> = HashMap::new();
    for s in subs {
        m.entry((s.account, s.problem.as_str())).or_default().push(s);
    }
    for v in m.values_mut() {
        v.sort_by_key(|s| (s.time_ms, s.id));
    }
    m
}

/// ICPC cell from one team's submissions on one problem (sorted by time).
/// After a pending submission everything is pending: its result decides
/// whether later ones count. Returns the cell and the accept time in ms.
fn icpc_cell(subs: &[&Sub], rules: &IcpcRules) -> (Cell, i64) {
    let mut c = Cell::default();
    let mut rejected = 0u32;
    for s in subs {
        let outcome = if s.hidden { Outcome::Pending } else { s.outcome };
        if c.pending > 0 {
            c.pending += 1;
            continue;
        }
        match outcome {
            Outcome::Pending | Outcome::Error => c.pending += 1,
            Outcome::Accepted => {
                c.solved = true;
                c.time_min = s.time_ms / 60_000;
                c.attempts = rejected + 1;
                return (c, s.time_ms);
            }
            Outcome::Rejected => rejected += 1,
            Outcome::CompileError => {
                if rules.compile_error_penalty {
                    rejected += 1
                }
            }
        }
    }
    c.attempts = rejected + c.pending;
    (c, 0)
}

fn icpc_rank(rows: &mut [Row]) {
    rows.sort_by(|a, b| {
        b.solved.cmp(&a.solved).then(a.penalty.cmp(&b.penalty)).then(a.last_ac_ms.cmp(&b.last_ac_ms))
    });
    let mut rank = 0;
    for i in 0..rows.len() {
        let tie = i > 0 && {
            let (p, c) = (&rows[i - 1], &rows[i]);
            p.solved == c.solved && p.penalty == c.penalty && p.last_ac_ms == c.last_ac_ms
        };
        if !tie {
            rank = i as u32 + 1;
        }
        rows[i].rank = rank;
    }
}

fn icpc_totals(row: &mut Row, rules: &IcpcRules, ac_ms: &[i64]) {
    row.solved = 0;
    row.penalty = 0;
    row.last_ac_ms = 0;
    for (c, &t) in row.cells.iter().zip(ac_ms) {
        if c.solved {
            row.solved += 1;
            row.penalty += c.time_min + rules.penalty_min * (c.attempts as i64 - 1);
            row.last_ac_ms = row.last_ac_ms.max(t);
        }
    }
}

/// ICPC scoreboard. `accounts` lists every team in display order (ties keep it).
pub fn icpc_board(accounts: &[i64], problems: &[String], subs: &[Sub], rules: &IcpcRules) -> Vec<Row> {
    let groups = group(subs);
    let mut rows: Vec<Row> = Vec::with_capacity(accounts.len());
    let mut first: Vec<Option<i64>> = vec![None; problems.len()];
    let mut times: Vec<Vec<i64>> = Vec::with_capacity(accounts.len());
    for &a in accounts {
        let mut cells = Vec::with_capacity(problems.len());
        let mut ac = Vec::with_capacity(problems.len());
        for (pi, p) in problems.iter().enumerate() {
            let (c, t) = groups.get(&(a, p.as_str())).map(|v| icpc_cell(v, rules)).unwrap_or_default();
            if c.solved {
                first[pi] = Some(first[pi].map_or(t, |f: i64| f.min(t)));
            }
            cells.push(c);
            ac.push(t);
        }
        let mut row = Row { account: a, rank: 0, solved: 0, penalty: 0, last_ac_ms: 0, total: 0.0, cells };
        icpc_totals(&mut row, rules, &ac);
        rows.push(row);
        times.push(ac);
    }
    for (row, ac) in rows.iter_mut().zip(&times) {
        for (pi, c) in row.cells.iter_mut().enumerate() {
            c.first = c.solved && first[pi] == Some(ac[pi]);
        }
    }
    icpc_rank(&mut rows);
    rows
}

/// IOI scoreboard: best score per problem over all (visible) submissions.
pub fn ioi_board(accounts: &[i64], problems: &[String], subs: &[Sub], scoring: IoiScoring) -> Vec<Row> {
    let groups = group(subs);
    let mut rows: Vec<Row> = accounts
        .iter()
        .map(|&a| {
            let cells: Vec<Cell> = problems
                .iter()
                .map(|p| {
                    let mut c = Cell::default();
                    let Some(v) = groups.get(&(a, p.as_str())) else { return c };
                    let mut best_sub: Option<f64> = None;
                    let mut per_subtask: Vec<f64> = Vec::new();
                    for s in v {
                        c.attempts += 1;
                        let outcome = if s.hidden { Outcome::Pending } else { s.outcome };
                        match outcome {
                            Outcome::Pending | Outcome::Error => c.pending += 1,
                            Outcome::CompileError => best_sub = Some(best_sub.unwrap_or(0.0)),
                            Outcome::Accepted | Outcome::Rejected => {
                                best_sub = Some(best_sub.map_or(s.score, |b| b.max(s.score)));
                                if per_subtask.len() < s.subtask_scores.len() {
                                    per_subtask.resize(s.subtask_scores.len(), 0.0);
                                }
                                for (b, &x) in per_subtask.iter_mut().zip(&s.subtask_scores) {
                                    *b = b.max(x);
                                }
                            }
                        }
                    }
                    c.score = best_sub.map(|b| match scoring {
                        IoiScoring::MaxSubmission => b,
                        IoiScoring::MaxSubtask => round2(per_subtask.iter().sum::<f64>()).max(b),
                    });
                    c.solved = false;
                    c
                })
                .collect();
            let total = round2(cells.iter().filter_map(|c| c.score).sum());
            Row { account: a, rank: 0, solved: 0, penalty: 0, last_ac_ms: 0, total, cells }
        })
        .collect();
    score_rank(&mut rows);
    rows
}

/// Board from final scores (OI after judging): `scores[(account, problem)]`.
pub fn score_board(accounts: &[i64], problems: &[String], scores: &HashMap<(i64, String), f64>) -> Vec<Row> {
    let mut rows: Vec<Row> = accounts
        .iter()
        .map(|&a| {
            let cells: Vec<Cell> = problems
                .iter()
                .map(|p| Cell { score: scores.get(&(a, p.clone())).copied(), ..Default::default() })
                .collect();
            let total = round2(cells.iter().filter_map(|c| c.score).sum());
            Row { account: a, rank: 0, solved: 0, penalty: 0, last_ac_ms: 0, total, cells }
        })
        .collect();
    score_rank(&mut rows);
    rows
}

fn score_rank(rows: &mut [Row]) {
    rows.sort_by(|a, b| b.total.partial_cmp(&a.total).unwrap_or(std::cmp::Ordering::Equal));
    let mut rank = 0;
    for i in 0..rows.len() {
        if i == 0 || (rows[i - 1].total - rows[i].total).abs() > 1e-9 {
            rank = i as u32 + 1;
        }
        rows[i].rank = rank;
    }
}

fn round2(x: f64) -> f64 {
    (x * 100.0).round() / 100.0
}

/// One step of the ICPC resolver: a frozen cell is revealed.
#[derive(Debug, Clone, Serialize)]
pub struct ResolveStep {
    pub account: i64,
    pub problem: usize,
    pub cell: Cell,
    pub solved: u32,
    pub penalty: i64,
    pub last_ac_ms: i64,
}

/// Resolver: start from the frozen board (`subs` with `hidden` set for
/// frozen submissions) and reveal one cell at a time, always the leftmost
/// frozen cell of the lowest-ranked team that still has one. Returns the
/// frozen board and the steps; replaying the steps (and re-sorting by solved,
/// penalty, last accept) yields the final board.
pub fn resolve(accounts: &[i64], problems: &[String], subs: &[Sub], rules: &IcpcRules) -> (Vec<Row>, Vec<ResolveStep>) {
    let frozen = icpc_board(accounts, problems, subs, rules);
    let revealed: Vec<Sub> = subs.iter().cloned().map(|mut s| {
        s.hidden = false;
        s
    }).collect();
    let groups = group(&revealed);
    // "First to solve" as on the final board (a frozen accept can be the first).
    let final_first: HashMap<i64, Vec<bool>> = icpc_board(accounts, problems, &revealed, rules)
        .into_iter()
        .map(|r| (r.account, r.cells.iter().map(|c| c.first).collect()))
        .collect();
    let mut rows = frozen.clone();
    let mut ac: HashMap<i64, Vec<i64>> = HashMap::new();
    // Accept times of the cells known on the frozen board.
    let frozen_groups = group(subs);
    for r in &rows {
        let v = problems
            .iter()
            .map(|p| frozen_groups.get(&(r.account, p.as_str())).map(|v| icpc_cell(v, rules).1).unwrap_or(0))
            .collect();
        ac.insert(r.account, v);
    }
    let mut steps = Vec::new();
    loop {
        let Some(ri) = rows.iter().rposition(|r| r.cells.iter().any(|c| c.pending > 0)) else { break };
        let pi = rows[ri].cells.iter().position(|c| c.pending > 0).unwrap();
        let account = rows[ri].account;
        let (mut cell, t) =
            groups.get(&(account, problems[pi].as_str())).map(|v| icpc_cell(v, rules)).unwrap_or_default();
        cell.first = final_first.get(&account).is_some_and(|f| f[pi]);
        let times = ac.get_mut(&account).unwrap();
        times[pi] = t;
        rows[ri].cells[pi] = cell.clone();
        let times = times.clone();
        icpc_totals(&mut rows[ri], rules, &times);
        steps.push(ResolveStep {
            account,
            problem: pi,
            cell,
            solved: rows[ri].solved,
            penalty: rows[ri].penalty,
            last_ac_ms: rows[ri].last_ac_ms,
        });
        icpc_rank(&mut rows);
    }
    (frozen, steps)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sub(id: i64, account: i64, p: &str, min: i64, o: Outcome) -> Sub {
        Sub {
            id,
            account,
            problem: p.into(),
            time_ms: min * 60_000,
            outcome: o,
            score: 0.0,
            subtask_scores: vec![],
            hidden: false,
        }
    }
    const R: IcpcRules = IcpcRules { penalty_min: 20, compile_error_penalty: false };
    fn probs() -> Vec<String> {
        vec!["A".into(), "B".into()]
    }

    #[test]
    fn icpc_penalty_and_ranking() {
        use Outcome::*;
        let subs = vec![
            sub(1, 1, "A", 10, Rejected),
            sub(2, 1, "A", 15, CompileError), // no penalty by default
            sub(3, 1, "A", 20, Accepted),     // 20 + 20
            sub(4, 1, "A", 30, Rejected),     // after accept: ignored
            sub(5, 2, "A", 5, Accepted),      // 5, first to solve
            sub(6, 2, "B", 50, Accepted),     // 50
            sub(7, 3, "B", 70, Rejected),
        ];
        let rows = icpc_board(&[1, 2, 3], &probs(), &subs, &R);
        assert_eq!(rows.iter().map(|r| r.account).collect::<Vec<_>>(), vec![2, 1, 3]);
        let t2 = &rows[0];
        assert_eq!((t2.solved, t2.penalty, t2.rank), (2, 55, 1));
        assert!(t2.cells[0].first && t2.cells[1].first);
        let t1 = &rows[1];
        assert_eq!((t1.solved, t1.penalty, t1.rank), (1, 40, 2));
        assert_eq!((t1.cells[0].attempts, t1.cells[0].time_min, t1.cells[0].first), (2, 20, false));
        assert_eq!((rows[2].cells[1].attempts, rows[2].solved, rows[2].rank), (1, 0, 3));

        let ce = IcpcRules { compile_error_penalty: true, ..R };
        assert_eq!(icpc_board(&[1], &probs(), &subs, &ce)[0].penalty, 60);
    }

    #[test]
    fn icpc_ties_share_rank() {
        let subs = vec![sub(1, 1, "A", 10, Outcome::Accepted), sub(2, 2, "A", 10, Outcome::Accepted)];
        let rows = icpc_board(&[1, 2, 3], &probs(), &subs, &R);
        assert_eq!(rows.iter().map(|r| r.rank).collect::<Vec<_>>(), vec![1, 1, 3]);
        assert!(rows[0].cells[0].first && rows[1].cells[0].first);
    }

    #[test]
    fn pending_hides_later_results() {
        use Outcome::*;
        let mut subs = vec![sub(1, 1, "A", 10, Rejected), sub(2, 1, "A", 12, Pending), sub(3, 1, "A", 14, Accepted)];
        let rows = icpc_board(&[1], &probs(), &subs, &R);
        assert_eq!((rows[0].cells[0].pending, rows[0].cells[0].solved, rows[0].cells[0].attempts), (2, false, 3));
        subs[1].outcome = Rejected;
        let rows = icpc_board(&[1], &probs(), &subs, &R);
        assert_eq!((rows[0].cells[0].solved, rows[0].penalty), (true, 14 + 40));
    }

    #[test]
    fn freeze_and_resolver_reach_final_board() {
        use Outcome::*;
        let mut subs = vec![
            sub(1, 1, "A", 10, Accepted),
            sub(2, 2, "A", 20, Accepted),
            sub(3, 2, "B", 250, Accepted), // frozen: team 2 overtakes team 1 at the end
            sub(4, 3, "A", 255, Rejected), // frozen
            sub(5, 1, "B", 260, Rejected), // frozen
        ];
        for s in subs.iter_mut() {
            s.hidden = s.time_ms >= 240 * 60_000;
        }
        let (frozen, steps) = resolve(&[1, 2, 3], &probs(), &subs, &R);
        assert_eq!(frozen.iter().map(|r| r.account).collect::<Vec<_>>(), vec![1, 2, 3]);
        assert_eq!(frozen[1].cells[1].pending, 1);
        // Lowest team first: team 3, then team 2 (B, solved), then team 1 (B, rejected).
        assert_eq!(steps.iter().map(|s| (s.account, s.problem)).collect::<Vec<_>>(), vec![(3, 0), (2, 1), (1, 1)]);
        assert_eq!((steps[1].solved, steps[1].penalty), (2, 270));

        let open: Vec<Sub> = subs.iter().cloned().map(|mut s| {
            s.hidden = false;
            s
        }).collect();
        let fin = icpc_board(&[1, 2, 3], &probs(), &open, &R);
        assert_eq!(fin.iter().map(|r| r.account).collect::<Vec<_>>(), vec![2, 1, 3]);
    }

    #[test]
    fn ioi_scoring_modes() {
        let mk = |id, score: f64, st: Vec<f64>| Sub {
            id,
            account: 1,
            problem: "A".into(),
            time_ms: id * 1000,
            outcome: Outcome::Rejected,
            score,
            subtask_scores: st,
            hidden: false,
        };
        let subs = vec![mk(1, 30.0, vec![30.0, 0.0]), mk(2, 50.0, vec![0.0, 50.0])];
        let p = vec!["A".to_string()];
        assert_eq!(ioi_board(&[1], &p, &subs, IoiScoring::MaxSubtask)[0].total, 80.0);
        assert_eq!(ioi_board(&[1], &p, &subs, IoiScoring::MaxSubmission)[0].total, 50.0);
        let mut pend = subs.clone();
        pend[1].hidden = true;
        let row = &ioi_board(&[1], &p, &pend, IoiScoring::MaxSubtask)[0];
        assert_eq!((row.total, row.cells[0].pending, row.cells[0].attempts), (30.0, 1, 2));
    }

    #[test]
    fn score_board_ranks() {
        let mut m = HashMap::new();
        m.insert((1, "A".to_string()), 100.0);
        m.insert((2, "A".to_string()), 100.0);
        m.insert((3, "A".to_string()), 40.0);
        let rows = score_board(&[1, 2, 3], &["A".to_string()], &m);
        assert_eq!(rows.iter().map(|r| r.rank).collect::<Vec<_>>(), vec![1, 1, 3]);
    }
}
