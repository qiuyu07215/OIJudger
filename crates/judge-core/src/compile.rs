//! Compiler discovery and sandboxed, cached compilation.

use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::workspace::Worker;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum Lang {
    C,
    Cpp,
}

impl Lang {
    pub fn from_path(p: &Path) -> Option<Lang> {
        let ext = p.extension()?.to_str()?.to_ascii_lowercase();
        match ext.as_str() {
            "c" => Some(Lang::C),
            "cpp" | "cc" | "cxx" | "c++" => Some(Lang::Cpp),
            _ => None,
        }
    }
    pub const SOURCE_EXTS: [&'static str; 4] = ["cpp", "cc", "cxx", "c"];
}

/// One configured compiler.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CompilerConfig {
    pub lang: Lang,
    /// Full path to g++.exe / gcc.exe.
    pub path: PathBuf,
    /// Arguments placed before `-o <exe> <sources>`.
    pub args: String,
}

impl CompilerConfig {
    pub fn default_args(lang: Lang) -> &'static str {
        match lang {
            Lang::Cpp => "-O2 -std=c++17 -static -DONLINE_JUDGE",
            Lang::C => "-O2 -std=c11 -static -DONLINE_JUDGE -lm",
        }
    }
    pub fn bin_dir(&self) -> PathBuf {
        self.path.parent().map(Path::to_path_buf).unwrap_or_default()
    }
    /// Directory that must be readable inside the sandbox (the toolchain
    /// root, i.e. the parent of `bin`).
    pub fn toolchain_root(&self) -> PathBuf {
        let bin = self.bin_dir();
        match bin.file_name().and_then(|n| n.to_str()) {
            Some(n) if n.eq_ignore_ascii_case("bin") => bin.parent().map(Path::to_path_buf).unwrap_or(bin),
            _ => bin,
        }
    }
    /// First line of `--version`, used in cache keys and shown in the UI.
    pub fn version(&self) -> String {
        Command::new(&self.path)
            .arg("--version")
            .output()
            .ok()
            .and_then(|o| String::from_utf8_lossy(&o.stdout).lines().next().map(str::to_string))
            .unwrap_or_default()
    }
}

/// Find MinGW style compilers in the usual places.
pub fn detect_compilers() -> Vec<CompilerConfig> {
    let mut dirs: Vec<PathBuf> = Vec::new();
    if let Ok(exe) = std::env::current_exe() {
        if let Some(d) = exe.parent() {
            // Portable bundle: <app>/mingw64/bin
            dirs.push(d.join("mingw64").join("bin"));
            dirs.push(d.join("compiler").join("bin"));
        }
    }
    if let Some(path) = std::env::var_os("PATH") {
        dirs.extend(std::env::split_paths(&path));
    }
    for d in [
        "C:\\mingw64\\bin",
        "C:\\msys64\\ucrt64\\bin",
        "C:\\msys64\\mingw64\\bin",
        "C:\\TDM-GCC-64\\bin",
        "C:\\Program Files\\Dev-Cpp\\MinGW64\\bin",
        "C:\\Program Files (x86)\\Dev-Cpp\\MinGW64\\bin",
        "C:\\MinGW\\bin",
        "C:\\winlibs\\mingw64\\bin",
    ] {
        dirs.push(PathBuf::from(d));
    }
    let mut out: Vec<CompilerConfig> = Vec::new();
    for d in dirs {
        for (lang, exe) in [(Lang::Cpp, "g++.exe"), (Lang::C, "gcc.exe")] {
            let p = d.join(exe);
            if p.is_file() && !out.iter().any(|c| c.lang == lang) {
                // Skip Git for Windows' MSYS shims etc.: they live in usr\bin
                if p.to_string_lossy().to_ascii_lowercase().contains("\\git\\") {
                    continue;
                }
                out.push(CompilerConfig { lang, path: p, args: CompilerConfig::default_args(lang).into() });
            }
        }
    }
    out
}

/// Split an argument string, honouring double quotes.
pub fn split_args(s: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut quoted = false;
    let mut any = false;
    for ch in s.chars() {
        match ch {
            '"' => {
                quoted = !quoted;
                any = true;
            }
            c if c.is_whitespace() && !quoted => {
                if any {
                    out.push(std::mem::take(&mut cur));
                    any = false;
                }
            }
            c => {
                cur.push(c);
                any = true;
            }
        }
    }
    if any {
        out.push(cur);
    }
    out
}

/// Inputs of one build.
#[derive(Debug, Clone)]
pub struct BuildRequest {
    /// (file on disk, name inside the build directory, pass on command line)
    pub files: Vec<(PathBuf, String, bool)>,
    pub compiler: CompilerConfig,
    pub extra_args: String,
}

#[derive(Debug, Clone)]
pub enum BuildOutput {
    Ok { exe: PathBuf, log: String },
    Failed { log: String },
}

impl BuildRequest {
    pub fn cache_key(&self, compiler_version: &str) -> Result<String> {
        let mut h = blake3::Hasher::new();
        h.update(self.compiler.path.to_string_lossy().as_bytes());
        h.update(b"\0");
        h.update(compiler_version.as_bytes());
        h.update(b"\0");
        h.update(self.compiler.args.as_bytes());
        h.update(b"\0");
        h.update(self.extra_args.as_bytes());
        for (path, name, main) in &self.files {
            h.update(b"\0file\0");
            h.update(name.as_bytes());
            h.update(&[*main as u8]);
            let data = std::fs::read(path).with_context(|| format!("reading {}", path.display()))?;
            h.update(&(data.len() as u64).to_le_bytes());
            h.update(&data);
        }
        Ok(h.finalize().to_hex()[..32].to_string())
    }
}

