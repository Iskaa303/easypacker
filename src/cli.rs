use clap::Parser;

#[derive(Parser, Debug)]
#[command(name = "easypacker", version, about = "Minecraft Modpack creator")]
pub struct Cli {
    /// Search query (omit to open TUI)
    #[arg(short, long)]
    pub query: Option<String>,

    /// Use CurseForge (default: Modrinth)
    #[arg(long)]
    pub cf: bool,

    /// Use Modrinth explicitly
    #[arg(long)]
    pub modrinth: bool,

    /// Minecraft version filter (e.g. 1.20.1)
    #[arg(short, long)]
    pub version: Option<String>,

    /// Mod loader filter (fabric, forge, neoforge, quilt)
    #[arg(short, long)]
    pub loader: Option<String>,

    /// Project type (mod, resourcepack, datapack, shader)
    #[arg(short = 'y', long)]
    pub r#type: Option<String>,
    /// Sort order (relevance, downloads, follows, updated)
    #[arg(short, long, default_value = "relevance")]
    pub sort: String,

    /// Max results
    #[arg(short = 'n', long, default_value_t = 25)]
    pub limit: usize,

    /// Result offset
    #[arg(long, default_value_t = 0)]
    pub offset: usize,

    /// CurseForge API key (overrides config file / env)
    #[arg(long)]
    pub api_key: Option<String>,
}
