//! Export results as CSV, Excel workbook or a self-contained HTML report.

use std::collections::HashMap;
use std::fmt::Write as _;
use std::path::Path;

use analysis::ContestStats;
use anyhow::Result;
use judge_core::{Contest, ProblemResult, Status, Verdict};
use rust_xlsxwriter::{Color, Format, FormatAlign, FormatBorder, Workbook};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Lang {
    Zh,
    En,
}

impl Lang {
    pub fn from_code(code: &str) -> Lang {
        if code.starts_with("en") {
            Lang::En
        } else {
            Lang::Zh
        }
    }
    fn t(self, key: &str) -> &'static str {
        let zh = self == Lang::Zh;
        match key {
            "rank" => {
                if zh {
                    "名次"
                } else {
                    "Rank"
                }
            }
            "contestant" => {
                if zh {
                    "选手"
                } else {
                    "Contestant"
                }
            }
            "total" => {
                if zh {
                    "总分"
                } else {
                    "Total"
                }
            }
            "time" => {
                if zh {
                    "用时(ms)"
                } else {
                    "Time (ms)"
                }
            }
            "results" => {
                if zh {
                    "成绩"
                } else {
                    "Results"
                }
            }
            "stats" => {
                if zh {
                    "统计"
                } else {
                    "Statistics"
                }
            }
            "problem" => {
                if zh {
                    "试题"
                } else {
                    "Problem"
                }
            }
            "full" => {
                if zh {
                    "满分"
                } else {
                    "Full"
                }
            }
            "mean" => {
                if zh {
                    "平均分"
                } else {
                    "Mean"
                }
            }
            "median" => {
                if zh {
                    "中位数"
                } else {
                    "Median"
                }
            }
            "std" => {
                if zh {
                    "标准差"
                } else {
                    "Std. dev."
                }
            }
            "max" => {
                if zh {
                    "最高分"
                } else {
                    "Max"
                }
            }
            "fullcount" => {
                if zh {
                    "满分人数"
                } else {
                    "Full scores"
                }
            }
            "zerocount" => {
                if zh {
                    "零分人数"
                } else {
                    "Zero scores"
                }
            }
            "submitted" => {
                if zh {
                    "提交人数"
                } else {
                    "Submitted"
                }
            }
            "disc" => {
                if zh {
                    "区分度"
                } else {
                    "Discrimination"
                }
            }
            "overall" => {
                if zh {
                    "总分"
                } else {
                    "Overall"
                }
            }
            "details" => {
                if zh {
                    "详细结果"
                } else {
                    "Details"
                }
            }
            "subtask" => {
                if zh {
                    "测试点"
                } else {
                    "Test point"
                }
            }
            "input" => {
                if zh {
                    "输入文件"
                } else {
                    "Input"
                }
            }
            "verdict" => {
                if zh {
                    "结果"
                } else {
                    "Verdict"
                }
            }
            "score" => {
                if zh {
                    "得分"
                } else {
                    "Score"
                }
            }
            "memory" => {
                if zh {
                    "内存(MiB)"
                } else {
                    "Memory (MiB)"
                }
            }
            "message" => {
                if zh {
                    "信息"
                } else {
                    "Message"
                }
            }
            "missing" => {
                if zh {
                    "缺少文件"
                } else {
                    "Missing"
                }
            }
            "ce" => {
                if zh {
                    "编译错误"
                } else {
                    "Compile error"
                }
            }
            "se" => {
                if zh {
                    "系统错误"
                } else {
                    "System error"
                }
            }
            "notjudged" => {
                if zh {
                    "未评测"
                } else {
                    "Not judged"
                }
            }
            "generated" => {
                if zh {
                    "由 OIJudger 生成"
                } else {
                    "Generated by OIJudger"
                }
            }
            "distribution" => {
                if zh {
                    "分数分布"
                } else {
                    "Score distribution"
                }
            }
            "count" => {
                if zh {
                    "人数"
                } else {
                    "Count"
                }
            }
            "ratio" => {
                if zh {
                    "比例"
                } else {
                    "Share"
                }
            }
            "atleast" => {
                if zh {
                    "不低于此分"
                } else {
                    "At least"
                }
            }
            "atmost" => {
                if zh {
                    "不高于此分"
                } else {
                    "At most"
                }
            }
            _ => "",
        }
    }
}

