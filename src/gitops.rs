use anyhow::Result;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

fn last_commit_ts() -> Option<i64> {
    let out = Command::new("git").args(["log","-1","--format=%ct"]).output().ok()?;
    if !out.status.success() { return None; }
    let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
    s.parse::<i64>().ok()
}

fn now_ts() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs() as i64
}

pub fn auto_commit_if_due() -> Result<()> {
    // only if in a git repo
    if !std::path::Path::new(".git").exists() { return Ok(()); }

    let due = match last_commit_ts() {
        Some(ts) => now_ts() - ts >= 24*3600,
        None => true, // no commits yet
    };
    if !due { return Ok(()); }

    // add & commit if changes exist
    let _ = Command::new("git").args(["add","README.md"]).status();
    let _ = Command::new("git").args(["add",".blaze/active.json"]).status();
    let _ = Command::new("git").args(["add",".blaze/"]).status();
    let msg = format!("blazectl: update ({})", chrono::Utc::now().format("%Y-%m-%d UTC"));
    let _ = Command::new("git").args(["commit","-m",&msg]).status();
    Ok(())
}
