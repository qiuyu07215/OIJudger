//! Diagnostic: time a sandboxed build. Usage: compile_probe <files...> [-- extra args]
use judge_core::compile::{build, BuildLimits, BuildOutput, BuildRequest, Lang};
use judge_core::workspace::Workspace;

fn main() -> anyhow::Result<()> {
    sandbox::init_process();
    let args: Vec<String> = std::env::args().skip(1).collect();
    let split = args.iter().position(|a| a == "--").unwrap_or(args.len());
    let files: Vec<_> = args[..split]
        .iter()
        .map(|f| {
            let p = std::path::PathBuf::from(f);
            let name = p.file_name().unwrap().to_string_lossy().into_owned();
            let main = Lang::from_path(&p).is_some();
            (p, name, main)
        })
        .collect();
    let extra = args.get(split + 1..).map(|a| a.join(" ")).unwrap_or_default();
    let compiler = judge_core::detect_compilers().into_iter().find(|c| c.lang == Lang::Cpp).unwrap();
    sandbox::Toolchain::grant_read(&compiler.toolchain_root())?;
    let ws = Workspace::new(1, false)?;
    let cache = tempfile::tempdir()?;
    for i in 0..2 {
        let req =
            BuildRequest { files: files.clone(), compiler: compiler.clone(), extra_args: format!("{extra} -DRUN{i}") };
        let t0 = std::time::Instant::now();
        let out = build(&ws.workers[0], &req, "probe", cache.path(), &BuildLimits::default())?;
        let log = match out {
            BuildOutput::Ok { log, .. } => log,
            BuildOutput::Failed { log } => log,
        };
        println!("run {i}: {} ms\n{log}", t0.elapsed().as_millis());
    }
    Ok(())
}