/// Everything an export needs.
pub struct ExportData<'a> {
    pub contest: &'a Contest,
    pub contestants: &'a [String],
    pub results: &'a [ProblemResult],
    pub stats: &'a ContestStats,
    pub lang: Lang,
}

struct Row<'a> {
    rank: usize,
    name: &'a str,
    total: f64,
    time: u64,
    cells: Vec<Option<&'a ProblemResult>>,
}

fn ranking<'a>(d: &'a ExportData<'a>) -> Vec<Row<'a>> {
    let by: HashMap<(&str, &str), &ProblemResult> =
        d.results.iter().map(|r| ((r.contestant.as_str(), r.problem.as_str()), r)).collect();
    let mut rows: Vec<Row> = d
        .contestants
        .iter()
        .map(|c| {
            let cells: Vec<Option<&ProblemResult>> =
                d.contest.problems.iter().map(|p| by.get(&(c.as_str(), p.name.as_str())).copied()).collect();
            let total = cells.iter().flatten().map(|r| r.score).sum();
            let time = cells.iter().flatten().map(|r| r.time_ms).sum();
            Row { rank: 0, name: c, total, time, cells }
        })
        .collect();
    rows.sort_by(|a, b| {
        b.total
            .partial_cmp(&a.total)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| judge_core::discover::natural_cmp(a.name, b.name))
    });
    for i in 0..rows.len() {
        rows[i].rank = if i > 0 && (rows[i].total - rows[i - 1].total).abs() < 1e-9 { rows[i - 1].rank } else { i + 1 };
    }
    rows
}

pub fn fmt_score(x: f64) -> String {
    if (x - x.round()).abs() < 1e-9 {
        format!("{}", x.round() as i64)
    } else {
        let s = format!("{x:.2}");
        s.trim_end_matches('0').trim_end_matches('.').to_string()
    }
}

fn cell_text(r: Option<&ProblemResult>, lang: Lang) -> String {
    match r {
        None => lang.t("notjudged").into(),
        Some(r) => match r.status {
            Status::Judged => fmt_score(r.score),
            Status::NoSource => lang.t("missing").into(),
            Status::CompileError => lang.t("ce").into(),
            Status::SystemError => lang.t("se").into(),
        },
    }
}

fn csv_field(s: &str) -> String {
    if s.contains([',', '"', '\n', '\r']) {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_string()
    }
}

/// CSV with a UTF-8 BOM so Excel detects the encoding.
pub fn to_csv(d: &ExportData) -> String {
    let l = d.lang;
    let mut out = String::from("\u{feff}");
    let mut header = vec![l.t("rank").to_string(), l.t("contestant").to_string(), l.t("total").to_string()];
    header.extend(d.contest.problems.iter().map(|p| p.display_title().to_string()));
    header.push(l.t("time").to_string());
    out.push_str(&header.iter().map(|h| csv_field(h)).collect::<Vec<_>>().join(","));
    out.push_str("\r\n");
    for r in ranking(d) {
        let mut f = vec![r.rank.to_string(), csv_field(r.name), fmt_score(r.total)];
        f.extend(r.cells.iter().map(|c| csv_field(&cell_text(*c, l))));
        f.push(r.time.to_string());
        out.push_str(&f.join(","));
        out.push_str("\r\n");
    }
    out
}

fn ratio_color(r: Option<&ProblemResult>) -> u32 {
    match r {
        None => 0xFFFFFF,
        Some(r) => match r.status {
            Status::NoSource => 0xF1F3F4,
            Status::CompileError => 0xECE3F8,
            Status::SystemError => 0xFFF1C2,
            Status::Judged => {
                if r.full <= 0.0 || r.score <= 1e-9 {
                    0xFBE0DD
                } else if r.score >= r.full - 1e-9 {
                    0xD9F2DD
                } else {
                    0xFFF0C8
                }
            }
        },
    }
}

