//! Machine-wide application settings (`%APPDATA%\OIJudger\settings.toml`),
//! shared by the GUI and the command line.

use std::path::PathBuf;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::checker::CheckerLimits;
use crate::compile::{detect_compilers, BuildLimits, CompilerConfig};
use crate::judge::Settings;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct AppSettings {
    /// Configured compilers; empty = detect automatically.
    pub compilers: Vec<CompilerConfig>,
    /// Parallel workers; 0 = automatic.
    pub workers: usize,
    pub pin_cores: bool,
    pub patch_stack: bool,
    pub output_limit_mb: u64,
    pub compile_time_ms: u64,
    pub compile_memory_mb: u64,
    pub checker_time_ms: u64,
    pub checker_memory_mb: u64,
    /// UI language: "zh-CN" or "en".
    pub language: String,
    /// Recently opened contests (most recent first).
    pub recent: Vec<PathBuf>,
}

impl Default for AppSettings {
    fn default() -> Self {
        let s = Settings::default();
        AppSettings {
            compilers: Vec::new(),
            workers: 0,
            pin_cores: s.pin_cores,
            patch_stack: s.patch_stack,
            output_limit_mb: s.output_limit_mb,
            compile_time_ms: s.build.cpu_ms,
            compile_memory_mb: s.build.memory_mb,
            checker_time_ms: s.checker.cpu_ms,
            checker_memory_mb: s.checker.memory_mb,
            language: "zh-CN".into(),
            recent: Vec::new(),
        }
    }
}

impl AppSettings {
    pub fn path() -> PathBuf {
        let base = std::env::var_os("APPDATA").map(PathBuf::from).unwrap_or_else(std::env::temp_dir);
        base.join("OIJudger").join("settings.toml")
    }

    /// Load settings, falling back to defaults when missing or unreadable.
    pub fn load() -> AppSettings {
        std::fs::read_to_string(Self::path()).ok().and_then(|t| toml::from_str(&t).ok()).unwrap_or_default()
    }

    pub fn save(&self) -> Result<()> {
        let p = Self::path();
        if let Some(d) = p.parent() {
            std::fs::create_dir_all(d)?;
        }
        std::fs::write(&p, toml::to_string_pretty(self)?).with_context(|| format!("writing {}", p.display()))
    }

    /// Compilers to use: configured ones that still exist, otherwise detected.
    pub fn effective_compilers(&self) -> Vec<CompilerConfig> {
        let configured: Vec<CompilerConfig> = self.compilers.iter().filter(|c| c.path.is_file()).cloned().collect();
        if configured.is_empty() {
            detect_compilers()
        } else {
            configured
        }
    }

    pub fn judge_settings(&self) -> Settings {
        Settings {
            workers: self.workers,
            pin_cores: self.pin_cores,
            patch_stack: self.patch_stack,
            output_limit_mb: self.output_limit_mb.max(1),
            build: BuildLimits { cpu_ms: self.compile_time_ms.max(1000), memory_mb: self.compile_memory_mb.max(256) },
            checker: CheckerLimits {
                cpu_ms: self.checker_time_ms.max(1000),
                memory_mb: self.checker_memory_mb.max(256),
            },
        }
    }

    pub fn add_recent(&mut self, dir: PathBuf) {
        self.recent.retain(|d| d != &dir);
        self.recent.insert(0, dir);
        self.recent.truncate(10);
    }
}
