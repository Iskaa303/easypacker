mod api;
mod app;
mod cli;
mod config;
mod export;
mod lock;
mod project;
mod search;
mod tui;
mod types;
mod ui;

use clap::Parser;
use color_eyre::eyre::Result;
use std::path::PathBuf;

#[tokio::main]
async fn main() -> Result<()> {
    color_eyre::install()?;

    let args = cli::Cli::parse();
    if let Some(dir) = &args.dir {
        std::env::set_current_dir(dir)?;
    }
    let cfg = config::Config::load()?;
    let cwd = std::env::current_dir()?;

    // Only `export` needs a lockfile resolved before it runs (it packs what
    // the lock holds). The TUI rebuilds it in the background, and the other
    // commands either never read it (search, list, versions) or rebuild it
    // themselves after editing the manifest (init/add/remove/pin/link/set) —
    // regenerating here made every run wait seconds for the network.
    if let Some(cli::Cmd::Export(e)) = &args.cmd {
        let pack_dir = e.output.as_ref().map_or_else(|| cwd.clone(), PathBuf::from);
        if project::Manifest::detect(&pack_dir).is_some() {
            match lock::generate(&pack_dir, &cfg).await {
                Ok(r) => {
                    eprintln!(
                        "Easypacker.lock: {} pinned, {} failed",
                        r.resolved, r.failed
                    );
                    for warning in &r.warnings {
                        eprintln!("easypacker: {warning}");
                    }
                }
                Err(e) => eprintln!("easypacker: lockfile not updated: {e:#}"),
            }
        }
    }

    match &args.cmd {
        None => tui::run_tui(cfg, project::Manifest::detect(&cwd))?,
        Some(cli::Cmd::Search(a)) => api::run_cli(a, &cfg).await?,
        Some(cli::Cmd::Init(a)) => cli::run_init(a, &cwd)?,
        Some(cli::Cmd::Add(a)) => cli::run_add(a, &cwd, &cfg).await?,
        Some(cli::Cmd::Remove(a)) => cli::run_remove(a, &cwd, &cfg).await?,
        Some(cli::Cmd::Versions(a)) => cli::run_versions(a, &cwd, &cfg).await?,
        Some(cli::Cmd::Pin(a)) => cli::run_pin(a, &cwd, &cfg).await?,
        Some(cli::Cmd::Link(a)) => cli::run_link(a, &cwd, &cfg).await?,
        Some(cli::Cmd::List(a)) => cli::run_list(a, &cwd)?,
        Some(cli::Cmd::Set(a)) => cli::run_set(a, &cwd, &cfg).await?,
        Some(cli::Cmd::Export(a)) => export::run_cli(a).await?,
    }

    Ok(())
}