/// Excel workbook: ranking (colour coded), statistics, one sheet per problem.
pub fn to_xlsx(d: &ExportData, path: &Path) -> Result<()> {
    let l = d.lang;
    let mut wb = Workbook::new();
    let head = Format::new()
        .set_bold()
        .set_background_color(Color::RGB(0x2F7D4F))
        .set_font_color(Color::White)
        .set_border(FormatBorder::Thin);
    let plain = Format::new().set_border(FormatBorder::Thin);
    let bold = Format::new().set_bold().set_border(FormatBorder::Thin);
    let rows = ranking(d);

    let ws = wb.add_worksheet().set_name(l.t("results"))?;
    let mut col = 0u16;
    for h in [l.t("rank"), l.t("contestant"), l.t("total")] {
        ws.write_with_format(0, col, h, &head)?;
        col += 1;
    }
    for p in &d.contest.problems {
        ws.write_with_format(0, col, p.display_title(), &head)?;
        col += 1;
    }
    ws.write_with_format(0, col, l.t("time"), &head)?;
    ws.set_column_width(1, 18)?;
    ws.set_freeze_panes(1, 3)?;
    for (i, r) in rows.iter().enumerate() {
        let row = (i + 1) as u32;
        ws.write_with_format(row, 0, r.rank as f64, &plain)?;
        ws.write_with_format(row, 1, r.name, &plain)?;
        ws.write_with_format(row, 2, r.total, &bold)?;
        for (j, c) in r.cells.iter().enumerate() {
            let fmt = Format::new()
                .set_border(FormatBorder::Thin)
                .set_align(FormatAlign::Center)
                .set_background_color(Color::RGB(ratio_color(*c)));
            let colj = 3 + j as u16;
            match c {
                Some(res) if res.status == Status::Judged => ws.write_with_format(row, colj, res.score, &fmt)?,
                _ => ws.write_with_format(row, colj, cell_text(*c, l), &fmt)?,
            };
        }
        ws.write_with_format(row, 3 + r.cells.len() as u16, r.time as f64, &plain)?;
    }

    // Statistics sheet.
    let st = d.stats;
    let ws = wb.add_worksheet().set_name(l.t("stats"))?;
    let heads = [
        l.t("problem"),
        l.t("full"),
        l.t("submitted"),
        l.t("mean"),
        l.t("median"),
        l.t("std"),
        l.t("max"),
        l.t("fullcount"),
        l.t("zerocount"),
        l.t("disc"),
    ];
    for (c, h) in heads.iter().enumerate() {
        ws.write_with_format(0, c as u16, *h, &head)?;
    }
    ws.set_column_width(0, 20)?;
    let mut put = |row: u32, name: &str, s: &analysis::stats::Summary| -> Result<()> {
        ws.write_with_format(row, 0, name, &bold)?;
        let vals = [
            s.full,
            s.submitted as f64,
            s.mean,
            s.median,
            s.std_dev,
            s.max,
            s.full_count as f64,
            s.zero_count as f64,
            s.discrimination,
        ];
        for (c, v) in vals.iter().enumerate() {
            ws.write_with_format(row, 1 + c as u16, (v * 1000.0).round() / 1000.0, &plain)?;
        }
        Ok(())
    };
    put(1, l.t("overall"), &st.overall)?;
    for (i, p) in st.problems.iter().enumerate() {
        put(2 + i as u32, &p.title, &p.summary)?;
    }

    // One sheet per problem: subtask scores per contestant.
    for (pi, p) in d.contest.problems.iter().enumerate() {
        let name: String = p.name.chars().filter(|c| !"[]:*?/\\".contains(*c)).take(28).collect();
        let ws = wb.add_worksheet().set_name(format!("{name}"))?;
        ws.write_with_format(0, 0, l.t("contestant"), &head)?;
        ws.write_with_format(0, 1, l.t("score"), &head)?;
        for (si, s) in p.subtasks.iter().enumerate() {
            ws.write_with_format(0, 2 + si as u16, format!("#{} ({})", si + 1, fmt_score(s.score)), &head)?;
        }
        ws.set_column_width(0, 18)?;
        ws.set_freeze_panes(1, 2)?;
        for (i, r) in rows.iter().enumerate() {
            let row = (i + 1) as u32;
            ws.write_with_format(row, 0, r.name, &plain)?;
            match r.cells[pi] {
                Some(res) if res.status == Status::Judged => {
                    ws.write_with_format(row, 1, res.score, &bold)?;
                    for (si, s) in res.subtasks.iter().enumerate() {
                        let fmt = Format::new()
                            .set_border(FormatBorder::Thin)
                            .set_align(FormatAlign::Center)
                            .set_background_color(Color::RGB(if s.score >= s.full - 1e-9 {
                                0xD9F2DD
                            } else if s.score > 0.0 {
                                0xFFF0C8
                            } else {
                                0xFBE0DD
                            }));
                        ws.write_with_format(row, 2 + si as u16, s.score, &fmt)?;
                    }
                }
                other => {
                    ws.write_with_format(row, 1, cell_text(other, l), &plain)?;
                }
            }
        }
    }
    wb.save(path)?;
    Ok(())
}

