//! Source similarity ("查重") for C / C++ submissions.
//!
//! Sources are tokenised with identifiers, literals and comments normalised
//! (renaming variables or reformatting does not help), hashed into k-grams
//! and reduced to fingerprints by winnowing (Schleimer et al., the MOSS
//! algorithm). Fingerprints shared by many submissions (fast-IO templates,
//! the given grader interface, ...) are ignored. Pairs are scored by the
//! share of the smaller submission's fingerprints found in the other one.

use std::collections::{HashMap, HashSet};

use serde::Serialize;

const KEYWORDS: &[&str] = &[
    "alignas",
    "alignof",
    "auto",
    "bool",
    "break",
    "case",
    "catch",
    "char",
    "class",
    "const",
    "constexpr",
    "continue",
    "decltype",
    "default",
    "delete",
    "do",
    "double",
    "else",
    "enum",
    "explicit",
    "extern",
    "false",
    "float",
    "for",
    "friend",
    "goto",
    "if",
    "inline",
    "int",
    "long",
    "mutable",
    "namespace",
    "new",
    "noexcept",
    "nullptr",
    "operator",
    "private",
    "protected",
    "public",
    "register",
    "return",
    "short",
    "signed",
    "sizeof",
    "static",
    "struct",
    "switch",
    "template",
    "this",
    "throw",
    "true",
    "try",
    "typedef",
    "typename",
    "union",
    "unsigned",
    "using",
    "virtual",
    "void",
    "volatile",
    "while",
];

/// Normalised token: kind/text hash plus its source line.
#[derive(Debug, Clone, Copy)]
struct Tok {
    h: u64,
    line: u32,
}

fn fnv(s: &[u8]) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for &b in s {
        h ^= b as u64;
        h = h.wrapping_mul(0x100_0000_01b3);
    }
    h
}

fn tokenize(src: &str) -> Vec<Tok> {
    let b = src.as_bytes();
    let mut out = Vec::with_capacity(b.len() / 3);
    let mut i = 0;
    let mut line = 1u32;
    let kw: HashSet<&[u8]> = KEYWORDS.iter().map(|k| k.as_bytes()).collect();
    let (h_ident, h_num, h_str, h_chr) = (fnv(b"<id>"), fnv(b"<num>"), fnv(b"<str>"), fnv(b"<chr>"));
    let mut at_line_start = true;
    while i < b.len() {
        let c = b[i];
        if c == b'\n' {
            line += 1;
            i += 1;
            at_line_start = true;
            continue;
        }
        if c.is_ascii_whitespace() {
            i += 1;
            continue;
        }
        // Preprocessor lines (#include, #define, ...) are skipped entirely.
        if c == b'#' && at_line_start {
            while i < b.len() && b[i] != b'\n' {
                if b[i] == b'\\' && i + 1 < b.len() && b[i + 1] == b'\n' {
                    line += 1;
                    i += 1;
                }
                i += 1;
            }
            continue;
        }
        at_line_start = false;
        if c == b'/' && i + 1 < b.len() && b[i + 1] == b'/' {
            while i < b.len() && b[i] != b'\n' {
                i += 1;
            }
            continue;
        }
        if c == b'/' && i + 1 < b.len() && b[i + 1] == b'*' {
            i += 2;
            while i + 1 < b.len() && !(b[i] == b'*' && b[i + 1] == b'/') {
                if b[i] == b'\n' {
                    line += 1;
                }
                i += 1;
            }
            i += 2;
            continue;
        }
        let start_line = line;
        if c == b'"' || c == b'\'' {
            let q = c;
            i += 1;
            while i < b.len() && b[i] != q && b[i] != b'\n' {
                if b[i] == b'\\' {
                    i += 1;
                }
                i += 1;
            }
            i += 1;
            out.push(Tok { h: if q == b'"' { h_str } else { h_chr }, line: start_line });
            continue;
        }
        if c.is_ascii_digit() || (c == b'.' && i + 1 < b.len() && b[i + 1].is_ascii_digit()) {
            while i < b.len() && (b[i].is_ascii_alphanumeric() || b[i] == b'.' || b[i] == b'\'') {
                i += 1;
            }
            out.push(Tok { h: h_num, line: start_line });
            continue;
        }
        if c.is_ascii_alphabetic() || c == b'_' || c >= 0x80 {
            let s = i;
            while i < b.len() && (b[i].is_ascii_alphanumeric() || b[i] == b'_' || b[i] >= 0x80) {
                i += 1;
            }
            let w = &b[s..i];
            out.push(Tok { h: if kw.contains(w) { fnv(w) } else { h_ident }, line: start_line });
            continue;
        }
        // Operators: longest of the common multi-character ones.
        let ops: [&[u8]; 20] = [
            b"<<=", b">>=", b"->", b"++", b"--", b"<<", b">>", b"<=", b">=", b"==", b"!=", b"&&", b"||", b"+=", b"-=",
            b"*=", b"/=", b"%=", b"::", b"^=",
        ];
        let mut len = 1;
        for op in ops {
            if b[i..].starts_with(op) {
                len = op.len();
                break;
            }
        }
        out.push(Tok { h: fnv(&b[i..i + len]), line: start_line });
        i += len;
    }
    out
}

