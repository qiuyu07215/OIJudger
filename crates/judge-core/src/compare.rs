//! Built-in output comparators. Files are memory mapped, so comparing large
//! outputs costs no extra copies.

use std::fs::File;
use std::io;
use std::path::Path;

use crate::model::Comparator;

#[derive(Debug, Clone, PartialEq)]
pub struct Compared {
    pub ok: bool,
    pub message: String,
}

impl Compared {
    fn ok() -> Self {
        Compared { ok: true, message: String::new() }
    }
    fn wa(message: String) -> Self {
        Compared { ok: false, message }
    }
}

enum Data {
    Map(memmap2::Mmap),
    Empty,
}

impl std::ops::Deref for Data {
    type Target = [u8];
    fn deref(&self) -> &[u8] {
        match self {
            Data::Map(m) => m,
            Data::Empty => &[],
        }
    }
}

fn load(path: &Path) -> io::Result<Data> {
    let f = File::open(path)?;
    if f.metadata()?.len() == 0 {
        return Ok(Data::Empty);
    }
    // Safety: the files are not modified while we compare (the contestant
    // process has exited and the data files are only read).
    Ok(Data::Map(unsafe { memmap2::Mmap::map(&f)? }))
}

pub fn compare_files(kind: Comparator, eps: f64, output: &Path, answer: &Path) -> io::Result<Compared> {
    let out = load(output)?;
    let ans = load(answer)?;
    Ok(compare_bytes(kind, eps, &out, &ans))
}

pub fn compare_bytes(kind: Comparator, eps: f64, out: &[u8], ans: &[u8]) -> Compared {
    match kind {
        Comparator::Lines => lines(out, ans, true),
        Comparator::Strict => lines(out, ans, false),
        Comparator::Tokens => tokens(out, ans, None),
        Comparator::Real => tokens(out, ans, Some(eps)),
        Comparator::Binary => {
            if out == ans {
                Compared::ok()
            } else {
                let pos = out.iter().zip(ans).position(|(a, b)| a != b).unwrap_or(out.len().min(ans.len()));
                Compared::wa(format!("Files differ at byte {pos}"))
            }
        }
    }
}

fn show(b: &[u8]) -> String {
    const MAX: usize = 64;
    let s = String::from_utf8_lossy(&b[..b.len().min(MAX)]).into_owned();
    if b.len() > MAX {
        format!("{s}…")
    } else {
        s
    }
}

struct Lines<'a> {
    rest: &'a [u8],
    done: bool,
}

impl<'a> Iterator for Lines<'a> {
    type Item = &'a [u8];
    fn next(&mut self) -> Option<&'a [u8]> {
        if self.done {
            return None;
        }
        match self.rest.iter().position(|&c| c == b'\n') {
            Some(i) => {
                let line = &self.rest[..i];
                self.rest = &self.rest[i + 1..];
                Some(line)
            }
            None => {
                self.done = true;
                Some(self.rest)
            }
        }
    }
}

fn trim_end(mut s: &[u8], all_ws: bool) -> &[u8] {
    while let Some(&c) = s.last() {
        if c == b'\r' || (all_ws && (c == b' ' || c == b'\t' || c == 0x0b || c == 0x0c)) {
            s = &s[..s.len() - 1];
        } else {
            break;
        }
    }
    s
}

fn lines(out: &[u8], ans: &[u8], ignore_ws: bool) -> Compared {
    let mut a = Lines { rest: out, done: false };
    let mut b = Lines { rest: ans, done: false };
    let mut n = 0usize;
    loop {
        n += 1;
        match (a.next(), b.next()) {
            (None, None) => return Compared::ok(),
            (Some(x), Some(y)) => {
                let (x, y) = (trim_end(x, ignore_ws), trim_end(y, ignore_ws));
                if x != y {
                    return Compared::wa(format!("Line {n}: expected `{}`, found `{}`", show(y), show(x)));
                }
            }
            (Some(x), None) => {
                if !trim_end(x, true).is_empty() || a.any(|l| !trim_end(l, true).is_empty()) {
                    return Compared::wa(format!("Line {n}: expected end of file, found `{}`", show(x)));
                }
                return Compared::ok();
            }
            (None, Some(y)) => {
                if !trim_end(y, true).is_empty() || b.any(|l| !trim_end(l, true).is_empty()) {
                    return Compared::wa(format!("Line {n}: expected `{}`, found end of file", show(y)));
                }
                return Compared::ok();
            }
        }
    }
}