fn esc(s: &str) -> String {
    let mut o = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '<' => o.push_str("&lt;"),
            '>' => o.push_str("&gt;"),
            '&' => o.push_str("&amp;"),
            '"' => o.push_str("&quot;"),
            '\'' => o.push_str("&#39;"),
            _ => o.push(c),
        }
    }
    o
}

fn css_class(r: Option<&ProblemResult>) -> &'static str {
    match r {
        None => "nj",
        Some(r) => match r.status {
            Status::NoSource => "ms",
            Status::CompileError => "ce",
            Status::SystemError => "se",
            Status::Judged if r.full > 0.0 && r.score >= r.full - 1e-9 => "ac",
            Status::Judged if r.score <= 1e-9 => "zero",
            Status::Judged => "pc",
        },
    }
}

fn verdict_class(v: Verdict) -> &'static str {
    match v {
        Verdict::AC => "ac",
        Verdict::PC => "pc",
        Verdict::Skipped | Verdict::NoOutput => "ms",
        Verdict::SystemError => "se",
        _ => "zero",
    }
}

const CSS: &str = r#"
:root{--bg:#fff;--fg:#1f2328;--muted:#656d76;--line:#d0d7de;--head:#f6f8fa;--ac:#d9f2dd;--acf:#17602a;--zero:#fbe0dd;--zerof:#8e1d12;--pc:#fff0c8;--pcf:#7a5600;--ce:#ece3f8;--cef:#5b2a91;--ms:#f1f3f4;--msf:#80868b;--se:#fff1c2;--sef:#7a5600}
@media (prefers-color-scheme:dark){:root{--bg:#0d1117;--fg:#e6edf3;--muted:#8d96a0;--line:#30363d;--head:#161b22;--ac:#1d3b25;--acf:#8be39f;--zero:#43211d;--zerof:#ff9c8f;--pc:#3d3216;--pcf:#f1cf6b;--ce:#33264a;--cef:#c9a7f5;--ms:#1c2128;--msf:#6b7177;--se:#3d3216;--sef:#f1cf6b}}
body{margin:0;padding:24px 16px;background:var(--bg);color:var(--fg);font:14px/1.5 "Segoe UI","Microsoft YaHei UI",system-ui,sans-serif}
main{max-width:1200px;margin:0 auto}h1{font-size:22px;margin:0 0 4px}h2{font-size:17px;margin:28px 0 10px}
.muted{color:var(--muted);font-size:12px}a{color:inherit;text-decoration:underline dotted}.wrap{overflow-x:auto}
table{border-collapse:collapse;font-variant-numeric:tabular-nums}th,td{border:1px solid var(--line);padding:4px 8px;text-align:center;white-space:nowrap}
th{background:var(--head);position:sticky;top:0}td.name{text-align:left}td.l{text-align:left;white-space:normal;max-width:480px;font-family:Consolas,monospace;font-size:12px}
.ac{background:var(--ac);color:var(--acf)}.zero{background:var(--zero);color:var(--zerof)}.pc{background:var(--pc);color:var(--pcf)}
.ce{background:var(--ce);color:var(--cef)}.ms,.nj{background:var(--ms);color:var(--msf)}.se{background:var(--se);color:var(--sef)}
details{border:1px solid var(--line);border-radius:6px;margin:6px 0;padding:6px 10px}summary{cursor:pointer;font-weight:600}
pre{white-space:pre-wrap;font-size:12px;background:var(--head);padding:8px;border-radius:6px;max-height:300px;overflow:auto}
"#;

/// Self-contained HTML report. `detailed` adds every contestant's per-case
/// results (larger file, like LemonLime's `.html` vs `.htm`).
pub fn to_html(d: &ExportData, detailed: bool) -> String {
    let l = d.lang;
    let rows = ranking(d);
    let mut h = String::with_capacity(64 * 1024);
    let title = if d.contest.title.is_empty() { "OIJudger" } else { d.contest.title.as_str() };
    let _ = write!(
        h,
        "<!doctype html><html lang=\"{}\"><head><meta charset=\"utf-8\"><meta name=\"viewport\" content=\"width=device-width,initial-scale=1\"><title>{}</title><style>{CSS}</style></head><body><main>",
        if l == Lang::Zh { "zh-CN" } else { "en" },
        esc(title)
    );
    let _ = write!(
        h,
        "<h1>{}</h1><div class=\"muted\">{} · {} {}</div>",
        esc(title),
        l.t("generated"),
        d.contestants.len(),
        l.t("contestant")
    );

    // Ranking.
    let _ = write!(
        h,
        "<h2>{}</h2><div class=\"wrap\"><table><thead><tr><th>{}</th><th>{}</th><th>{}</th>",
        l.t("results"),
        l.t("rank"),
        l.t("contestant"),
        l.t("total")
    );
    for p in &d.contest.problems {
        let _ = write!(
            h,
            "<th>{}<br><span class=\"muted\">{}</span></th>",
            esc(p.display_title()),
            fmt_score(p.full_score())
        );
    }
    let _ = write!(h, "<th>{}</th></tr></thead><tbody>", l.t("time"));
    for r in &rows {
        let link = if detailed { format!("<a href=\"#c-{}\">{}</a>", esc(r.name), esc(r.name)) } else { esc(r.name) };
        let _ =
            write!(h, "<tr><td>{}</td><td class=\"name\">{}</td><td><b>{}</b></td>", r.rank, link, fmt_score(r.total));
        for c in &r.cells {
            let _ = write!(h, "<td class=\"{}\">{}</td>", css_class(*c), esc(&cell_text(*c, l)));
        }
        let _ = write!(h, "<td>{}</td></tr>", r.time);
    }
    h.push_str("</tbody></table></div>");

    // Statistics.
    let st = d.stats;
    let _ = write!(h, "<h2>{}</h2><div class=\"wrap\"><table><thead><tr>", l.t("stats"));
    for k in ["problem", "full", "submitted", "mean", "median", "std", "max", "fullcount", "zerocount", "disc"] {
        let _ = write!(h, "<th>{}</th>", l.t(k));
    }
    h.push_str("</tr></thead><tbody>");
    let mut stat_row = |name: &str, s: &analysis::stats::Summary| {
        let _ = write!(
            h,
            "<tr><td class=\"name\"><b>{}</b></td><td>{}</td><td>{}</td><td>{:.2}</td><td>{}</td><td>{:.2}</td><td>{}</td><td>{}</td><td>{}</td><td>{:.3}</td></tr>",
            esc(name), fmt_score(s.full), s.submitted, s.mean, fmt_score(s.median), s.std_dev, fmt_score(s.max), s.full_count, s.zero_count, s.discrimination
        );
    };
    stat_row(l.t("overall"), &st.overall);
    for p in &st.problems {
        stat_row(&p.title, &p.summary);
    }
    h.push_str("</tbody></table></div>");

    // Score distribution (LemonLime style).
    let _ = write!(
        h,
        "<h2>{}</h2><div class=\"wrap\"><table><thead><tr><th>{}</th><th>{}</th><th>{}</th><th>{}</th><th>{}</th></tr></thead><tbody>",
        l.t("distribution"), l.t("total"), l.t("count"), l.t("ratio"), l.t("atleast"), l.t("atmost")
    );
    for c in &st.counts {
        let _ = write!(
            h,
            "<tr><td>{}</td><td>{}</td><td>{:.1}%</td><td>{}</td><td>{}</td></tr>",
            fmt_score(c.score),
            c.count,
            c.ratio * 100.0,
            c.suffix,
            c.prefix
        );
    }
    h.push_str("</tbody></table></div>");

    if detailed {
        let _ = write!(h, "<h2>{}</h2>", l.t("details"));
        for r in &rows {
            let _ = write!(
                h,
                "<details id=\"c-{}\"><summary>#{} {} — {}</summary>",
                esc(r.name),
                r.rank,
                esc(r.name),
                fmt_score(r.total)
            );
            for (p, c) in d.contest.problems.iter().zip(&r.cells) {
                let _ = write!(
                    h,
                    "<h3>{} <span class=\"{}\" style=\"padding:0 6px;border-radius:4px\">{}</span></h3>",
                    esc(p.display_title()),
                    css_class(*c),
                    esc(&cell_text(*c, l))
                );
                let Some(res) = c else { continue };
                if res.status != Status::Judged {
                    if !res.message.is_empty() {
                        let _ = write!(h, "<pre>{}</pre>", esc(&res.message));
                    }
                    continue;
                }
                let _ = write!(h, "<div class=\"wrap\"><table><thead><tr><th>{}</th><th>{}</th><th>{}</th><th>{}</th><th>ms</th><th>{}</th><th>{}</th></tr></thead><tbody>",
                    l.t("subtask"), l.t("score"), l.t("input"), l.t("verdict"), l.t("memory"), l.t("message"));
                for (si, s) in res.subtasks.iter().enumerate() {
                    for (ci, cs) in s.cases.iter().enumerate() {
                        h.push_str("<tr>");
                        if ci == 0 {
                            let _ = write!(
                                h,
                                "<td rowspan=\"{}\">#{}</td><td rowspan=\"{}\">{} / {}</td>",
                                s.cases.len(),
                                si + 1,
                                s.cases.len(),
                                fmt_score(s.score),
                                fmt_score(s.full)
                            );
                        }
                        let _ = write!(
                            h,
                            "<td>{}</td><td class=\"{}\">{}</td><td>{}</td><td>{:.1}</td><td class=\"l\">{}</td></tr>",
                            esc(&cs.input),
                            verdict_class(cs.verdict),
                            cs.verdict.short(),
                            cs.time_ms,
                            cs.memory_kb as f64 / 1024.0,
                            esc(&cs.message)
                        );
                    }
                }
                h.push_str("</tbody></table></div>");
            }
            h.push_str("</details>");
        }
    }
    h.push_str("</main></body></html>");
    h
}

#[cfg(test)]
mod tests {
    use super::*;
    use judge_core::model::{Case, Problem, Subtask};

    fn sample() -> (Contest, Vec<String>, Vec<ProblemResult>) {
        let mut p = Problem::new("a");
        p.subtasks.push(Subtask {
            score: 100.0,
            time_ms: 1000,
            memory_mb: 256,
            cases: vec![Case { input: "a/1.in".into(), output: "a/1.out".into() }],
            depends: vec![],
        });
        let contest = Contest { title: "T<est>".into(), problems: vec![p] };
        let mut r1 = ProblemResult::empty("x,y", "a", 100.0, Status::Judged, "");
        r1.score = 100.0;
        let r2 = ProblemResult::empty("b", "a", 100.0, Status::CompileError, "err <b>");
        (contest, vec!["x,y".into(), "b".into()], vec![r1, r2])
    }

    #[test]
    fn csv_and_html() {
        let (c, names, rs) = sample();
        let stats = analysis::compute(&c, &names, &rs, &Default::default());
        let d = ExportData { contest: &c, contestants: &names, results: &rs, stats: &stats, lang: Lang::En };
        let csv = to_csv(&d);
        assert!(csv.contains("1,\"x,y\",100,100,0"), "{csv}");
        assert!(csv.contains("2,b,0,Compile error,0"), "{csv}");
        let html = to_html(&d, true);
        assert!(html.contains("T&lt;est&gt;"));
        assert!(html.contains("err &lt;b&gt;"));
        let tmp = tempfile::tempdir().unwrap();
        let p = tmp.path().join("r.xlsx");
        to_xlsx(&d, &p).unwrap();
        assert!(std::fs::metadata(&p).unwrap().len() > 1000);
    }
}
