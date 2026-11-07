//! `oijudger selftest`: verify on this machine that the sandbox isolates and
//! limits programs as promised. Every probe is compiled with the configured
//! compiler inside the compile sandbox and run inside the run sandbox.

use std::fs::File;
use std::io::Read;
use std::path::Path;

use anyhow::{Context, Result};
use judge_core::compile::{build, BuildLimits, BuildOutput, BuildRequest, Lang};
use judge_core::workspace::{Worker, Workspace};
use judge_core::AppSettings;
use sandbox::{Limits, Outcome, RunResult, RunSpec, Stdio};

const MIB: u64 = 1 << 20;

macro_rules! probe_src {
    ($name:literal) => {
        ($name, include_str!(concat!("../../../tests/sandbox-escape/", $name, ".cpp")))
    };
}

const SOURCES: &[(&str, &str)] = &[
    probe_src!("aplusb"),
    probe_src!("net_connect"),
    probe_src!("read_file"),
    probe_src!("write_file"),
    probe_src!("spawn_child"),
    probe_src!("clipboard"),
    probe_src!("print_env"),
    probe_src!("tle_loop"),
    probe_src!("sleep_forever"),
    probe_src!("mle_heap"),
    probe_src!("mle_static"),
    probe_src!("deep_recursion"),
    probe_src!("output_flood"),
    probe_src!("div_zero"),
];

pub struct ProbeReport {
    pub name: &'static str,
    pub passed: bool,
    pub detail: String,
}

fn source(name: &str) -> &'static str {
    SOURCES.iter().find(|(n, _)| *n == name).map(|(_, s)| *s).expect("known probe")
}

struct Ctx<'a> {
    worker: &'a Worker,
    other: &'a Worker,
    cache: &'a Path,
    compiler: judge_core::CompilerConfig,
    version: String,
    secret_dir: &'a Path,
}

impl Ctx<'_> {
    fn compile(&self, name: &str) -> Result<std::result::Result<std::path::PathBuf, String>> {
        let src = self.secret_dir.join(format!("{name}.cpp"));
        std::fs::write(&src, source(name))?;
        let req = BuildRequest {
            files: vec![(src, format!("{name}.cpp"), true)],
            compiler: self.compiler.clone(),
            extra_args: "-lws2_32 -luser32".into(),
        };
        Ok(match build(self.worker, &req, &self.version, self.cache, &BuildLimits::default())? {
            BuildOutput::Ok { exe, .. } => Ok(exe),
            BuildOutput::Failed { log } => Err(log),
        })
    }

    fn run(&self, name: &str, args: &[String], limits: Limits, patch_stack: bool) -> Result<(RunResult, String)> {
        let exe = self.compile(name)?.map_err(|log| anyhow::anyhow!("compile {name} failed:\n{log}"))?;
        let dir = self.worker.run_dir();
        self.worker.clean_dir(&dir, &[])?;
        let prog = dir.join("probe.exe");
        std::fs::copy(&exe, &prog)?;
        if patch_stack {
            sandbox::set_stack_reserve(&prog, limits.memory_bytes)?;
        }
        let out = dir.join("__stdout");
        let mut spec = RunSpec::new(&prog, &dir, limits);
        spec.args = args.iter().map(Into::into).collect();
        spec.stdout = Stdio::File(File::create(&out)?);
        let r = self.worker.container().run(spec)?;
        let mut s = String::new();
        File::open(&out)?.read_to_string(&mut s)?;
        Ok((r, s.trim().to_string()))
    }
}

