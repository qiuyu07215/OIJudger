//! `.oij/online.db`: accounts, sessions, submissions, judgings,
//! clarifications, announcements, balloons, print jobs and an audit log.

use std::path::Path;

use anyhow::{Context, Result};
use parking_lot::Mutex;
use rusqlite::{params, Connection, OptionalExtension, Row};
use serde::{Deserialize, Serialize};

use crate::scoring::Outcome;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    Contestant,
    /// Judges: see everything, answer clarifications, rejudge.
    Jury,
    /// Volunteers: balloons and printing.
    Balloon,
    Admin,
}

impl Role {
    pub fn as_str(self) -> &'static str {
        match self {
            Role::Contestant => "contestant",
            Role::Jury => "jury",
            Role::Balloon => "balloon",
            Role::Admin => "admin",
        }
    }
    pub fn parse(s: &str) -> Role {
        match s {
            "jury" => Role::Jury,
            "balloon" => Role::Balloon,
            "admin" => Role::Admin,
            _ => Role::Contestant,
        }
    }
    /// Jury or admin: may see all submissions and the unfrozen scoreboard.
    pub fn is_jury(self) -> bool {
        matches!(self, Role::Jury | Role::Admin)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Account {
    pub id: i64,
    /// Login name; also the contestant's folder in `source/`.
    pub username: String,
    pub display_name: String,
    /// School / team affiliation, used for grouping.
    pub group_name: String,
    /// Seat or room location (for balloons and printing).
    pub seat: String,
    pub role: Role,
    /// If set, logins are only accepted from this IP address.
    pub ip_lock: String,
    pub disabled: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum JudgingState {
    Queued,
    Judging,
    Done,
    Cancelled,
}

impl JudgingState {
    fn as_str(self) -> &'static str {
        match self {
            JudgingState::Queued => "queued",
            JudgingState::Judging => "judging",
            JudgingState::Done => "done",
            JudgingState::Cancelled => "cancelled",
        }
    }
    fn parse(s: &str) -> JudgingState {
        match s {
            "judging" => JudgingState::Judging,
            "done" => JudgingState::Done,
            "cancelled" => JudgingState::Cancelled,
            _ => JudgingState::Queued,
        }
    }
}

fn outcome_str(o: Outcome) -> &'static str {
    match o {
        Outcome::Pending => "pending",
        Outcome::Accepted => "accepted",
        Outcome::Rejected => "rejected",
        Outcome::CompileError => "compile_error",
        Outcome::Error => "error",
    }
}

fn parse_outcome(s: &str) -> Outcome {
    match s {
        "accepted" => Outcome::Accepted,
        "rejected" => Outcome::Rejected,
        "compile_error" => Outcome::CompileError,
        "error" => Outcome::Error,
        _ => Outcome::Pending,
    }
}

