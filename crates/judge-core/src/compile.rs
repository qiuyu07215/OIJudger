//! Compiler discovery and sandboxed, cached compilation.

use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::workspace::{retry_io, Worker};

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
        hidden(Command::new(&self.path))
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

/// A command that never opens a console window (the GUI has no console).
fn hidden(mut c: Command) -> Command {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        c.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }
    c
}

fn includes_bits(p: &Path) -> bool {
    std::fs::read(p).map(|d| d.windows(14).any(|w| w == b"bits/stdc++.h>")).unwrap_or(false)
}

/// Root of precompiled headers, readable by build sandboxes.
pub fn pch_root() -> PathBuf {
    crate::workspace::work_base().parent().map(|p| p.join("pch")).unwrap_or_else(|| PathBuf::from("pch"))
}

/// Precompiled `<bits/stdc++.h>` for this compiler and its arguments: an
/// include directory containing `bits/stdc++.h.gch`, built once (outside the
/// sandbox: it only compiles the compiler's own header) and reused by all
/// later sessions. Parsing that header dominates the compile time of typical
/// OI code, so this makes compilation about three times faster.
pub fn ensure_pch(compiler: &CompilerConfig, version: &str) -> Option<PathBuf> {
    use std::collections::HashMap;
    use std::sync::{Mutex, OnceLock};
    static BUILT: OnceLock<Mutex<HashMap<String, Option<PathBuf>>>> = OnceLock::new();

    // Only flags that affect the preprocessor / code generation matter.
    let flags: Vec<String> = split_args(&compiler.args)
        .into_iter()
        .filter(|a| !a.starts_with("-l") && a != "-static" && a != "-s")
        .collect();
    let mut h = blake3::Hasher::new();
    h.update(compiler.path.to_string_lossy().as_bytes());
    h.update(version.as_bytes());
    h.update(flags.join("\0").as_bytes());
    let key = h.finalize().to_hex()[..20].to_string();

    let mut built = BUILT.get_or_init(Default::default).lock().unwrap_or_else(|e| e.into_inner());
    if let Some(r) = built.get(&key) {
        return r.clone();
    }
    let root = pch_root();
    if std::fs::create_dir_all(&root).is_err() || sandbox::Toolchain::grant_read(&root).is_err() {
        built.insert(key, None);
        return None;
    }
    let dir = root.join(&key);
    let gch = dir.join("bits").join("stdc++.h.gch");
    let result = if gch.is_file() {
        Some(dir)
    } else {
        let make = || -> Option<PathBuf> {
            std::fs::create_dir_all(gch.parent()?).ok()?;
            let wrapper = dir.join("wrapper.hpp");
            std::fs::write(&wrapper, "#include <bits/stdc++.h>\n").ok()?;
            let tmp = dir.join("bits").join("stdc++.h.gch.tmp");
            let ok = hidden(Command::new(&compiler.path))
                .args(&flags)
                .args(["-x", "c++-header"])
                .arg(&wrapper)
                .arg("-o")
                .arg(&tmp)
                .output()
                .ok()?
                .status
                .success();
            if !ok {
                return None;
            }
            std::fs::rename(&tmp, &gch).ok()?;
            Some(dir.clone())
        };
        let r = make();
        if r.is_none() {
            log::warn!("could not build precompiled header for {}", compiler.path.display());
        }
        r
    };
    built.insert(key, result.clone());
    result
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
    /// `lease` keeps `exe` from being evicted from the cache while it is in
    /// use; `fresh` is true when it was compiled now (not a cache hit).
    Ok { exe: PathBuf, log: String, lease: Lease, fresh: bool },
    Failed { log: String },
}

/// Cached programs in use. Eviction ([`evict`]) never deletes a leased file;
/// checking a cache entry and leasing it happen under the same lock.
#[derive(Default)]
struct LeaseTable {
    count: std::collections::HashMap<PathBuf, usize>,
    /// Programs whose last lease ended, oldest first: they were judged in
    /// this run and are the cheapest to evict.
    released: Vec<PathBuf>,
}

static LEASES: std::sync::LazyLock<parking_lot::Mutex<LeaseTable>> = std::sync::LazyLock::new(Default::default);

/// Keeps a cached program from being evicted while it is alive.
#[derive(Debug)]
pub struct Lease {
    path: PathBuf,
}

impl Lease {
    fn acquire(t: &mut LeaseTable, path: &Path) -> Lease {
        *t.count.entry(path.to_path_buf()).or_default() += 1;
        Lease { path: path.to_path_buf() }
    }
    /// Never release (checkers / interactors used for the whole session).
    pub fn keep_forever(self) {
        std::mem::forget(self);
    }
}

