mod active;
mod store;
mod readme;
mod gitops;
mod util;

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name="blazectl", version, about="Train/Battle time logger (UTC)")]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Start a session: train | battle
    Start { tag: String },
    /// Stop a session: train | battle
    Stop  { tag: String },
    /// Show active session, if any
    Status,
    /// Force README regeneration
    RenderReadme,
}

fn main() {
    let cli = Cli::parse();

    // Ensure .blaze exists
    store::ensure_dirs().expect(".blaze init failed");

    match cli.cmd {
        Cmd::Start { tag } => {
            active::start(&tag).unwrap_or_else(|e| {
                eprintln!("start error: {e}");
                std::process::exit(1);
            });
        }
        Cmd::Stop { tag } => {
            match active::stop(&tag) {
                Ok(Some(entry)) => {
                    if let Err(e) = store::append_entry(&entry) {
                        eprintln!("append error: {e}");
                        std::process::exit(1);
                    }
                    // Synchronous: README + daily commit
                    // NOTE:
                    // SVG + README writes are buffered. We do a naive mtime poll before auto-commit.
                    // This is not perfectly reliable under fs writeback delays.
                    // Acceptable for personal workflow; revisit if failures become annoying.

                    let before = std::fs::metadata("assets/activity.svg")
                        .ok()
                        .and_then(|m| m.modified().ok());

                    if let Err(e) = readme::render_all() { eprintln!("readme: {e}"); }

                    for _ in 0..20 {
                        let now = std::fs::metadata("assets/activity.svg")
                            .ok()
                            .and_then(|m| m.modified().ok());

                        if now != before { break; }
                        std::thread::sleep(std::time::Duration::from_millis(100));
                    }

                    if let Err(e) = gitops::auto_commit_if_due() { eprintln!("git: {e}"); }
                }
                Ok(None) => {
                    println!("No active `{tag}` session.");
                }
                Err(e) => {
                    eprintln!("stop error: {e}");
                    std::process::exit(1);
                }
            }
        }
        Cmd::Status => {
            match active::status() {
                Ok(Some((tag, start))) => println!("Active: {tag} since {start} (UTC)"),
                Ok(None) => println!("No active session."),
                Err(e) => { eprintln!("status error: {e}"); std::process::exit(1); }
            }
        }
        Cmd::RenderReadme => {
            if let Err(e) = readme::render_all() {
                eprintln!("readme: {e}");
                std::process::exit(1);
            }
        }
    }
}
