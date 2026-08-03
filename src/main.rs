mod api;
mod app;
mod cli;
mod config;
mod lock;
mod project;
mod search;
mod tui;
mod types;
mod ui;

use clap::Parser;
use color_eyre::eyre::Result;

#[tokio::main]
async fn main() -> Result<()> {
    color_eyre::install()?;

    let args = cli::Cli::parse();
    let cfg = config::Config::load()?;
    let cwd = std::env::current_dir()?;

    // Every run: resolve the manifest and regenerate Easypacker.lock.
    let manifest = project::Manifest::detect(&cwd);
    if manifest.is_some() {
        match lock::generate(&cwd, &cfg).await {
            Ok(r) => eprintln!("Easypacker.lock: {} pinned, {} failed", r.resolved, r.failed),
            Err(e) => eprintln!("easypacker: lockfile not updated: {e:#}"),
        }
    }

    if args.query.is_some() {
        api::run_cli(&args, &cfg).await?;
    } else {
        tui::run_tui(cfg, manifest).await?;
    }

    Ok(())
}