impl Clone for Lease {
    fn clone(&self) -> Lease {
        Lease::acquire(&mut LEASES.lock(), &self.path)
    }
}

impl Drop for Lease {
    fn drop(&mut self) {
        let mut t = LEASES.lock();
        if let Some(c) = t.count.get_mut(&self.path) {
            *c -= 1;
            if *c == 0 {
                t.count.remove(&self.path);
                t.released.push(self.path.clone());
            }
        }
    }
}

/// Forget which programs were released (start of a judging session).
pub fn reset_released() {
    LEASES.lock().released.clear();
}

/// Delete unleased programs (and their compile logs) from the cache until
/// at least `need` bytes are freed: programs judged in this session first,
/// then the least recently modified. Returns the bytes freed. Files that
/// cannot be deleted right now are skipped.
pub fn evict(cache_dir: &Path, need: u64) -> u64 {
    let mut t = LEASES.lock();
    let mut order: Vec<PathBuf> = std::mem::take(&mut t.released);
    let mut rest: Vec<(std::time::SystemTime, PathBuf)> = std::fs::read_dir(cache_dir)
        .map(|rd| {
            rd.flatten()
                .map(|e| e.path())
                .filter(|p| p.extension().is_some_and(|x| x.eq_ignore_ascii_case("exe")))
                .filter_map(|p| Some((std::fs::metadata(&p).ok()?.modified().ok()?, p)))
                .collect()
        })
        .unwrap_or_default();
    rest.sort();
    order.extend(rest.into_iter().map(|(_, p)| p));
    let mut seen = std::collections::HashSet::new();
    let mut freed = 0u64;
    for p in order {
        if freed >= need {
            break;
        }
        if !seen.insert(p.clone()) || t.count.contains_key(&p) {
            continue;
        }
        let Ok(m) = std::fs::metadata(&p) else { continue };
        if std::fs::remove_file(&p).is_ok() {
            freed += m.len();
            let log = p.with_extension("log");
            if let Ok(lm) = std::fs::metadata(&log) {
                if std::fs::remove_file(&log).is_ok() {
                    freed += lm.len();
                }
            }
        }
    }
    freed
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
        h.update(b"\0strip");
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

/// Exclusive right to build one cache key; released (waking waiters) on drop.
struct Claim(String);

type InFlight = (std::sync::Mutex<std::collections::HashSet<String>>, std::sync::Condvar);

fn in_flight() -> &'static InFlight {
    static S: std::sync::OnceLock<InFlight> = std::sync::OnceLock::new();
    S.get_or_init(Default::default)
}

impl Claim {
    /// `Some` if the caller should build `key`; `None` after waiting for
    /// another builder of the same key to finish (check the cache again).
    fn take(key: &str) -> Option<Claim> {
        let (m, cv) = in_flight();
        let mut set = m.lock().unwrap_or_else(|e| e.into_inner());
        if set.insert(key.to_string()) {
            return Some(Claim(key.to_string()));
        }
        while set.contains(key) {
            set = cv.wait(set).unwrap_or_else(|e| e.into_inner());
        }
        None
    }
}

impl Drop for Claim {
    fn drop(&mut self) {
        let (m, cv) = in_flight();
        m.lock().unwrap_or_else(|e| e.into_inner()).remove(&self.0);
        cv.notify_all();
    }
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
    let t_enter = std::time::Instant::now();
    let key = req.cache_key(compiler_version)?;
    let cached_exe = cache_dir.join(format!("{key}.exe"));
    let cached_log = cache_dir.join(format!("{key}.log"));
    let cached_fail = cache_dir.join(format!("{key}.fail"));
    // Several workers (the compile pipeline and the judging workers) may
    // ask for the same build; only one compiles, the others wait for it.
    let _claim = loop {
        let hit = {
            let mut t = LEASES.lock();
            cached_exe.is_file().then(|| Lease::acquire(&mut t, &cached_exe))
        };
        if let Some(lease) = hit {
            return Ok(BuildOutput::Ok { exe: cached_exe, log: read_log(&cached_log), lease, fresh: false });
        }
        if cached_fail.is_file() {
            return Ok(BuildOutput::Failed { log: read_log(&cached_fail) });
        }
        if let Some(c) = Claim::take(&key) {
            break c;
        }
    };

