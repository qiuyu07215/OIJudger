//! Diagnostic: measure per-run overhead. Usage: run_bench <program.exe> [runs]
use std::time::Instant;

#[cfg(windows)]
fn main() {
    use sandbox::{Container, Limits, RunSpec, Stdio};
    sandbox::init_process();
    let args: Vec<String> = std::env::args().collect();
    let exe = std::path::PathBuf::from(&args[1]);
    let runs: usize = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(50);
    let tmp = std::env::temp_dir().join(format!("oij-bench-{}", std::process::id()));
    std::fs::create_dir_all(&tmp).unwrap();
    let c = Container::open(&format!("OIJudger.Bench.{}", std::process::id())).unwrap();
    c.grant_dir(&tmp).unwrap();
    let prog = tmp.join("p.exe");
    std::fs::copy(&exe, &prog).unwrap();
    let input = tmp.join("in.txt");
    std::fs::write(&input, "1 2\n").unwrap();

    for (label, container) in [("isolated", &c), ("job only", &Container::unisolated())] {
        // warm up (first execution of a new exe is scanned by antivirus)
        let _ = container.run(spec(&prog, &tmp, &input));
        let t0 = Instant::now();
        let mut cpu = 0;
        for _ in 0..runs {
            let r = container.run(spec(&prog, &tmp, &input)).unwrap();
            cpu += r.cpu_ms;
        }
        let per = t0.elapsed().as_secs_f64() * 1000.0 / runs as f64;
        println!("{label:>9}: {per:.1} ms wall / run, {:.1} ms cpu / run", cpu as f64 / runs as f64);
    }
    {
        let t0 = Instant::now();
        for _ in 0..runs {
            std::process::Command::new(&prog)
                .stdin(std::fs::File::open(&input).unwrap())
                .stdout(std::fs::File::create(tmp.join("out2.txt")).unwrap())
                .status()
                .unwrap();
        }
        println!(
            " baseline: {:.1} ms wall / run (std::process, no limits)",
            t0.elapsed().as_secs_f64() * 1000.0 / runs as f64
        );
    }
    let _ = std::fs::remove_dir_all(&tmp);
    Container::delete_profile(&format!("OIJudger.Bench.{}", std::process::id()));

    fn spec(prog: &std::path::Path, dir: &std::path::Path, input: &std::path::Path) -> RunSpec {
        let mut s = RunSpec::new(prog, dir, Limits::program(1000, 256 << 20));
        s.stdin = Stdio::File(std::fs::File::open(input).unwrap());
        s.stdout = Stdio::File(std::fs::File::create(dir.join("out.txt")).unwrap());
        s
    }
}

#[cfg(not(windows))]
fn main() {}
