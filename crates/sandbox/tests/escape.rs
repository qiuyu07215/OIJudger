//! Sandbox escape and resource limit tests.
//!
//! Compiles the programs in `tests/sandbox-escape/` with g++ (from PATH or
//! `OIJ_GXX`) and runs them inside the sandbox.
#![cfg(windows)]

use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::OnceLock;

use sandbox::{Container, Limits, Outcome, RunResult, RunSpec, Stdio};

const MIB: u64 = 1 << 20;

fn src_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/sandbox-escape")
}

fn build_dir() -> &'static Path {
    static DIR: OnceLock<PathBuf> = OnceLock::new();
    DIR.get_or_init(|| {
        sandbox::init_process();
        let d = Path::new(env!("CARGO_TARGET_TMPDIR")).join("escape-bin");
        std::fs::create_dir_all(&d).unwrap();
        d
    })
}

fn compile(name: &str) -> PathBuf {
    let exe = build_dir().join(format!("{name}.exe"));
    let src = src_dir().join(format!("{name}.cpp"));
    let fresh = exe.metadata().and_then(|e| Ok(e.modified()? >= src.metadata()?.modified()?)).unwrap_or(false);
    if !fresh {
        let gxx = std::env::var("OIJ_GXX").unwrap_or_else(|_| "g++".into());
        let st = Command::new(gxx)
            .args(["-O2", "-std=c++17", "-static", "-o"])
            .arg(&exe)
            .arg(&src)
            .args(["-lws2_32", "-luser32"])
            .status()
            .expect("g++ not found (set OIJ_GXX)");
        assert!(st.success(), "compile {name} failed");
    }
    exe
}

struct Env {
    _tmp: tempfile::TempDir,
    work: PathBuf,
    exe: PathBuf,
}

fn container(i: usize) -> Container {
    Container::open(&format!("OIJudger.Test.W{i}")).expect("AppContainer")
}

fn setup(c: &Container, name: &str) -> Env {
    let tmp = tempfile::tempdir().unwrap();
    let work = tmp.path().join("work");
    std::fs::create_dir(&work).unwrap();
    c.grant_dir(&work).unwrap();
    let exe = work.join("prog.exe");
    std::fs::copy(compile(name), &exe).unwrap();
    Env { _tmp: tmp, work, exe }
}

fn run(c: &Container, env: &Env, args: &[String], limits: Limits) -> (RunResult, String) {
    let out = env.work.join("stdout.txt");
    let mut spec = RunSpec::new(&env.exe, &env.work, limits);
    spec.args = args.iter().map(Into::into).collect();
    spec.stdout = Stdio::File(File::create(&out).unwrap());
    let r = c.run(spec).expect("spawn");
    let mut s = String::new();
    File::open(&out).unwrap().read_to_string(&mut s).unwrap();
    (r, s.trim().to_string())
}

fn limits() -> Limits {
    Limits::program(2000, 256 * MIB)
}

#[test]
fn aplusb_runs() {
    let c = container(0);
    let env = setup(&c, "aplusb");
    let input = env.work.join("../in.txt");
    std::fs::write(&input, "1 2\n").unwrap();
    let out = env.work.join("stdout.txt");
    let mut spec = RunSpec::new(&env.exe, &env.work, limits());
    // stdin comes from a file the container itself cannot open
    spec.stdin = Stdio::File(File::open(&input).unwrap());
    spec.stdout = Stdio::File(File::create(&out).unwrap());
    let r = c.run(spec).unwrap();
    assert_eq!(r.outcome, Outcome::Ok, "{r:?}");
    assert_eq!(std::fs::read_to_string(&out).unwrap().trim(), "3");
    assert!(r.peak_memory_bytes > 0);
}

#[test]
fn file_io_in_workdir() {
    let c = container(0);
    let env = setup(&c, "fileio");
    std::fs::write(env.work.join("sum.in"), "40 2\n").unwrap();
    let (r, _) = run(&c, &env, &[], limits());
    assert_eq!(r.outcome, Outcome::Ok, "{r:?}");
    assert_eq!(std::fs::read_to_string(env.work.join("sum.out")).unwrap().trim(), "42");
}

#[test]
fn network_is_blocked() {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port().to_string();
    let c = container(0);
    let env = setup(&c, "net_connect");
    let (r, out) = run(&c, &env, &[port.clone()], Limits::program(10_000, 256 * MIB));
    assert_eq!(r.outcome, Outcome::Ok, "{r:?}");
    assert!(out.ends_with("BLOCKED"), "{out}");

    // Sanity check: without isolation the loopback connection succeeds, so
    // the test above really measures the sandbox.
    let open = Container::unisolated();
    let (_, out) = run(&open, &env, &[port], Limits::program(10_000, 256 * MIB));
    assert!(out.contains("loopback LEAK"), "{out}");
}

