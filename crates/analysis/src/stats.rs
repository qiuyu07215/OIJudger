//! Score statistics: the LemonLime set (distribution with prefix / suffix
//! sums, mean, standard deviation, discrimination, test case pass rates)
//! plus medians, quartiles, problem correlation and group comparison.

use std::collections::{BTreeMap, HashMap};

use judge_core::{Contest, ProblemResult, Status, Verdict};
use serde::Serialize;

#[derive(Debug, Clone, Serialize, Default, PartialEq)]
pub struct Summary {
    /// Number of contestants considered.
    pub count: usize,
    /// Contestants with a submission (not "missing").
    pub submitted: usize,
    pub full: f64,
    pub mean: f64,
    pub std_dev: f64,
    pub min: f64,
    pub q1: f64,
    pub median: f64,
    pub q3: f64,
    pub max: f64,
    /// Contestants with the full score.
    pub full_count: usize,
    /// Contestants with zero.
    pub zero_count: usize,
    /// (mean of top 27% − mean of bottom 27%) / full, in [−1, 1].
    pub discrimination: f64,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Bucket {
    /// Inclusive lower bound.
    pub lo: f64,
    /// Exclusive upper bound (inclusive for the last bucket).
    pub hi: f64,
    pub count: usize,
    pub ratio: f64,
    /// Contestants with a score below `hi` (cumulative from the bottom).
    pub prefix: usize,
    /// Contestants with a score at least `lo` (cumulative from the top).
    pub suffix: usize,
}

/// One row per distinct score (LemonLime's table).
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct ScoreCount {
    pub score: f64,
    pub count: usize,
    pub ratio: f64,
    /// Contestants scoring at most this.
    pub prefix: usize,
    /// Contestants scoring at least this.
    pub suffix: usize,
}

fn quantile(sorted: &[f64], q: f64) -> f64 {
    if sorted.is_empty() {
        return 0.0;
    }
    let pos = q * (sorted.len() - 1) as f64;
    let (lo, hi) = (pos.floor() as usize, pos.ceil() as usize);
    sorted[lo] + (sorted[hi] - sorted[lo]) * (pos - lo as f64)
}

/// `scores[i]` belongs to the contestant ranked by `order_key[i]` (usually
/// the total score); discrimination compares the top and bottom 27% by that
/// key, which for a single problem is the classical item discrimination.
pub fn summarize(scores: &[f64], order_key: &[f64], full: f64, submitted: usize) -> Summary {
    let n = scores.len();
    if n == 0 {
        return Summary { full, ..Default::default() };
    }
    let mean = scores.iter().sum::<f64>() / n as f64;
    let var = scores.iter().map(|s| (s - mean).powi(2)).sum::<f64>() / n as f64;
    let mut sorted = scores.to_vec();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let mut idx: Vec<usize> = (0..n).collect();
    idx.sort_by(|&a, &b| order_key[b].partial_cmp(&order_key[a]).unwrap().then(a.cmp(&b)));
    let g = ((n as f64) * 0.27).round().max(1.0) as usize;
    let g = g.min(n);
    let top = idx[..g].iter().map(|&i| scores[i]).sum::<f64>() / g as f64;
    let bottom = idx[n - g..].iter().map(|&i| scores[i]).sum::<f64>() / g as f64;
    Summary {
        count: n,
        submitted,
        full,
        mean,
        std_dev: var.sqrt(),
        min: sorted[0],
        q1: quantile(&sorted, 0.25),
        median: quantile(&sorted, 0.5),
        q3: quantile(&sorted, 0.75),
        max: sorted[n - 1],
        full_count: scores.iter().filter(|&&s| full > 0.0 && s >= full - 1e-9).count(),
        zero_count: scores.iter().filter(|&&s| s <= 1e-9).count(),
        discrimination: if full > 0.0 && n >= 2 { (top - bottom) / full } else { 0.0 },
    }
}

pub fn score_counts(scores: &[f64]) -> Vec<ScoreCount> {
    let n = scores.len();
    let mut m: BTreeMap<i64, usize> = BTreeMap::new();
    for &s in scores {
        *m.entry((s * 1000.0).round() as i64).or_default() += 1;
    }
    let mut prefix = 0;
    let mut out: Vec<ScoreCount> = m
        .into_iter()
        .map(|(k, count)| {
            prefix += count;
            ScoreCount { score: k as f64 / 1000.0, count, ratio: count as f64 / n as f64, prefix, suffix: 0 }
        })
        .collect();
    let mut suffix = 0;
    for r in out.iter_mut().rev() {
        suffix += r.count;
        r.suffix = suffix;
    }
    out.reverse(); // highest score first
    out
}

/// Fixed-width histogram over [0, full].
pub fn histogram(scores: &[f64], full: f64, buckets: usize) -> Vec<Bucket> {
    let buckets = buckets.max(1);
    let n = scores.len().max(1);
    let width = if full > 0.0 { full / buckets as f64 } else { 1.0 };
    let mut counts = vec![0usize; buckets];
    for &s in scores {
        let i = if width > 0.0 { ((s / width).floor() as isize).clamp(0, buckets as isize - 1) as usize } else { 0 };
        counts[i] += 1;
    }
    let mut prefix = 0;
    let total: usize = counts.iter().sum();
    let mut out = Vec::with_capacity(buckets);
    for (i, &c) in counts.iter().enumerate() {
        let suffix = total - prefix;
        prefix += c;
        out.push(Bucket {
            lo: i as f64 * width,
            hi: (i + 1) as f64 * width,
            count: c,
            ratio: c as f64 / n as f64,
            prefix,
            suffix,
        });
    }
    out
}

pub fn pearson(a: &[f64], b: &[f64]) -> f64 {
    let n = a.len().min(b.len());
    if n < 2 {
        return 0.0;
    }
    let ma = a[..n].iter().sum::<f64>() / n as f64;
    let mb = b[..n].iter().sum::<f64>() / n as f64;
    let (mut cov, mut va, mut vb) = (0.0, 0.0, 0.0);
    for i in 0..n {
        cov += (a[i] - ma) * (b[i] - mb);
        va += (a[i] - ma).powi(2);
        vb += (b[i] - mb).powi(2);
    }
    if va <= 0.0 || vb <= 0.0 {
        0.0
    } else {
        cov / (va * vb).sqrt()
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct SubtaskStat {
    pub full: f64,
    pub mean: f64,
    /// Fraction of judged contestants with the full subtask score.
    pub pass_rate: f64,
    /// Same, per case (fraction of AC among contestants who ran it).
    pub case_pass_rates: Vec<f64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ProblemStat {
    pub name: String,
    pub title: String,
    pub summary: Summary,
    pub counts: Vec<ScoreCount>,
    pub histogram: Vec<Bucket>,
    /// Result labels (AC, WA, CE, Missing, ...) → contestants.
    pub labels: BTreeMap<String, usize>,
    /// Case verdicts over all contestants.
    pub verdicts: BTreeMap<String, usize>,
    pub subtasks: Vec<SubtaskStat>,
    /// Mean of max case time among accepted solutions (ms).
    pub mean_time_ms: f64,
}

#[derive(Debug, Clone, Serialize)]
pub struct GroupStat {
    pub key: String,
    pub value: String,
    pub summary: Summary,
    /// Mean score per problem (contest order).
    pub problem_means: Vec<f64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ContestStats {
    pub contestants: usize,
    pub overall: Summary,
    pub counts: Vec<ScoreCount>,
    pub histogram: Vec<Bucket>,
    pub problems: Vec<ProblemStat>,
    /// Pearson correlation between problem scores, `[i][j]`.
    pub correlation: Vec<Vec<f64>>,
    /// Totals in ranking order (for box plots / scatter).
    pub totals: Vec<f64>,
    /// Scores per problem, same contestant order as `totals`.
    pub problem_scores: Vec<Vec<f64>>,
    /// Statistics per contestant group (from contestants.csv).
    pub groups: Vec<GroupStat>,
    pub group_keys: Vec<String>,
}

/// Contestant metadata from `contestants.csv`: name → (column → value).
pub type Groups = HashMap<String, BTreeMap<String, String>>;

/// Parse `contestants.csv` (first column = contestant name, header row
/// names the other columns). Accepts comma, semicolon or tab separators and
/// a UTF-8 BOM.
pub fn parse_groups(text: &str) -> (Vec<String>, Groups) {
    let text = text.trim_start_matches('\u{feff}');
    let mut lines = text.lines().filter(|l| !l.trim().is_empty());
    let Some(header) = lines.next() else { return (Vec::new(), Groups::new()) };
    let sep = [',', ';', '\t'].into_iter().max_by_key(|c| header.matches(*c).count()).unwrap_or(',');
    let split = |l: &str| -> Vec<String> { l.split(sep).map(|x| x.trim().trim_matches('"').to_string()).collect() };
    let cols = split(header);
    let keys: Vec<String> = cols.iter().skip(1).cloned().collect();
    let mut out = Groups::new();
    for l in lines {
        let v = split(l);
        let Some(name) = v.first().filter(|n| !n.is_empty()) else { continue };
        let m: BTreeMap<String, String> = keys
            .iter()
            .zip(v.iter().skip(1))
            .filter(|(_, x)| !x.is_empty())
            .map(|(k, x)| (k.clone(), x.clone()))
            .collect();
        out.insert(name.clone(), m);
    }
    (keys, out)
}

pub fn compute(contest: &Contest, contestants: &[String], results: &[ProblemResult], groups: &Groups) -> ContestStats {
    let by: HashMap<(&str, &str), &ProblemResult> =
        results.iter().map(|r| ((r.contestant.as_str(), r.problem.as_str()), r)).collect();
    let probs = &contest.problems;
    let n = contestants.len();

    // Score matrix (missing result = 0).
    let mut matrix: Vec<Vec<f64>> = vec![vec![0.0; n]; probs.len()];
    let mut totals = vec![0.0; n];
    for (ci, c) in contestants.iter().enumerate() {
        for (pi, p) in probs.iter().enumerate() {
            let s = by.get(&(c.as_str(), p.name.as_str())).map(|r| r.score).unwrap_or(0.0);
            matrix[pi][ci] = s;
            totals[ci] += s;
        }
    }
    let full_total = contest.full_score();
    let submitted_any = contestants
        .iter()
        .filter(|c| {
            probs.iter().any(|p| by.get(&(c.as_str(), p.name.as_str())).is_some_and(|r| r.status != Status::NoSource))
        })
        .count();

    let problems = probs
        .iter()
        .enumerate()
        .map(|(pi, p)| {
            let full = p.full_score();
            let mut labels: BTreeMap<String, usize> = BTreeMap::new();
            let mut verdicts: BTreeMap<String, usize> = BTreeMap::new();
            let mut submitted = 0;
            let mut sub_full = vec![0usize; p.subtasks.len()];
            let mut sub_sum = vec![0.0f64; p.subtasks.len()];
            let mut case_ac: Vec<Vec<(usize, usize)>> =
                p.subtasks.iter().map(|s| vec![(0, 0); s.cases.len()]).collect();
            let mut judged = 0usize;
            let mut times: Vec<f64> = Vec::new();
            for c in contestants {
                let Some(r) = by.get(&(c.as_str(), p.name.as_str())) else {
                    *labels.entry("Missing".into()).or_default() += 1;
                    continue;
                };
                *labels.entry(r.label().to_string()).or_default() += 1;
                if r.status != Status::NoSource {
                    submitted += 1;
                }
                if r.status != Status::Judged {
                    continue;
                }
                judged += 1;
                if r.score >= full - 1e-9 {
                    if let Some(m) = r.subtasks.iter().flat_map(|s| &s.cases).map(|c| c.time_ms).max() {
                        times.push(m as f64);
                    }
                }
                for (si, s) in r.subtasks.iter().enumerate().take(p.subtasks.len()) {
                    sub_sum[si] += s.score;
                    if s.score >= s.full - 1e-9 && s.full > 0.0 {
                        sub_full[si] += 1;
                    }
                    for (ki, cs) in s.cases.iter().enumerate() {
                        if cs.verdict != Verdict::Skipped {
                            *verdicts.entry(cs.verdict.short().to_string()).or_default() += 1;
                        }
                        if let Some(slot) = case_ac.get_mut(si).and_then(|v| v.get_mut(ki)) {
                            if cs.verdict != Verdict::Skipped {
                                slot.1 += 1;
                                if cs.verdict == Verdict::AC {
                                    slot.0 += 1;
                                }
                            }
                        }
                    }
                }
            }
            let jd = judged.max(1) as f64;
            let subtasks = p
                .subtasks
                .iter()
                .enumerate()
                .map(|(si, s)| SubtaskStat {
                    full: s.score,
                    mean: sub_sum[si] / jd,
                    pass_rate: sub_full[si] as f64 / jd,
                    case_pass_rates: case_ac[si]
                        .iter()
                        .map(|&(ac, ran)| if ran == 0 { 0.0 } else { ac as f64 / ran as f64 })
                        .collect(),
                })
                .collect();
            ProblemStat {
                name: p.name.clone(),
                title: p.display_title().to_string(),
                summary: summarize(&matrix[pi], &totals, full, submitted),
                counts: score_counts(&matrix[pi]),
                histogram: histogram(&matrix[pi], full, 10),
                labels,
                verdicts,
                subtasks,
                mean_time_ms: if times.is_empty() { 0.0 } else { times.iter().sum::<f64>() / times.len() as f64 },
            }
        })
        .collect();

    let correlation = (0..probs.len())
        .map(|i| (0..probs.len()).map(|j| if i == j { 1.0 } else { pearson(&matrix[i], &matrix[j]) }).collect())
        .collect();

    // Groups: every (column, value) pair.
    let mut group_keys: Vec<String> = groups.values().flat_map(|m| m.keys().cloned()).collect();
    group_keys.sort();
    group_keys.dedup();
    let mut gs: Vec<GroupStat> = Vec::new();
    for key in &group_keys {
        let mut values: BTreeMap<String, Vec<usize>> = BTreeMap::new();
        for (ci, c) in contestants.iter().enumerate() {
            if let Some(v) = groups.get(c).and_then(|m| m.get(key)) {
                values.entry(v.clone()).or_default().push(ci);
            }
        }
        for (value, idx) in values {
            let t: Vec<f64> = idx.iter().map(|&i| totals[i]).collect();
            gs.push(GroupStat {
                key: key.clone(),
                value,
                summary: summarize(&t, &t, full_total, idx.len()),
                problem_means: (0..probs.len())
                    .map(|pi| idx.iter().map(|&i| matrix[pi][i]).sum::<f64>() / idx.len().max(1) as f64)
                    .collect(),
            });
        }
    }

    // Ranking order for the raw arrays.
    let mut order: Vec<usize> = (0..n).collect();
    order.sort_by(|&a, &b| totals[b].partial_cmp(&totals[a]).unwrap());
    ContestStats {
        contestants: n,
        overall: summarize(&totals, &totals, full_total, submitted_any),
        counts: score_counts(&totals),
        histogram: histogram(&totals, full_total, 20),
        problems,
        correlation,
        totals: order.iter().map(|&i| totals[i]).collect(),
        problem_scores: matrix.iter().map(|row| order.iter().map(|&i| row[i]).collect()).collect(),
        groups: gs,
        group_keys,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn summary_basics() {
        let s = [0.0, 50.0, 100.0, 100.0];
        let sm = summarize(&s, &s, 100.0, 4);
        assert_eq!(sm.mean, 62.5);
        assert_eq!(sm.median, 75.0);
        assert_eq!(sm.full_count, 2);
        assert_eq!(sm.zero_count, 1);
        assert!(sm.discrimination > 0.9);
    }

    #[test]
    fn counts_prefix_suffix() {
        let c = score_counts(&[10.0, 10.0, 20.0, 30.0]);
        assert_eq!(c[0].score, 30.0);
        assert_eq!(c[0].suffix, 1);
        assert_eq!(c[0].prefix, 4);
        assert_eq!(c[2].score, 10.0);
        assert_eq!(c[2].count, 2);
        assert_eq!(c[2].prefix, 2);
        assert_eq!(c[2].suffix, 4);
    }

    #[test]
    fn hist() {
        let h = histogram(&[0.0, 5.0, 10.0, 99.0, 100.0], 100.0, 10);
        assert_eq!(h.len(), 10);
        assert_eq!(h[0].count, 2);
        assert_eq!(h[9].count, 2);
        assert_eq!(h.iter().map(|b| b.count).sum::<usize>(), 5);
    }

    #[test]
    fn correlation() {
        assert!((pearson(&[1.0, 2.0, 3.0], &[2.0, 4.0, 6.0]) - 1.0).abs() < 1e-9);
        assert!((pearson(&[1.0, 2.0, 3.0], &[3.0, 2.0, 1.0]) + 1.0).abs() < 1e-9);
    }

    #[test]
    fn groups_csv() {
        let (keys, g) = parse_groups("\u{feff}name,school,grade\nalice,A,9\nbob;x\ncarol,B,\n");
        assert_eq!(keys, vec!["school", "grade"]);
        assert_eq!(g["alice"]["grade"], "9");
        assert!(!g["carol"].contains_key("grade"));
    }
}
