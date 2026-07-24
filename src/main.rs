mod api;
mod cli;
mod config;
mod tui;

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
        tui::run_tui(args, cfg).await?;
    }

    Ok(())
}
