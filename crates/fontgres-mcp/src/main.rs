//! `fontgres-mcp`: Model Context Protocol server exposing the font database as
//! tools an LLM client can call.
//!
//! Serves local MCP clients over standard input and output.

#![doc = include_str!("../README.md")]

mod config;
mod server;
#[cfg(test)]
mod test_support;
mod tools;

use anyhow::Context as _;
use clap::Parser;
use fontgres_core::cli::{self, LevelFilter, Logging, SetupCommand, SharedArgs};
use fontgres_core::env::SystemEnv;
use rmcp::ServiceExt as _;

use crate::config::ServerConfig;
use crate::server::Server;

/// The server reports its start and what it serves unless `-v`/`-q`,
/// `FONTGRES_LOG_LEVEL` or `[log] level` says otherwise.
const LOGGING: Logging<'_> = Logging {
    default_level: LevelFilter::INFO,
    targets: &["fontgres_core", "fontgres_mcp", "rmcp"],
};

/// CLI arguments for the MCP server.
#[derive(Parser)]
#[command(
    name = "fontgres-mcp",
    about = "MCP server for querying installed fonts",
    version
)]
struct Args {
    /// Optional subcommand. When omitted, the MCP server runs.
    #[command(subcommand)]
    command: Option<SetupCommand>,

    #[command(flatten)]
    shared: SharedArgs,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args: Args = cli::parse();

    // Completion scripts and man pages need no config.
    if let Some(setup) = &args.command {
        setup.run::<Args>()?;
        return Ok(());
    }

    let config = ServerConfig::load(&args.shared, &SystemEnv)?;
    args.shared
        .start_logging(&LOGGING, config.core.log, &SystemEnv);

    tracing::info!("fontgres-mcp starting");
    serve(&config).await
}

/// Loads the configured fonts, then serves the tools over standard input and
/// output until the client closes them.
async fn serve(config: &ServerConfig) -> anyhow::Result<()> {
    let fonts = config.core.fonts.clone();
    // Scanning system fonts reads many files, so keep it off the async runtime.
    let database = tokio::task::spawn_blocking(move || fonts.build_database())
        .await
        .context("loading fonts")?
        .context("loading fonts")?;
    tracing::info!(faces = database.len(), "fonts loaded");
    let server = Server::new(database, config.mcp.max_response_bytes);
    let service = server.serve(rmcp::transport::io::stdio_pipes()).await?;
    service.waiting().await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::Args;

    #[test]
    fn the_command_line_is_well_formed() {
        use clap::CommandFactory as _;

        Args::command().debug_assert();
    }
}
