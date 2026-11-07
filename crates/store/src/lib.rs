//! Result storage: one row per (contestant, problem) in `.oij/results.db`.
//! Summary columns make the score table cheap to load; per-case details are
//! kept as JSON and loaded on demand.

use std::path::Path;

use anyhow::{Context, Result};
use judge_core::{ProblemResult, Status};
use rusqlite::{params, Connection, OptionalExtension};

pub struct Store {
    conn: Connection,
}

/// Row without per-case details.
#[derive(Debug, Clone, PartialEq)]
pub struct Summary {
    pub contestant: String,
    pub problem: String,
    pub status: Status,
    pub label: String,
    pub score: f64,
    pub full: f64,
    pub time_ms: u64,
    pub source_hash: String,
    pub problem_hash: String,
    pub judged_at: i64,
}

fn status_str(s: Status) -> &'static str {
    match s {
        Status::Judged => "judged",
        Status::NoSource => "no_source",
        Status::CompileError => "compile_error",
        Status::SystemError => "system_error",
    }
}

fn parse_status(s: &str) -> Status {
    match s {
        "no_source" => Status::NoSource,
        "compile_error" => Status::CompileError,
        "system_error" => Status::SystemError,
        _ => Status::Judged,
    }
}

fn save_on(conn: &Connection, r: &ProblemResult) -> Result<()> {
    conn.prepare_cached(
        "INSERT OR REPLACE INTO results
         (contestant, problem, status, label, score, full, time_ms, source_hash, problem_hash, judged_at, detail)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
    )?
    .execute(params![
        r.contestant,
        r.problem,
        status_str(r.status),
        r.label(),
        r.score,
        r.full,
        r.time_ms as i64,
        r.source_hash,
        r.problem_hash,
        r.judged_at,
        serde_json::to_string(r)?,
    ])?;
    Ok(())
}

impl Store {
    pub fn open(path: &Path) -> Result<Store> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let conn = Connection::open(path).with_context(|| format!("opening {}", path.display()))?;
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "synchronous", "NORMAL")?;
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS results (
                contestant   TEXT NOT NULL,
                problem      TEXT NOT NULL,
                status       TEXT NOT NULL,
                label        TEXT NOT NULL,
                score        REAL NOT NULL,
                full         REAL NOT NULL,
                time_ms      INTEGER NOT NULL,
                source_hash  TEXT NOT NULL,
                problem_hash TEXT NOT NULL,
                judged_at    INTEGER NOT NULL,
                detail       TEXT NOT NULL,
                PRIMARY KEY (contestant, problem)
            );",
        )?;
        Ok(Store { conn })
    }

    pub fn open_in_memory() -> Result<Store> {
        let s = Store { conn: Connection::open_in_memory()? };
        s.conn.execute_batch(
            "CREATE TABLE results (contestant TEXT NOT NULL, problem TEXT NOT NULL, status TEXT NOT NULL,
             label TEXT NOT NULL, score REAL NOT NULL, full REAL NOT NULL, time_ms INTEGER NOT NULL,
             source_hash TEXT NOT NULL, problem_hash TEXT NOT NULL, judged_at INTEGER NOT NULL,
             detail TEXT NOT NULL, PRIMARY KEY (contestant, problem));",
        )?;
        Ok(s)
    }

    pub fn save(&self, r: &ProblemResult) -> Result<()> {
        save_on(&self.conn, r)
    }

    /// Save many results in one transaction.
    pub fn save_many(&mut self, rs: &[ProblemResult]) -> Result<()> {
        let tx = self.conn.transaction()?;
        for r in rs {
            save_on(&tx, r)?;
        }
        tx.commit()?;
        Ok(())
    }

    pub fn get(&self, contestant: &str, problem: &str) -> Result<Option<ProblemResult>> {
        let detail: Option<String> = self
            .conn
            .query_row(
                "SELECT detail FROM results WHERE contestant = ?1 AND problem = ?2",
                params![contestant, problem],
                |r| r.get(0),
            )
            .optional()?;
        Ok(match detail {
            Some(d) => Some(serde_json::from_str(&d)?),
            None => None,
        })
    }

    pub fn summaries(&self) -> Result<Vec<Summary>> {
        let mut st = self.conn.prepare(
            "SELECT contestant, problem, status, label, score, full, time_ms, source_hash, problem_hash, judged_at
             FROM results",
        )?;
        let rows = st.query_map([], |r| {
            Ok(Summary {
                contestant: r.get(0)?,
                problem: r.get(1)?,
                status: parse_status(&r.get::<_, String>(2)?),
                label: r.get(3)?,
                score: r.get(4)?,
                full: r.get(5)?,
                time_ms: r.get::<_, i64>(6)? as u64,
                source_hash: r.get(7)?,
                problem_hash: r.get(8)?,
                judged_at: r.get(9)?,
            })
        })?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    pub fn load_all(&self) -> Result<Vec<ProblemResult>> {
        let mut st = self.conn.prepare("SELECT detail FROM results")?;
        let rows = st.query_map([], |r| r.get::<_, String>(0))?;
        let mut out = Vec::new();
        for d in rows {
            out.push(serde_json::from_str(&d?)?);
        }
        Ok(out)
    }

    pub fn delete(&self, contestant: &str, problem: &str) -> Result<()> {
        self.conn
            .execute("DELETE FROM results WHERE contestant = ?1 AND problem = ?2", params![contestant, problem])?;
        Ok(())
    }

    /// Drop results of contestants / problems that no longer exist.
    pub fn retain(&self, contestants: &[String], problems: &[String]) -> Result<usize> {
        let mut n = 0;
        for s in self.summaries()? {
            if !contestants.contains(&s.contestant) || !problems.contains(&s.problem) {
                self.delete(&s.contestant, &s.problem)?;
                n += 1;
            }
        }
        Ok(n)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip() {
        let tmp = tempfile::tempdir().unwrap();
        let s = Store::open(&tmp.path().join("r.db")).unwrap();
        let mut r = ProblemResult::empty("alice", "sum", 100.0, Status::CompileError, "error: x");
        r.source_hash = "abc".into();
        s.save(&r).unwrap();
        r.score = 0.0;
        s.save(&r).unwrap();
        let all = s.summaries().unwrap();
        assert_eq!(all.len(), 1);
        assert_eq!(all[0].label, "CE");
        assert_eq!(s.get("alice", "sum").unwrap().unwrap(), r);
        assert_eq!(s.retain(&["bob".into()], &["sum".into()]).unwrap(), 1);
        assert!(s.summaries().unwrap().is_empty());
    }
}