/// A fingerprint and the source lines it covers.
#[derive(Debug, Clone, Copy)]
struct Print {
    h: u64,
    first_line: u32,
    last_line: u32,
}

fn fingerprints(toks: &[Tok], k: usize, w: usize) -> Vec<Print> {
    if toks.len() < k {
        return Vec::new();
    }
    let grams: Vec<Print> = (0..=toks.len() - k)
        .map(|i| {
            let mut h: u64 = 0;
            for t in &toks[i..i + k] {
                h = h.rotate_left(7) ^ t.h;
                h = h.wrapping_mul(0x9E37_79B9_7F4A_7C15);
            }
            Print { h, first_line: toks[i].line, last_line: toks[i + k - 1].line }
        })
        .collect();
    // Winnowing: the minimum hash of every window (rightmost on ties).
    let mut out: Vec<Print> = Vec::new();
    let mut last: Option<usize> = None;
    if grams.len() <= w {
        if let Some((i, _)) = grams.iter().enumerate().min_by_key(|(_, g)| g.h) {
            out.push(grams[i]);
        }
        return out;
    }
    for s in 0..=grams.len() - w {
        let mut m = s;
        for j in s..s + w {
            if grams[j].h <= grams[m].h {
                m = j;
            }
        }
        if last != Some(m) {
            out.push(grams[m]);
            last = Some(m);
        }
    }
    out
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Pair {
    pub a: String,
    pub b: String,
    /// Share of the smaller submission's fingerprints found in the other (0..1).
    pub similarity: f64,
    /// Jaccard index of the fingerprint sets (0..1).
    pub jaccard: f64,
    pub shared: usize,
    /// Matched line ranges (1-based, inclusive) in `a` and `b`.
    pub a_lines: Vec<(u32, u32)>,
    pub b_lines: Vec<(u32, u32)>,
}

pub struct Options {
    /// Tokens per k-gram.
    pub k: usize,
    /// Winnowing window.
    pub window: usize,
    /// Ignore fingerprints appearing in more than this share of submissions.
    pub common_ratio: f64,
    /// Report pairs with at least this similarity.
    pub threshold: f64,
    /// Ignore submissions with fewer fingerprints (tiny programs are
    /// necessarily alike).
    pub min_fingerprints: usize,
}

impl Default for Options {
    fn default() -> Self {
        Options { k: 15, window: 6, common_ratio: 0.3, threshold: 0.5, min_fingerprints: 12 }
    }
}

fn merge_ranges(mut v: Vec<(u32, u32)>) -> Vec<(u32, u32)> {
    v.sort();
    let mut out: Vec<(u32, u32)> = Vec::new();
    for (a, b) in v {
        match out.last_mut() {
            Some(l) if a <= l.1 + 1 => l.1 = l.1.max(b),
            _ => out.push((a, b)),
        }
    }
    out
}

/// Compare all submissions (name, source text) of one problem.
pub fn compare_all(subs: &[(String, String)], opt: &Options) -> Vec<Pair> {
    let prints: Vec<Vec<Print>> = subs.iter().map(|(_, s)| fingerprints(&tokenize(s), opt.k, opt.window)).collect();
    // Document frequency of each fingerprint.
    let mut df: HashMap<u64, usize> = HashMap::new();
    for p in &prints {
        let set: HashSet<u64> = p.iter().map(|x| x.h).collect();
        for h in set {
            *df.entry(h).or_default() += 1;
        }
    }
    let limit = ((subs.len() as f64) * opt.common_ratio).ceil().max(3.0) as usize;
    let sets: Vec<HashSet<u64>> = prints
        .iter()
        .map(|p| p.iter().map(|x| x.h).filter(|h| df.get(h).copied().unwrap_or(0) <= limit).collect())
        .collect();
    // Inverted index: only pairs sharing a fingerprint are compared.
    let mut index: HashMap<u64, Vec<usize>> = HashMap::new();
    for (i, s) in sets.iter().enumerate() {
        for &h in s {
            index.entry(h).or_default().push(i);
        }
    }
    let mut shared: HashMap<(usize, usize), usize> = HashMap::new();
    for docs in index.values() {
        for x in 0..docs.len() {
            for y in x + 1..docs.len() {
                *shared.entry((docs[x], docs[y])).or_default() += 1;
            }
        }
    }
    let mut out = Vec::new();
    for ((i, j), n) in shared {
        let (si, sj) = (sets[i].len(), sets[j].len());
        if si.min(sj) < opt.min_fingerprints {
            continue;
        }
        let smaller = si.min(sj).max(1);
        let similarity = n as f64 / smaller as f64;
        if similarity < opt.threshold {
            continue;
        }
        let jaccard = n as f64 / (si + sj - n).max(1) as f64;
        let common: HashSet<u64> = sets[i].intersection(&sets[j]).copied().collect();
        let lines = |p: &Vec<Print>| {
            merge_ranges(p.iter().filter(|x| common.contains(&x.h)).map(|x| (x.first_line, x.last_line)).collect())
        };
        out.push(Pair {
            a: subs[i].0.clone(),
            b: subs[j].0.clone(),
            similarity,
            jaccard,
            shared: n,
            a_lines: lines(&prints[i]),
            b_lines: lines(&prints[j]),
        });
    }
    out.sort_by(|x, y| y.similarity.partial_cmp(&x.similarity).unwrap().then_with(|| x.a.cmp(&y.a)));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const ORIGINAL: &str = r#"
#include <bits/stdc++.h>
using namespace std;
int n, a[100005];
long long solve() {
    long long best = 0, cur = 0;
    for (int i = 1; i <= n; i++) {
        cur = max(cur + a[i], (long long)a[i]);
        best = max(best, cur);
    }
    return best;
}
int main() {
    scanf("%d", &n);
    for (int i = 1; i <= n; i++) scanf("%d", &a[i]);
    printf("%lld\n", solve());
    return 0;
}
"#;

    // Same program: renamed identifiers, comments, different formatting.
    const DISGUISED: &str = r#"
#include <cstdio>
#include <algorithm>
using namespace std;
// my own solution!!
int N, arr[100005];
long long work()
{
    long long ans = 0, now = 0;   /* kadane */
    for (int j = 1; j <= N; j++)
    {
        now = max(now + arr[j], (long long)arr[j]);
        ans = max(ans, now);
    }
    return ans;
}
int main()
{
    scanf("%d", &N);
    for (int j = 1; j <= N; j++) scanf("%d", &arr[j]);
    printf("%lld\n", work());
    return 0;
}
"#;

    const DIFFERENT: &str = r#"
#include <iostream>
#include <vector>
int main() {
    std::ios::sync_with_stdio(false);
    int q; std::cin >> q;
    std::vector<long long> fib(90);
    fib[0] = 0; fib[1] = 1;
    for (size_t k = 2; k < fib.size(); ++k) fib[k] = fib[k - 1] + fib[k - 2];
    while (q--) { int x; std::cin >> x; std::cout << fib[x] << '\n'; }
}
"#;

    #[test]
    fn detects_renamed_copy() {
        let subs = vec![
            ("alice".to_string(), ORIGINAL.to_string()),
            ("bob".to_string(), DISGUISED.to_string()),
            ("carol".to_string(), DIFFERENT.to_string()),
        ];
        let opt = Options { threshold: 0.0, common_ratio: 1.0, min_fingerprints: 0, ..Default::default() };
        let pairs = compare_all(&subs, &opt);
        let ab = pairs.iter().find(|p| p.a == "alice" && p.b == "bob").expect("alice/bob pair");
        assert!(ab.similarity > 0.9, "{ab:?}");
        assert!(!ab.a_lines.is_empty() && !ab.b_lines.is_empty());
        for p in &pairs {
            if p.a == "carol" || p.b == "carol" {
                assert!(p.similarity < 0.3, "{p:?}");
            }
        }
    }

    #[test]
    fn tokenizer_skips_comments_and_preprocessor() {
        let t = tokenize("#include <x>\n// c\nint /* x */ a = 1;\n");
        assert_eq!(t.len(), 5); // int <id> = <num> ;
        assert_eq!(t[0].line, 3);
    }
}
