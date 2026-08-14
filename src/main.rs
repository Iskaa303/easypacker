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
    let cfg = config::Config::load()?;
    let cwd = std::env::current_dir()?;

    // Every run: resolve the manifest and regenerate Easypacker.lock. An
    // export reads the pack from --output, so regenerate the lockfile there
    // too — otherwise a stale lock could produce a broken export.
    let pack_dir = if args.export.is_some() {
        args.output
            .as_ref()
            .map_or_else(|| cwd.clone(), PathBuf::from)
    } else {
        cwd.clone()
    };
    let manifest = project::Manifest::detect(&pack_dir);
    if manifest.is_some() {
        match lock::generate(&pack_dir, &cfg).await {
            Ok(r) => eprintln!(
                "Easypacker.lock: {} pinned, {} failed",
                r.resolved, r.failed
            ),
            Err(e) => eprintln!("easypacker: lockfile not updated: {e:#}"),
        }
    }

    if args.query.is_some() {
        api::run_cli(&args, &cfg).await?;
    } else if args.export.is_some() {
        export::run_cli(&args).await?;
    } else {
        tui::run_tui(cfg, manifest)?;
    }

    Ok(())
}