    let dir = worker.fresh_build_dir()?;
    let mut args: Vec<String> = Vec::new();
    if req.compiler.lang == Lang::Cpp && req.files.iter().any(|(p, _, main)| *main && includes_bits(p)) {
        if let Some(pch) = ensure_pch(&req.compiler, compiler_version) {
            // GCC finds bits/stdc++.h.gch here first; with mismatching flags
            // it silently falls back to the real header.
            args.push(format!("-I{}", pch.display()));
        }
    }
    args.extend(split_args(&req.compiler.args));
    args.extend(split_args(&req.extra_args));
    // Drop symbol tables: cached programs get ~60% smaller (a statically
    // linked program is 0.5-2.5 MB otherwise); running them is unaffected.
    args.push("-s".into());
    args.push("-o".into());
    args.push("__oij_out.exe".into());
    for (src, name, main) in &req.files {
        let dst = dir.join(name);
        if let Some(parent) = dst.parent() {
            std::fs::create_dir_all(parent)?;
        }
        retry_io(|| std::fs::copy(src, &dst)).with_context(|| format!("copying {}", src.display()))?;
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
    l.affinity = worker.affinity();
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
    let prep_ms = t_enter.elapsed().as_millis();
    let t0 = std::time::Instant::now();
    let r = worker.build_container().run(spec).context("starting compiler")?;
    if std::env::var_os("OIJ_DEBUG").is_some() {
        eprintln!(
            "[w{}] compile {:?}: {:?} in {} ms (cpu {} ms) prep {} ms",
            worker.index(),
            req.files[0].1,
            r.outcome,
            t0.elapsed().as_millis(),
            r.cpu_ms,
            prep_ms
        );
    }

    let mut text = read_log(&log_path);
    let out_exe = dir.join("__oij_out.exe");
    std::fs::create_dir_all(cache_dir)?;
    if r.outcome == sandbox::Outcome::Ok && out_exe.is_file() {
        std::fs::write(&cached_log, &text)?;
        // Move, not copy: every newly written executable is scanned again by
        // antivirus software (hundreds of milliseconds each, serialised
        // across all compilers); a moved file keeps its scan result.
        let tmp = cache_dir.join(format!("{key}.{}.tmp", worker.index()));
        if retry_io(|| std::fs::rename(&out_exe, &tmp)).is_ok() {
            // Drop the build sandbox's access inherited from the build folder.
            sandbox::reset_acl(&tmp).context("resetting permissions of compiled program")?;
        } else {
            // Different volume.
            retry_io(|| std::fs::copy(&out_exe, &tmp)).context("storing compiled program")?;
        }
        let lease = {
            let mut t = LEASES.lock();
            retry_io(|| std::fs::rename(&tmp, &cached_exe))
                .or_else(|e| if cached_exe.is_file() { Ok(()) } else { Err(e) })
                .context("storing compiled program")?;
            Lease::acquire(&mut t, &cached_exe)
        };
        if std::env::var_os("OIJ_DEBUG").is_some() {
            eprintln!("[w{}] stored {:?} total {} ms", worker.index(), req.files[0].1, t_enter.elapsed().as_millis());
        }
        Ok(BuildOutput::Ok { exe: cached_exe, log: text, lease, fresh: true })
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

/// Number of files and bytes in a contest's compile cache.
pub fn cache_usage(cache_dir: &Path) -> (usize, u64) {
    let Ok(rd) = std::fs::read_dir(cache_dir) else { return (0, 0) };
    rd.flatten()
        .filter_map(|e| e.metadata().ok().filter(|m| m.is_file()))
        .fold((0, 0), |(n, b), m| (n + 1, b + m.len()))
}

/// Delete a contest's compile cache. Results are kept; programs are compiled
/// again when they are next judged. Returns the files and bytes removed.
pub fn clear_cache(cache_dir: &Path) -> Result<(usize, u64)> {
    let Ok(rd) = std::fs::read_dir(cache_dir) else { return Ok((0, 0)) };
    let (mut n, mut bytes) = (0usize, 0u64);
    let mut failed = Vec::new();
    for e in rd.flatten() {
        let Ok(m) = e.metadata() else { continue };
        if !m.is_file() {
            continue;
        }
        // Right after judging, antivirus software or a program that is still
        // shutting down can hold an executable for a few seconds.
        let mut r = retry_io(|| std::fs::remove_file(e.path()));
        for _ in 0..6 {
            if r.is_ok() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(700));
            r = retry_io(|| std::fs::remove_file(e.path()));
        }
        match r {
            Ok(()) => {
                n += 1;
                bytes += m.len();
            }
            Err(err) => failed.push(format!("{}: {err}", e.path().display())),
        }
    }
    if !failed.is_empty() {
        anyhow::bail!("removed {n} file(s), but {} could not be deleted:\n{}", failed.len(), failed.join("\n"));
    }
    Ok((n, bytes))
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
