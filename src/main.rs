mod api;
mod app;
mod cli;
mod config;
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

    if args.query.is_some() {
        api::run_cli(&args, &cfg).await?;
    } else {
        let project = project::ModpackProject::detect(&std::env::current_dir()?);
        tui::run_tui(cfg, project).await?;
    }

    Ok(())
}