const LOG_LIMIT: usize = 16 * 1024;

fn read_log(path: &Path) -> String {
    let data = std::fs::read(path).unwrap_or_default();
    let mut s = String::from_utf8_lossy(&data[..data.len().min(LOG_LIMIT)]).into_owned();
    if data.len() > LOG_LIMIT {
        s.push_str("\n… (truncated)");
    }
    s
}

/// Settings for compilation in the sandbox.
#[derive(Debug, Clone)]
pub struct BuildLimits {
    pub cpu_ms: u64,
    pub memory_mb: u64,
}

impl Default for BuildLimits {
    fn default() -> Self {
        BuildLimits { cpu_ms: 20_000, memory_mb: 2048 }
    }
}

/// Compile in the worker's sandbox, using `cache_dir` to skip work.
/// Compilation errors are cached too, so re-judging never recompiles.
pub fn build(
    worker: &Worker,
    req: &BuildRequest,
    compiler_version: &str,
    cache_dir: &Path,
    limits: &BuildLimits,
) -> Result<BuildOutput> {
    let key = req.cache_key(compiler_version)?;
    let cached_exe = cache_dir.join(format!("{key}.exe"));
    let cached_log = cache_dir.join(format!("{key}.log"));
    let cached_fail = cache_dir.join(format!("{key}.fail"));
    if cached_exe.is_file() {
        return Ok(BuildOutput::Ok { exe: cached_exe, log: read_log(&cached_log) });
    }
    if cached_fail.is_file() {
        return Ok(BuildOutput::Failed { log: read_log(&cached_fail) });
    }

    let dir = worker.build_dir();
    worker.clean_dir(&dir, &[])?;
    let mut args: Vec<String> = split_args(&req.compiler.args);
    args.extend(split_args(&req.extra_args));
    args.push("-o".into());
    args.push("__oij_out.exe".into());
    for (src, name, main) in &req.files {
        let dst = dir.join(name);
        if let Some(parent) = dst.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::copy(src, &dst).with_context(|| format!("copying {}", src.display()))?;
        if *main {
            args.push(name.clone());
        }
    }
    // Libraries must come after sources for GNU ld.
    let (libs, rest): (Vec<String>, Vec<String>) = args.into_iter().partition(|a| a.starts_with("-l"));
    let mut args = rest;
    args.extend(libs);

    let log_path = dir.join("__oij_compile.log");
    let log = std::fs::File::create(&log_path)?;
    let log2 = log.try_clone()?;
    let mut l = sandbox::Limits::program(limits.cpu_ms, limits.memory_mb << 20);
    l.wall_ms = limits.cpu_ms * 2 + 5000;
    l.memory_per_job = true;
    l.max_processes = 16;
    l.output_bytes = 4 << 20;
    l.watch_files = vec![log_path.clone()];

    let sysroot = std::env::var("SystemRoot").unwrap_or_else(|_| "C:\\Windows".into());
    let mut spec = sandbox::RunSpec::new(&req.compiler.path, &dir, l)
        .env("PATH", format!("{};{sysroot}\\System32", req.compiler.bin_dir().display()));
    spec.args = args.into_iter().map(Into::into).collect();
    spec.stdout = sandbox::Stdio::File(log);
    spec.stderr = sandbox::Stdio::File(log2);
    // GCC keeps intermediate files in TEMP, which CreateProcess points at the
    // build container's profile folder; start from an empty one every time.
    worker.build_container().wipe_profile_temp();
    let t0 = std::time::Instant::now();
    let r = worker.build_container().run(spec).context("starting compiler")?;
    if std::env::var_os("OIJ_DEBUG").is_some() {
        eprintln!(
            "[w{}] compile {:?}: {:?} in {} ms (cpu {} ms)",
            worker.index(),
            req.files[0].1,
            r.outcome,
            t0.elapsed().as_millis(),
            r.cpu_ms
        );
    }

    let mut text = read_log(&log_path);
    let out_exe = dir.join("__oij_out.exe");
    std::fs::create_dir_all(cache_dir)?;
    if r.outcome == sandbox::Outcome::Ok && out_exe.is_file() {
        std::fs::write(&cached_log, &text)?;
        let tmp = cache_dir.join(format!("{key}.{}.tmp", worker.index()));
        std::fs::copy(&out_exe, &tmp)?;
        std::fs::rename(&tmp, &cached_exe).or_else(|e| if cached_exe.is_file() { Ok(()) } else { Err(e) })?;
        Ok(BuildOutput::Ok { exe: cached_exe, log: text })
    } else {
        match r.outcome {
            sandbox::Outcome::TimeLimit | sandbox::Outcome::IdleLimit => {
                text.push_str("\nCompilation time limit exceeded")
            }
            sandbox::Outcome::MemoryLimit => text.push_str("\nCompilation memory limit exceeded"),
            sandbox::Outcome::OutputLimit => text.push_str("\nCompiler output too long"),
            _ => {}
        }
        if text.trim().is_empty() {
            text = format!("Compiler failed: {}", r.describe());
        }
        std::fs::write(&cached_fail, &text)?;
        Ok(BuildOutput::Failed { log: text })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn split() {
        assert_eq!(split_args("-O2  -std=c++17 -DX=\"a b\""), vec!["-O2", "-std=c++17", "-DX=a b"]);
        assert!(split_args("   ").is_empty());
    }
}
