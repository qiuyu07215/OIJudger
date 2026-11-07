//! Contest timing: start, end, freeze, in milliseconds since the Unix epoch.

use chrono::{Local, NaiveDateTime, TimeZone};
use judge_core::model::OnlineConfig;
use serde::Serialize;

pub fn now_ms() -> i64 {
    chrono::Utc::now().timestamp_millis()
}

/// Parse `YYYY-MM-DD HH:MM[:SS]` (also with `T`) as local time.
pub fn parse_local(s: &str) -> Option<i64> {
    let s = s.trim().replace('T', " ");
    let dt = NaiveDateTime::parse_from_str(&s, "%Y-%m-%d %H:%M:%S")
        .or_else(|_| NaiveDateTime::parse_from_str(&s, "%Y-%m-%d %H:%M"))
        .ok()?;
    Local.from_local_datetime(&dt).earliest().map(|d| d.timestamp_millis())
}

pub fn format_local(ms: i64) -> String {
    Local.timestamp_millis_opt(ms).single().map(|d| d.format("%Y-%m-%d %H:%M:%S").to_string()).unwrap_or_default()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Phase {
    /// No start time set.
    NotScheduled,
    Before,
    Running,
    Ended,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct Clock {
    pub start_ms: Option<i64>,
    pub duration_ms: i64,
    pub freeze_ms: i64,
}

impl Clock {
    pub fn new(cfg: &OnlineConfig) -> Clock {
        Clock {
            start_ms: parse_local(&cfg.start),
            duration_ms: cfg.duration_min as i64 * 60_000,
            freeze_ms: cfg.freeze_min as i64 * 60_000,
        }
    }
    pub fn end_ms(&self) -> Option<i64> {
        self.start_ms.map(|s| s + self.duration_ms)
    }
    /// When the scoreboard freezes (`None`: never).
    pub fn freeze_at(&self) -> Option<i64> {
        if self.freeze_ms <= 0 {
            return None;
        }
        self.end_ms().map(|e| e - self.freeze_ms.min(self.duration_ms))
    }
    pub fn phase(&self, now: i64) -> Phase {
        match (self.start_ms, self.end_ms()) {
            (Some(s), Some(e)) => {
                if now < s {
                    Phase::Before
                } else if now < e {
                    Phase::Running
                } else {
                    Phase::Ended
                }
            }
            _ => Phase::NotScheduled,
        }
    }
    /// Contest time of `t` (ms since start, 0 before the start).
    pub fn contest_time(&self, t: i64) -> i64 {
        self.start_ms.map(|s| (t - s).max(0)).unwrap_or(0)
    }
    /// Submissions at or after this time are hidden while frozen.
    pub fn is_after_freeze(&self, t: i64) -> bool {
        self.freeze_at().is_some_and(|f| t >= f)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn phases_and_freeze() {
        let cfg = OnlineConfig { start: "2026-11-08 09:00".into(), duration_min: 300, freeze_min: 60, ..Default::default() };
        let c = Clock::new(&cfg);
        let s = c.start_ms.unwrap();
        assert_eq!(c.phase(s - 1), Phase::Before);
        assert_eq!(c.phase(s), Phase::Running);
        assert_eq!(c.phase(s + 300 * 60_000), Phase::Ended);
        assert_eq!(c.freeze_at(), Some(s + 240 * 60_000));
        assert!(c.is_after_freeze(s + 240 * 60_000));
        assert!(!c.is_after_freeze(s + 239 * 60_000));
        assert_eq!(parse_local("2026-11-08T09:00:30"), Some(s + 30_000));
        assert_eq!(Clock::new(&OnlineConfig::default()).phase(0), Phase::NotScheduled);
    }
}
