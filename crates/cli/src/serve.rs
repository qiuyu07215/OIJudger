//! `oijudger serve` and `oijudger accounts`: online contests from the
//! command line.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use judge_core::AppSettings;
use online::OnlineContest;

/// Run the web server until Ctrl+C.
pub fn serve(dir: &Path, bind: Option<String>, settings: AppSettings) -> Result<i32> {
    let rt = tokio::runtime::Builder::new_multi_thread().enable_all().build()?;
    rt.block_on(async move {
        let (tx, notify) = server::channel();
        let contest = OnlineContest::open(dir, settings, notify)?;
        if let Some(pw) = contest.ensure_admin()? {
            println!("Created account 'admin' with password: {pw}");
            println!("  (keep it; reset with `oijudger-cli accounts reset {} admin`)", dir.display());
        }
        match contest.start_judging() {
            Ok(()) => println!("Judging service started."),
            Err(e) => eprintln!("warning: judging is not running: {e:#}"),
        }
        let bind = bind.unwrap_or_else(|| contest.config().bind);
        let state = server::AppState::new(contest.clone(), tx);
        let handle = server::start(state, &bind).await?;
        let port = handle.addr.port();
        println!("Serving {} on {}", contest.contest().title, handle.addr);
        if handle.addr.ip().is_unspecified() {
            for ip in server::lan_addresses() {
                println!("  contestants:  http://{ip}:{port}/");
            }
        }
        println!("  this machine: http://127.0.0.1:{port}/   (jury: /jury, scoreboard: /scoreboard)");
        println!("Press Ctrl+C to stop.");
        tokio::signal::ctrl_c().await.ok();
        println!("Stopping…");
        handle.stop().await;
        let c = contest.clone();
        tokio::task::spawn_blocking(move || c.stop_judging()).await.ok();
        Ok(0)
    })
}

pub fn import(dir: &Path, csv: &Path, out: Option<PathBuf>, settings: AppSettings) -> Result<i32> {
    let contest = OnlineContest::open(dir, settings, std::sync::Arc::new(|_| {}))?;
    let text = std::fs::read_to_string(csv).with_context(|| format!("reading {}", csv.display()))?;
    let list = online::accounts::parse_account_csv(&text)?;
    let mut lines = vec!["username,display_name,group,seat,password".to_string()];
    for (a, pw) in list {
        let (_, password) = contest.create_account(a.clone(), pw)?;
        lines.push(format!("{},{},{},{},{}", a.username, csv_field(&a.display_name), csv_field(&a.group_name), csv_field(&a.seat), password));
    }
    let n = lines.len() - 1;
    match out {
        Some(p) => {
            std::fs::write(&p, "\u{feff}".to_string() + &lines.join("\r\n") + "\r\n")?;
            println!("Created {n} account(s); passwords written to {}", p.display());
        }
        None => {
            for l in &lines {
                println!("{l}");
            }
        }
    }
    Ok(0)
}

pub fn list(dir: &Path, settings: AppSettings) -> Result<i32> {
    let contest = OnlineContest::open(dir, settings, std::sync::Arc::new(|_| {}))?;
    for a in contest.db.accounts()? {
        println!(
            "{:<20} {:<10} {:<24} {:<16} {}{}",
            a.username,
            a.role.as_str(),
            a.display_name,
            a.group_name,
            a.seat,
            if a.disabled { "  (disabled)" } else { "" }
        );
    }
    Ok(0)
}

pub fn reset(dir: &Path, username: &str, settings: AppSettings) -> Result<i32> {
    let contest = OnlineContest::open(dir, settings, std::sync::Arc::new(|_| {}))?;
    let (a, _) = contest.db.account_for_login(username)?.with_context(|| format!("no account {username}"))?;
    let pw = contest.reset_password(a.id, None)?;
    println!("New password for {}: {pw}", a.username);
    Ok(0)
}

fn csv_field(s: &str) -> String {
    if s.contains([',', '"', '\n']) {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_string()
    }
}