fn token_iter(s: &[u8]) -> impl Iterator<Item = &[u8]> {
    s.split(|c| c.is_ascii_whitespace()).filter(|t| !t.is_empty())
}

fn parse_f64(t: &[u8]) -> Option<f64> {
    std::str::from_utf8(t).ok()?.parse::<f64>().ok()
}

pub fn real_equal(x: f64, y: f64, eps: f64) -> bool {
    if x.is_nan() || y.is_nan() {
        return x.is_nan() && y.is_nan();
    }
    if x.is_infinite() || y.is_infinite() {
        return x == y;
    }
    let d = (x - y).abs();
    d <= eps || d <= eps * y.abs()
}

fn tokens(out: &[u8], ans: &[u8], eps: Option<f64>) -> Compared {
    let mut a = token_iter(out);
    let mut b = token_iter(ans);
    let mut n = 0usize;
    loop {
        n += 1;
        match (a.next(), b.next()) {
            (None, None) => return Compared::ok(),
            (Some(x), None) => return Compared::wa(format!("Token {n}: expected end of file, found `{}`", show(x))),
            (None, Some(y)) => return Compared::wa(format!("Token {n}: expected `{}`, found end of file", show(y))),
            (Some(x), Some(y)) => {
                if x == y {
                    continue;
                }
                if let Some(eps) = eps {
                    if let (Some(fx), Some(fy)) = (parse_f64(x), parse_f64(y)) {
                        if real_equal(fx, fy, eps) {
                            continue;
                        }
                        return Compared::wa(format!(
                            "Token {n}: expected {}, found {} (error {:.3e})",
                            show(y),
                            show(x),
                            (fx - fy).abs()
                        ));
                    }
                }
                return Compared::wa(format!("Token {n}: expected `{}`, found `{}`", show(y), show(x)));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ok(k: Comparator, a: &str, b: &str) -> bool {
        compare_bytes(k, 1e-6, a.as_bytes(), b.as_bytes()).ok
    }

    #[test]
    fn lines_default() {
        assert!(ok(Comparator::Lines, "1 2\n3\n", "1 2\n3\n"));
        assert!(ok(Comparator::Lines, "1 2  \r\n3", "1 2\n3\n\n\n"));
        assert!(ok(Comparator::Lines, "", ""));
        assert!(ok(Comparator::Lines, "\n\n", ""));
        assert!(!ok(Comparator::Lines, "1  2\n", "1 2\n"));
        assert!(!ok(Comparator::Lines, "1\n2\n", "1\n"));
        assert!(!ok(Comparator::Lines, "1\n", "1\n2\n"));
        let r = compare_bytes(Comparator::Lines, 0.0, b"1\n5\n", b"1\n4\n");
        assert_eq!(r.message, "Line 2: expected `4`, found `5`");
    }

    #[test]
    fn strict_keeps_spaces() {
        assert!(ok(Comparator::Strict, "a\r\nb\n", "a\nb"));
        assert!(!ok(Comparator::Strict, "a \n", "a\n"));
    }

    #[test]
    fn token_mode() {
        assert!(ok(Comparator::Tokens, "1\n2   3", " 1 2\n3\n"));
        assert!(!ok(Comparator::Tokens, "1 2", "1 2 3"));
    }

    #[test]
    fn real_mode() {
        assert!(ok(Comparator::Real, "0.3333333", "0.333333333"));
        assert!(ok(Comparator::Real, "1000000.5", "1000000.0"));
        assert!(!ok(Comparator::Real, "0.33", "0.333333333"));
        assert!(ok(Comparator::Real, "nan inf -inf", "NaN inf -inf"));
        assert!(!ok(Comparator::Real, "inf", "-inf"));
        assert!(ok(Comparator::Real, "YES 1.0", "YES 1"));
        assert!(!ok(Comparator::Real, "NO", "YES"));
    }

    #[test]
    fn binary_mode() {
        assert!(ok(Comparator::Binary, "ab", "ab"));
        assert!(!ok(Comparator::Binary, "ab\n", "ab"));
    }
}