/// A submission with its current (valid) judging.
#[derive(Debug, Clone, Serialize)]
pub struct Submission {
    pub id: i64,
    pub account_id: i64,
    pub problem: String,
    pub lang: String,
    /// File name as stored (e.g. `sum.cpp`).
    pub filename: String,
    pub size: i64,
    pub at_ms: i64,
    pub ip: String,
    /// Ignored by the jury: as if never submitted (not on the scoreboard).
    pub ignored: bool,
    pub judging_id: Option<i64>,
    pub state: JudgingState,
    pub outcome: Outcome,
    /// Short verdict label ("AC", "WA", "CE", ...).
    pub label: String,
    pub score: f64,
    pub full: f64,
    pub subtask_scores: Vec<f64>,
    pub time_ms: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct Clarification {
    pub id: i64,
    /// Contestant who asked; `None` for a message sent by the jury.
    pub from_account: Option<i64>,
    /// Recipient of a jury message; `None` = everyone (when public).
    pub to_account: Option<i64>,
    pub problem: String,
    pub question: String,
    pub answer: String,
    /// Answer visible to all contestants.
    pub public: bool,
    pub at_ms: i64,
    pub answered_ms: Option<i64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Announcement {
    pub id: i64,
    pub title: String,
    pub body: String,
    pub at_ms: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct Balloon {
    pub id: i64,
    pub account_id: i64,
    pub problem: String,
    pub submission_id: i64,
    pub at_ms: i64,
    /// First team to solve this problem.
    pub first: bool,
    pub done: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct PrintJob {
    pub id: i64,
    pub account_id: i64,
    pub filename: String,
    pub content: String,
    pub at_ms: i64,
    pub done: bool,
}

pub struct Db {
    conn: Mutex<Connection>,
}

const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS accounts (
    id INTEGER PRIMARY KEY,
    username TEXT NOT NULL UNIQUE COLLATE NOCASE,
    display_name TEXT NOT NULL DEFAULT '',
    group_name TEXT NOT NULL DEFAULT '',
    seat TEXT NOT NULL DEFAULT '',
    role TEXT NOT NULL DEFAULT 'contestant',
    password_hash TEXT NOT NULL,
    ip_lock TEXT NOT NULL DEFAULT '',
    disabled INTEGER NOT NULL DEFAULT 0
);
CREATE TABLE IF NOT EXISTS sessions (
    token_hash TEXT PRIMARY KEY,
    account_id INTEGER NOT NULL,
    created_ms INTEGER NOT NULL,
    seen_ms INTEGER NOT NULL,
    ip TEXT NOT NULL DEFAULT ''
);
CREATE TABLE IF NOT EXISTS submissions (
    id INTEGER PRIMARY KEY,
    account_id INTEGER NOT NULL,
    problem TEXT NOT NULL,
    lang TEXT NOT NULL,
    filename TEXT NOT NULL,
    size INTEGER NOT NULL,
    at_ms INTEGER NOT NULL,
    ip TEXT NOT NULL DEFAULT ''
);
CREATE INDEX IF NOT EXISTS submissions_account ON submissions(account_id);
CREATE TABLE IF NOT EXISTS judgings (
    id INTEGER PRIMARY KEY,
    submission_id INTEGER NOT NULL,
    valid INTEGER NOT NULL DEFAULT 1,
    state TEXT NOT NULL,
    outcome TEXT NOT NULL DEFAULT 'pending',
    label TEXT NOT NULL DEFAULT '',
    score REAL NOT NULL DEFAULT 0,
    full REAL NOT NULL DEFAULT 0,
    subtask_scores TEXT NOT NULL DEFAULT '[]',
    time_ms INTEGER NOT NULL DEFAULT 0,
    detail TEXT NOT NULL DEFAULT '',
    started_ms INTEGER,
    finished_ms INTEGER
);
CREATE INDEX IF NOT EXISTS judgings_submission ON judgings(submission_id);
CREATE TABLE IF NOT EXISTS clarifications (
    id INTEGER PRIMARY KEY,
    from_account INTEGER,
    to_account INTEGER,
    problem TEXT NOT NULL DEFAULT '',
    question TEXT NOT NULL DEFAULT '',
    answer TEXT NOT NULL DEFAULT '',
    public INTEGER NOT NULL DEFAULT 0,
    at_ms INTEGER NOT NULL,
    answered_ms INTEGER
);
CREATE TABLE IF NOT EXISTS announcements (
    id INTEGER PRIMARY KEY,
    title TEXT NOT NULL,
    body TEXT NOT NULL DEFAULT '',
    at_ms INTEGER NOT NULL
);
CREATE TABLE IF NOT EXISTS balloons (
    id INTEGER PRIMARY KEY,
    account_id INTEGER NOT NULL,
    problem TEXT NOT NULL,
    submission_id INTEGER NOT NULL,
    at_ms INTEGER NOT NULL,
    first INTEGER NOT NULL DEFAULT 0,
    done INTEGER NOT NULL DEFAULT 0,
    UNIQUE (account_id, problem)
);
CREATE TABLE IF NOT EXISTS print_jobs (
    id INTEGER PRIMARY KEY,
    account_id INTEGER NOT NULL,
    filename TEXT NOT NULL,
    content TEXT NOT NULL,
    at_ms INTEGER NOT NULL,
    done INTEGER NOT NULL DEFAULT 0
);
CREATE TABLE IF NOT EXISTS state (key TEXT PRIMARY KEY, value TEXT NOT NULL);
CREATE TABLE IF NOT EXISTS events (
    id INTEGER PRIMARY KEY,
    at_ms INTEGER NOT NULL,
    actor TEXT NOT NULL DEFAULT '',
    kind TEXT NOT NULL,
    detail TEXT NOT NULL DEFAULT ''
);
";

const ACCOUNT_COLS: &str = "id, username, display_name, group_name, seat, role, ip_lock, disabled";

/// Columns added after the first release of the online schema.
fn migrate(conn: &Connection) -> Result<()> {
    let has = |table: &str, col: &str| -> Result<bool> {
        let mut st = conn.prepare(&format!("PRAGMA table_info({table})"))?;
        let cols = st.query_map([], |r| r.get::<_, String>(1))?.collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(cols.iter().any(|c| c == col))
    };
    if !has("submissions", "ignored")? {
        conn.execute_batch("ALTER TABLE submissions ADD COLUMN ignored INTEGER NOT NULL DEFAULT 0")?;
    }
    Ok(())
}

fn account_row(r: &Row) -> rusqlite::Result<Account> {
    Ok(Account {
        id: r.get(0)?,
        username: r.get(1)?,
        display_name: r.get(2)?,
        group_name: r.get(3)?,
        seat: r.get(4)?,
        role: Role::parse(&r.get::<_, String>(5)?),
        ip_lock: r.get(6)?,
        disabled: r.get::<_, i64>(7)? != 0,
    })
}

const SUBMISSION_SELECT: &str = "SELECT s.id, s.account_id, s.problem, s.lang, s.filename, s.size, s.at_ms, s.ip,
    j.id, j.state, j.outcome, j.label, j.score, j.full, j.subtask_scores, j.time_ms, s.ignored
    FROM submissions s LEFT JOIN judgings j ON j.submission_id = s.id AND j.valid = 1";

fn submission_row(r: &Row) -> rusqlite::Result<Submission> {
    let jid: Option<i64> = r.get(8)?;
    Ok(Submission {
        id: r.get(0)?,
        account_id: r.get(1)?,
        problem: r.get(2)?,
        lang: r.get(3)?,
        filename: r.get(4)?,
        size: r.get(5)?,
        at_ms: r.get(6)?,
        ip: r.get(7)?,
        ignored: r.get::<_, i64>(16)? != 0,
        judging_id: jid,
        state: r.get::<_, Option<String>>(9)?.map(|s| JudgingState::parse(&s)).unwrap_or(JudgingState::Queued),
        outcome: r.get::<_, Option<String>>(10)?.map(|s| parse_outcome(&s)).unwrap_or(Outcome::Pending),
        label: r.get::<_, Option<String>>(11)?.unwrap_or_default(),
        score: r.get::<_, Option<f64>>(12)?.unwrap_or(0.0),
        full: r.get::<_, Option<f64>>(13)?.unwrap_or(0.0),
        subtask_scores: r
            .get::<_, Option<String>>(14)?
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default(),
        time_ms: r.get::<_, Option<i64>>(15)?.unwrap_or(0),
    })
}

fn clar_row(r: &Row) -> rusqlite::Result<Clarification> {
    Ok(Clarification {
        id: r.get(0)?,
        from_account: r.get(1)?,
        to_account: r.get(2)?,
        problem: r.get(3)?,
        question: r.get(4)?,
        answer: r.get(5)?,
        public: r.get::<_, i64>(6)? != 0,
        at_ms: r.get(7)?,
        answered_ms: r.get(8)?,
    })
}

impl Db {
    pub fn open(path: &Path) -> Result<Db> {
        if let Some(d) = path.parent() {
            std::fs::create_dir_all(d)?;
        }
        let conn = Connection::open(path).with_context(|| format!("opening {}", path.display()))?;
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "synchronous", "NORMAL")?;
        conn.busy_timeout(std::time::Duration::from_secs(5))?;
        conn.execute_batch(SCHEMA)?;
        migrate(&conn)?;
        Ok(Db { conn: Mutex::new(conn) })
    }

    pub fn open_memory() -> Result<Db> {
        let conn = Connection::open_in_memory()?;
        conn.execute_batch(SCHEMA)?;
        migrate(&conn)?;
        Ok(Db { conn: Mutex::new(conn) })
    }

    pub fn set_ignored(&self, submission_id: i64, ignored: bool) -> Result<()> {
        self.conn
            .lock()
            .execute("UPDATE submissions SET ignored = ?2 WHERE id = ?1", params![submission_id, ignored as i64])?;
        Ok(())
    }

    // ---- accounts -------------------------------------------------------

    pub fn create_account(&self, a: &Account, password_hash: &str) -> Result<i64> {
        let c = self.conn.lock();
        c.execute(
            "INSERT INTO accounts (username, display_name, group_name, seat, role, password_hash, ip_lock, disabled)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                a.username,
                a.display_name,
                a.group_name,
                a.seat,
                a.role.as_str(),
                password_hash,
                a.ip_lock,
                a.disabled as i64
            ],
        )
        .with_context(|| format!("creating account {}", a.username))?;
        Ok(c.last_insert_rowid())
    }

    pub fn update_account(&self, a: &Account) -> Result<()> {
        self.conn.lock().execute(
            "UPDATE accounts SET username = ?2, display_name = ?3, group_name = ?4, seat = ?5, role = ?6,
             ip_lock = ?7, disabled = ?8 WHERE id = ?1",
            params![a.id, a.username, a.display_name, a.group_name, a.seat, a.role.as_str(), a.ip_lock, a.disabled as i64],
        )?;
        Ok(())
    }

    pub fn set_password_hash(&self, id: i64, hash: &str) -> Result<()> {
        self.conn.lock().execute("UPDATE accounts SET password_hash = ?2 WHERE id = ?1", params![id, hash])?;
        Ok(())
    }

    pub fn delete_account(&self, id: i64) -> Result<()> {
        let c = self.conn.lock();
        c.execute("DELETE FROM sessions WHERE account_id = ?1", [id])?;
        c.execute("DELETE FROM accounts WHERE id = ?1", [id])?;
        Ok(())
    }

    pub fn accounts(&self) -> Result<Vec<Account>> {
        let c = self.conn.lock();
        let mut st = c.prepare(&format!("SELECT {ACCOUNT_COLS} FROM accounts ORDER BY id"))?;
        let v = st.query_map([], account_row)?.collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(v)
    }

    pub fn account(&self, id: i64) -> Result<Option<Account>> {
        let c = self.conn.lock();
        Ok(c.query_row(&format!("SELECT {ACCOUNT_COLS} FROM accounts WHERE id = ?1"), [id], account_row).optional()?)
    }

    /// Account and password hash by user name (case-insensitive).
    pub fn account_for_login(&self, username: &str) -> Result<Option<(Account, String)>> {
        let c = self.conn.lock();
        Ok(c.query_row(
            &format!("SELECT {ACCOUNT_COLS}, password_hash FROM accounts WHERE username = ?1"),
            [username],
            |r| Ok((account_row(r)?, r.get::<_, String>(8)?)),
        )
        .optional()?)
    }

    // ---- sessions -------------------------------------------------------

    pub fn create_session(&self, token_hash: &str, account_id: i64, now: i64, ip: &str) -> Result<()> {
        self.conn.lock().execute(
            "INSERT INTO sessions (token_hash, account_id, created_ms, seen_ms, ip) VALUES (?1, ?2, ?3, ?3, ?4)",
            params![token_hash, account_id, now, ip],
        )?;
        Ok(())
    }

    /// Account of a session; refreshes its last-seen time.
    pub fn session_account(&self, token_hash: &str, now: i64) -> Result<Option<Account>> {
        let c = self.conn.lock();
        let id: Option<i64> = c
            .query_row("SELECT account_id FROM sessions WHERE token_hash = ?1", [token_hash], |r| r.get(0))
            .optional()?;
        let Some(id) = id else { return Ok(None) };
        c.execute("UPDATE sessions SET seen_ms = ?2 WHERE token_hash = ?1", params![token_hash, now])?;
        Ok(c.query_row(&format!("SELECT {ACCOUNT_COLS} FROM accounts WHERE id = ?1"), [id], account_row).optional()?)
    }

    pub fn delete_session(&self, token_hash: &str) -> Result<()> {
        self.conn.lock().execute("DELETE FROM sessions WHERE token_hash = ?1", [token_hash])?;
        Ok(())
    }

    pub fn delete_sessions_of(&self, account_id: i64) -> Result<()> {
        self.conn.lock().execute("DELETE FROM sessions WHERE account_id = ?1", [account_id])?;
        Ok(())
    }

    /// Accounts seen in the last `window_ms`.
    pub fn online_count(&self, now: i64, window_ms: i64) -> Result<i64> {
        Ok(self.conn.lock().query_row(
            "SELECT COUNT(DISTINCT account_id) FROM sessions WHERE seen_ms >= ?1",
            [now - window_ms],
            |r| r.get(0),
        )?)
    }

    // ---- submissions and judgings ----------------------------------------

    /// Insert a submission and a queued judging; returns (submission, judging).
    pub fn add_submission(
        &self,
        account_id: i64,
        problem: &str,
        lang: &str,
        filename: &str,
        size: i64,
        at_ms: i64,
        ip: &str,
    ) -> Result<(i64, i64)> {
        let mut c = self.conn.lock();
        let tx = c.transaction()?;
        tx.execute(
            "INSERT INTO submissions (account_id, problem, lang, filename, size, at_ms, ip) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![account_id, problem, lang, filename, size, at_ms, ip],
        )?;
        let sid = tx.last_insert_rowid();
        tx.execute("INSERT INTO judgings (submission_id, state) VALUES (?1, 'queued')", [sid])?;
        let jid = tx.last_insert_rowid();
        tx.commit()?;
        Ok((sid, jid))
    }

    /// New judging for a submission (rejudge); it replaces the previous one.
    pub fn new_judging(&self, submission_id: i64) -> Result<i64> {
        let mut c = self.conn.lock();
        let tx = c.transaction()?;
        tx.execute("UPDATE judgings SET valid = 0 WHERE submission_id = ?1", [submission_id])?;
        tx.execute("INSERT INTO judgings (submission_id, state) VALUES (?1, 'queued')", [submission_id])?;
        let jid = tx.last_insert_rowid();
        tx.commit()?;
        Ok(jid)
    }

    pub fn judging_started(&self, judging_id: i64, now: i64) -> Result<()> {
        self.conn.lock().execute(
            "UPDATE judgings SET state = 'judging', started_ms = ?2 WHERE id = ?1",
            params![judging_id, now],
        )?;
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    pub fn judging_done(
        &self,
        judging_id: i64,
        state: JudgingState,
        outcome: Outcome,
        label: &str,
        score: f64,
        full: f64,
        subtask_scores: &[f64],
        time_ms: i64,
        detail: &str,
        now: i64,
    ) -> Result<()> {
        self.conn.lock().execute(
            "UPDATE judgings SET state = ?2, outcome = ?3, label = ?4, score = ?5, full = ?6, subtask_scores = ?7,
             time_ms = ?8, detail = ?9, finished_ms = ?10 WHERE id = ?1",
            params![
                judging_id,
                state.as_str(),
                outcome_str(outcome),
                label,
                score,
                full,
                serde_json::to_string(subtask_scores)?,
                time_ms,
                detail,
                now
            ],
        )?;
        Ok(())
    }

    /// Submission id of a judging, if that judging is still the valid one.
    pub fn valid_judging_submission(&self, judging_id: i64) -> Result<Option<i64>> {
        Ok(self
            .conn
            .lock()
            .query_row("SELECT submission_id FROM judgings WHERE id = ?1 AND valid = 1", [judging_id], |r| r.get(0))
            .optional()?)
    }

    pub fn judging_detail(&self, judging_id: i64) -> Result<String> {
        Ok(self
            .conn
            .lock()
            .query_row("SELECT detail FROM judgings WHERE id = ?1", [judging_id], |r| r.get(0))
            .optional()?
            .unwrap_or_default())
    }

    pub fn submission(&self, id: i64) -> Result<Option<Submission>> {
        let c = self.conn.lock();
        Ok(c.query_row(&format!("{SUBMISSION_SELECT} WHERE s.id = ?1"), [id], submission_row).optional()?)
    }

    /// All submissions (oldest first), or one account's.
    pub fn submissions(&self, account: Option<i64>) -> Result<Vec<Submission>> {
        let c = self.conn.lock();
        let v = match account {
            Some(a) => {
                let mut st = c.prepare_cached(&format!("{SUBMISSION_SELECT} WHERE s.account_id = ?1 ORDER BY s.id"))?;
                let v = st.query_map([a], submission_row)?.collect::<rusqlite::Result<Vec<_>>>()?;
                v
            }
            None => {
                let mut st = c.prepare_cached(&format!("{SUBMISSION_SELECT} ORDER BY s.id"))?;
                let v = st.query_map([], submission_row)?.collect::<rusqlite::Result<Vec<_>>>()?;
                v
            }
        };
        Ok(v)
    }

    /// Submissions whose valid judging is not finished (to requeue on start).
    pub fn unfinished(&self) -> Result<Vec<(i64, i64)>> {
        let c = self.conn.lock();
        let mut st = c.prepare("SELECT submission_id, id FROM judgings WHERE valid = 1 AND state IN ('queued', 'judging') ORDER BY id")?;
        let v = st.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?.collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(v)
    }

    pub fn last_submission_ms(&self, account_id: i64) -> Result<Option<i64>> {
        Ok(self
            .conn
            .lock()
            .query_row("SELECT MAX(at_ms) FROM submissions WHERE account_id = ?1", [account_id], |r| r.get(0))
            .optional()?
            .flatten())
    }

    // ---- clarifications and announcements --------------------------------

    pub fn add_clarification(&self, from: Option<i64>, to: Option<i64>, problem: &str, question: &str, now: i64) -> Result<i64> {
        let c = self.conn.lock();
        c.execute(
            "INSERT INTO clarifications (from_account, to_account, problem, question, at_ms) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![from, to, problem, question, now],
        )?;
        Ok(c.last_insert_rowid())
    }

    pub fn answer_clarification(&self, id: i64, answer: &str, public: bool, now: i64) -> Result<()> {
        self.conn.lock().execute(
            "UPDATE clarifications SET answer = ?2, public = ?3, answered_ms = ?4 WHERE id = ?1",
            params![id, answer, public as i64, now],
        )?;
        Ok(())
    }

    /// All clarifications (jury), or those a contestant may see.
    pub fn clarifications(&self, viewer: Option<i64>) -> Result<Vec<Clarification>> {
        let c = self.conn.lock();
        let base = "SELECT id, from_account, to_account, problem, question, answer, public, at_ms, answered_ms FROM clarifications";
        let v = match viewer {
            None => {
                let mut st = c.prepare(&format!("{base} ORDER BY id"))?;
                let v = st.query_map([], clar_row)?.collect::<rusqlite::Result<Vec<_>>>()?;
                v
            }
            Some(a) => {
                let mut st = c.prepare(&format!(
                    "{base} WHERE from_account = ?1 OR to_account = ?1 OR (public = 1 AND answered_ms IS NOT NULL)
                     OR (from_account IS NULL AND to_account IS NULL) ORDER BY id"
                ))?;
                let v = st.query_map([a], clar_row)?.collect::<rusqlite::Result<Vec<_>>>()?;
                v
            }
        };
        Ok(v)
    }

    pub fn add_announcement(&self, title: &str, body: &str, now: i64) -> Result<i64> {
        let c = self.conn.lock();
        c.execute("INSERT INTO announcements (title, body, at_ms) VALUES (?1, ?2, ?3)", params![title, body, now])?;
        Ok(c.last_insert_rowid())
    }

    pub fn delete_announcement(&self, id: i64) -> Result<()> {
        self.conn.lock().execute("DELETE FROM announcements WHERE id = ?1", [id])?;
        Ok(())
    }

    pub fn announcements(&self) -> Result<Vec<Announcement>> {
        let c = self.conn.lock();
        let mut st = c.prepare("SELECT id, title, body, at_ms FROM announcements ORDER BY id")?;
        let v = st
            .query_map([], |r| Ok(Announcement { id: r.get(0)?, title: r.get(1)?, body: r.get(2)?, at_ms: r.get(3)? }))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(v)
    }

    // ---- balloons and printing --------------------------------------------

    /// Record a balloon unless the team already has one for the problem.
    /// Returns its id when created.
    pub fn add_balloon(&self, account_id: i64, problem: &str, submission_id: i64, now: i64, first: bool) -> Result<Option<i64>> {
        let c = self.conn.lock();
        let n = c.execute(
            "INSERT OR IGNORE INTO balloons (account_id, problem, submission_id, at_ms, first) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![account_id, problem, submission_id, now, first as i64],
        )?;
        Ok((n > 0).then(|| c.last_insert_rowid()))
    }

    pub fn balloons(&self) -> Result<Vec<Balloon>> {
        let c = self.conn.lock();
        let mut st = c.prepare("SELECT id, account_id, problem, submission_id, at_ms, first, done FROM balloons ORDER BY id")?;
        let v = st
            .query_map([], |r| {
                Ok(Balloon {
                    id: r.get(0)?,
                    account_id: r.get(1)?,
                    problem: r.get(2)?,
                    submission_id: r.get(3)?,
                    at_ms: r.get(4)?,
                    first: r.get::<_, i64>(5)? != 0,
                    done: r.get::<_, i64>(6)? != 0,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(v)
    }

    pub fn set_balloon_done(&self, id: i64, done: bool) -> Result<()> {
        self.conn.lock().execute("UPDATE balloons SET done = ?2 WHERE id = ?1", params![id, done as i64])?;
        Ok(())
    }

    pub fn add_print_job(&self, account_id: i64, filename: &str, content: &str, now: i64) -> Result<i64> {
        let c = self.conn.lock();
        c.execute(
            "INSERT INTO print_jobs (account_id, filename, content, at_ms) VALUES (?1, ?2, ?3, ?4)",
            params![account_id, filename, content, now],
        )?;
        Ok(c.last_insert_rowid())
    }

    pub fn print_jobs(&self, account: Option<i64>) -> Result<Vec<PrintJob>> {
        let c = self.conn.lock();
        let row = |r: &Row| {
            Ok(PrintJob {
                id: r.get(0)?,
                account_id: r.get(1)?,
                filename: r.get(2)?,
                content: r.get(3)?,
                at_ms: r.get(4)?,
                done: r.get::<_, i64>(5)? != 0,
            })
        };
        let base = "SELECT id, account_id, filename, content, at_ms, done FROM print_jobs";
        let v = match account {
            Some(a) => {
                let mut st = c.prepare(&format!("{base} WHERE account_id = ?1 ORDER BY id"))?;
                let v = st.query_map([a], row)?.collect::<rusqlite::Result<Vec<_>>>()?;
                v
            }
            None => {
                let mut st = c.prepare(&format!("{base} ORDER BY id"))?;
                let v = st.query_map([], row)?.collect::<rusqlite::Result<Vec<_>>>()?;
                v
            }
        };
        Ok(v)
    }

    pub fn set_print_done(&self, id: i64, done: bool) -> Result<()> {
        self.conn.lock().execute("UPDATE print_jobs SET done = ?2 WHERE id = ?1", params![id, done as i64])?;
        Ok(())
    }

    // ---- state and audit log ------------------------------------------------

    pub fn get_state(&self, key: &str) -> Result<Option<String>> {
        Ok(self.conn.lock().query_row("SELECT value FROM state WHERE key = ?1", [key], |r| r.get(0)).optional()?)
    }

    pub fn set_state(&self, key: &str, value: &str) -> Result<()> {
        self.conn
            .lock()
            .execute("INSERT OR REPLACE INTO state (key, value) VALUES (?1, ?2)", params![key, value])?;
        Ok(())
    }

    pub fn log(&self, now: i64, actor: &str, kind: &str, detail: &str) {
        let _ = self.conn.lock().execute(
            "INSERT INTO events (at_ms, actor, kind, detail) VALUES (?1, ?2, ?3, ?4)",
            params![now, actor, kind, detail],
        );
    }
}