#[test]
fn cannot_read_outside() {
    let c = container(0);
    let env = setup(&c, "read_file");
    let secret = env.work.parent().unwrap().join("answer.ans");
    std::fs::write(&secret, "secret answer").unwrap();
    let (r, out) = run(&c, &env, &[secret.display().to_string()], limits());
    assert_eq!(r.outcome, Outcome::Ok, "{r:?}");
    assert_eq!(out, "BLOCKED");

    let home = std::env::var("USERPROFILE").unwrap();
    let user_file = Path::new(&home).join(".gitconfig");
    if user_file.exists() {
        let (_, out) = run(&c, &env, &[user_file.display().to_string()], limits());
        assert_eq!(out, "BLOCKED");
    }
}

#[test]
fn workers_are_isolated_from_each_other() {
    let c0 = container(0);
    let c1 = container(1);
    let other = setup(&c1, "aplusb");
    let private = other.work.join("output.txt");
    std::fs::write(&private, "other contestant").unwrap();
    let env = setup(&c0, "read_file");
    let (_, out) = run(&c0, &env, &[private.display().to_string()], limits());
    assert_eq!(out, "BLOCKED");
}

#[test]
fn cannot_write_outside() {
    let c = container(0);
    let env = setup(&c, "write_file");
    let target = env.work.parent().unwrap().join("pwned.txt");
    let (_, out) = run(&c, &env, &[target.display().to_string()], limits());
    assert_eq!(out, "BLOCKED");
    assert!(!target.exists());
}

#[test]
fn cannot_spawn_children() {
    let c = container(0);
    let env = setup(&c, "spawn_child");
    let (_, out) = run(&c, &env, &[], limits());
    assert_eq!(out, "BLOCKED");
}

#[test]
fn cannot_read_clipboard() {
    let c = container(0);
    let env = setup(&c, "clipboard");
    let (_, out) = run(&c, &env, &[], limits());
    assert_eq!(out, "BLOCKED");
}

#[test]
fn busy_loop_is_tle() {
    let c = container(0);
    let env = setup(&c, "tle_loop");
    let (r, _) = run(&c, &env, &[], Limits::program(1000, 256 * MIB));
    assert_eq!(r.outcome, Outcome::TimeLimit, "{r:?}");
    assert!(r.wall_ms < 1600, "TLE detection too slow: {r:?}");
}

#[test]
fn sleeping_is_stopped_by_wall_clock() {
    let c = container(0);
    let env = setup(&c, "sleep_forever");
    let (r, _) = run(&c, &env, &[], Limits::program(500, 256 * MIB));
    assert_eq!(r.outcome, Outcome::IdleLimit, "{r:?}");
    assert!(r.wall_ms < 3000, "{r:?}");
}

#[test]
fn heap_growth_is_mle() {
    let c = container(0);
    let env = setup(&c, "mle_heap");
    let (r, _) = run(&c, &env, &[], Limits::program(5000, 128 * MIB));
    assert_eq!(r.outcome, Outcome::MemoryLimit, "{r:?}");
}

#[test]
fn huge_static_array_is_mle() {
    let c = container(0);
    let env = setup(&c, "mle_static");
    let (r, _) = run(&c, &env, &[], Limits::program(5000, 256 * MIB));
    assert_eq!(r.outcome, Outcome::MemoryLimit, "{r:?}");
}

#[test]
fn deep_recursion_with_patched_stack() {
    let c = container(0);
    let env = setup(&c, "deep_recursion");
    let (r, _) = run(&c, &env, &[], Limits::program(5000, 512 * MIB));
    assert_eq!(r.outcome, Outcome::Crashed, "default stack should overflow: {r:?}");

    sandbox::set_stack_reserve(&env.exe, 512 * MIB).unwrap();
    let (r, out) = run(&c, &env, &[], Limits::program(5000, 512 * MIB));
    assert_eq!(r.outcome, Outcome::Ok, "{r:?}");
    assert!(!out.is_empty());
}

#[test]
fn output_flood_is_ole() {
    let c = container(0);
    let env = setup(&c, "output_flood");
    let mut l = Limits::program(5000, 256 * MIB);
    l.output_bytes = 64 * MIB;
    l.watch_files = vec![env.work.join("stdout.txt")];
    let (r, _) = run(&c, &env, &[], l);
    assert_eq!(r.outcome, Outcome::OutputLimit, "{r:?}");
}

#[test]
fn division_by_zero_is_runtime_error() {
    let c = container(0);
    let env = setup(&c, "div_zero");
    let (r, _) = run(&c, &env, &[], limits());
    assert_eq!(r.outcome, Outcome::Crashed, "{r:?}");
    assert_eq!(r.exit_code, 0xC000_0094);
}

#[test]
fn container_profile_folder_is_not_writable() {
    let c = container(0);
    let env = setup(&c, "print_env");
    let (r, out) = run(&c, &env, &[], limits());
    assert_eq!(r.outcome, Outcome::Ok, "{r:?}");
    let probes: Vec<&str> = out.lines().filter(|l| l.starts_with("write ")).collect();
    assert!(!probes.is_empty(), "{out}");
    for line in probes {
        assert!(
            line.ends_with("BLOCKED"),
            "{line}
{out}"
        );
    }
}
