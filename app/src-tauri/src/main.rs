// No console window for the GUI; command line use attaches to the parent's.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod commands;
mod progress;

use clap::Parser;

fn main() {
    sandbox::init_process();

    // `OIJudger.exe judge <dir>` etc. behaves like the command line tool.
    if std::env::args_os().len() > 1 {
        #[cfg(windows)]
        unsafe {
            use windows_sys::Win32::System::Console::{AttachConsole, ATTACH_PARENT_PROCESS};
            AttachConsole(ATTACH_PARENT_PROCESS);
        }
        let cli = oij::cli::Cli::parse();
        let code = match oij::cli::run(cli) {
            Ok(c) => c,
            Err(e) => {
                eprintln!("error: {e:#}");
                2
            }
        };
        std::process::exit(code);
    }

    // The GUI has no console: report startup problems in a message box
    // instead of disappearing without a word.
    if sandbox::integrity_level().is_some_and(|l| l < sandbox::INTEGRITY_MEDIUM) {
        let dir = std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(|d| d.display().to_string()))
            .unwrap_or_default();
        fatal_box(&format!(
            "OIJudger 无法启动：程序正以“低完整性”级别运行。\n\n\
             原因：程序所在的文件夹\n{dir}\n带有“低完整性”标签（常见于沙箱或安全工具管理的目录）。\
             在其中运行的程序权限受限，无法创建界面，也无法使用评测沙箱。\n\n\
             解决：把 OIJudger 压缩包重新解压到普通文件夹（例如 D:\\OIJudger 或桌面）后再运行。\n\n\
             ----\n\
             OIJudger is running at Low integrity because its folder carries a Low integrity label. \
             Extract it into a normal folder (e.g. D:\\OIJudger) and run it from there."
        ));
        std::process::exit(1);
    }
    std::panic::set_hook(Box::new(|info| {
        let msg = info
            .payload()
            .downcast_ref::<&str>()
            .map(|s| s.to_string())
            .or_else(|| info.payload().downcast_ref::<String>().cloned())
            .unwrap_or_else(|| "unknown error".into());
        let at = info.location().map(|l| format!("\n\n({}:{})", l.file(), l.line())).unwrap_or_default();
        fatal_box(&format!("OIJudger 出现错误，即将退出。\nOIJudger hit an error and will close.\n\n{msg}{at}"));
    }));

    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .manage(commands::AppState::default())
        .invoke_handler(tauri::generate_handler![
            commands::app_info,
            commands::get_settings,
            commands::save_settings,
            commands::detect_compilers,
            commands::open_contest,
            commands::reload_contest,
            commands::close_contest,
            commands::save_contest_config,
            commands::detect_problems,
            commands::get_result,
            commands::read_source,
            commands::list_data_files,
            commands::start_judge,
            commands::cancel_judge,
            commands::judge_status,
            commands::run_selftest,
            commands::get_stats,
            commands::export_results,
            commands::similarity,
            commands::cache_usage,
            commands::clear_cache,
        ])
        .run(tauri::generate_context!())
        .expect("error while running OIJudger");
}

fn fatal_box(text: &str) {
    eprintln!("{text}");
    #[cfg(windows)]
    unsafe {
        use windows_sys::Win32::UI::WindowsAndMessaging::{MessageBoxW, MB_ICONERROR, MB_OK};
        let w = |s: &str| s.encode_utf16().chain(std::iter::once(0)).collect::<Vec<u16>>();
        MessageBoxW(std::ptr::null_mut(), w(text).as_ptr(), w("OIJudger").as_ptr(), MB_OK | MB_ICONERROR);
    }
}