/// Run all probes, reporting each one through `report` as it finishes.
pub fn run(settings: &AppSettings, report: &mut dyn FnMut(&ProbeReport)) -> Result<bool> {
    sandbox::init_process();
    let compiler = settings
        .effective_compilers()
        .into_iter()
        .find(|c| c.lang == Lang::Cpp)
        .context("No C++ compiler found; configure one first")?;
    sandbox::Toolchain::grant_read(&compiler.toolchain_root())?;
    let version = compiler.version();
    let ws = Workspace::new(2, false)?;
    let secret_dir = ws.workers[0].root().parent().unwrap().join("selftest");
    std::fs::create_dir_all(&secret_dir)?;
    let cache = secret_dir.join("cache");
    let ctx = Ctx {
        worker: &ws.workers[0],
        other: &ws.workers[1],
        cache: &cache,
        compiler,
        version,
        secret_dir: &secret_dir,
    };

    let mut all = true;
    let mut check = |name: &'static str, f: &dyn Fn() -> Result<(bool, String)>| {
        let (passed, detail) = match f() {
            Ok(v) => v,
            Err(e) => (false, format!("{e:#}")),
        };
        all &= passed;
        report(&ProbeReport { name, passed, detail });
    };
    let lim = || Limits::program(2000, 256 * MIB);

    check("normal program runs", &|| {
        let input = ctx.secret_dir.join("in.txt");
        std::fs::write(&input, "20 22")?;
        let exe = ctx.compile("aplusb")?.map_err(anyhow::Error::msg)?;
        let dir = ctx.worker.run_dir();
        ctx.worker.clean_dir(&dir, &[])?;
        std::fs::copy(&exe, dir.join("p.exe"))?;
        let out = dir.join("out.txt");
        let mut spec = RunSpec::new(dir.join("p.exe"), &dir, lim());
        spec.stdin = Stdio::File(File::open(&input)?);
        spec.stdout = Stdio::File(File::create(&out)?);
        let r = ctx.worker.container().run(spec)?;
        let s = std::fs::read_to_string(&out)?;
        Ok((r.outcome == Outcome::Ok && s.trim() == "42", format!("{:?}, {} ms", r.outcome, r.cpu_ms)))
    });
    check("network is blocked (loopback, internet, DNS)", &|| {
        let listener = std::net::TcpListener::bind("127.0.0.1:0")?;
        let port = listener.local_addr()?.port().to_string();
        let (_, out) = ctx.run("net_connect", &[port], Limits::program(10_000, 256 * MIB), false)?;
        Ok((out.ends_with("BLOCKED"), out.replace('\n', "; ")))
    });
    check("files outside the sandbox cannot be read", &|| {
        let secret = ctx.secret_dir.join("answer.ans");
        std::fs::write(&secret, "secret")?;
        let (_, out) = ctx.run("read_file", &[secret.display().to_string()], lim(), false)?;
        Ok((out == "BLOCKED", out))
    });
    check("other workers' files cannot be read", &|| {
        let f = ctx.other.run_dir().join("output.txt");
        std::fs::write(&f, "other contestant")?;
        let (_, out) = ctx.run("read_file", &[f.display().to_string()], lim(), false)?;
        Ok((out == "BLOCKED", out))
    });
    check("files outside the sandbox cannot be written", &|| {
        let target = ctx.secret_dir.join("pwned.txt");
        let (_, out) = ctx.run("write_file", &[target.display().to_string()], lim(), false)?;
        Ok((out == "BLOCKED" && !target.exists(), out))
    });
    check("container profile folder is locked", &|| {
        let (_, out) = ctx.run("print_env", &[], lim(), false)?;
        let probes: Vec<&str> = out.lines().filter(|l| l.starts_with("write ")).collect();
        Ok((!probes.is_empty() && probes.iter().all(|l| l.ends_with("BLOCKED")), probes.join("; ")))
    });
    check("child processes cannot be created", &|| {
        let (_, out) = ctx.run("spawn_child", &[], lim(), false)?;
        Ok((out == "BLOCKED", out))
    });
    check("clipboard cannot be read", &|| {
        let (_, out) = ctx.run("clipboard", &[], lim(), false)?;
        Ok((out == "BLOCKED", out))
    });
    check("busy loop is stopped (TLE)", &|| {
        let (r, _) = ctx.run("tle_loop", &[], Limits::program(1000, 256 * MIB), false)?;
        Ok((r.outcome == Outcome::TimeLimit, format!("{:?} after {} ms wall", r.outcome, r.wall_ms)))
    });
    check("sleeping program is stopped (wall clock)", &|| {
        let (r, _) = ctx.run("sleep_forever", &[], Limits::program(500, 256 * MIB), false)?;
        Ok((r.outcome == Outcome::IdleLimit, format!("{:?} after {} ms wall", r.outcome, r.wall_ms)))
    });
    check("growing heap is stopped (MLE)", &|| {
        let (r, _) = ctx.run("mle_heap", &[], Limits::program(5000, 128 * MIB), false)?;
        Ok((r.outcome == Outcome::MemoryLimit, format!("{:?}, peak {} MiB", r.outcome, r.peak_memory_bytes >> 20)))
    });
    check("oversized static array is MLE", &|| {
        let (r, _) = ctx.run("mle_static", &[], Limits::program(5000, 256 * MIB), false)?;
        Ok((r.outcome == Outcome::MemoryLimit, format!("{:?}", r.outcome)))
    });
    check("deep recursion works with stack = memory limit", &|| {
        let (r, _) = ctx.run("deep_recursion", &[], Limits::program(5000, 512 * MIB), true)?;
        Ok((r.outcome == Outcome::Ok, format!("{:?}, peak {} MiB", r.outcome, r.peak_memory_bytes >> 20)))
    });
    check("output flood is stopped (OLE)", &|| {
        let mut l = Limits::program(5000, 256 * MIB);
        l.output_bytes = 64 * MIB;
        l.watch_files = vec![ctx.worker.run_dir().join("__stdout")];
        let (r, _) = ctx.run("output_flood", &[], l, false)?;
        Ok((r.outcome == Outcome::OutputLimit, format!("{:?}", r.outcome)))
    });
    check("runtime errors are reported", &|| {
        let (r, _) = ctx.run("div_zero", &[], lim(), false)?;
        Ok((r.outcome == Outcome::Crashed, r.describe()))
    });
    check("compiler cannot #include files outside the sandbox", &|| {
        let secret = ctx.secret_dir.join("secret_data.h");
        std::fs::write(&secret, "int leaked = 42;\n")?;
        let src = ctx.secret_dir.join("inc.cpp");
        std::fs::write(
            &src,
            format!(
                "#include \"{}\"\nint main() {{ return leaked; }}\n",
                secret.display().to_string().replace('\\', "/")
            ),
        )?;
        let req = BuildRequest {
            files: vec![(src, "inc.cpp".into(), true)],
            compiler: ctx.compiler.clone(),
            extra_args: String::new(),
        };
        let r = build(ctx.worker, &req, &ctx.version, ctx.cache, &BuildLimits::default())?;
        Ok(match r {
            BuildOutput::Failed { log } => {
                (true, log.lines().find(|l| l.contains("error")).unwrap_or("").trim().to_string())
            }
            BuildOutput::Ok { .. } => (false, "compiled successfully — the compiler could read the file".into()),
        })
    });

    drop(ws);
    let _ = std::fs::remove_dir_all(&secret_dir);
    Ok(all)
}
