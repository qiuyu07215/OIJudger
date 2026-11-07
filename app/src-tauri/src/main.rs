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
        ])
        .run(tauri::generate_context!())
        .expect("error while running OIJudger");
}
