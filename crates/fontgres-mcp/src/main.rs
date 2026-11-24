//! `fontgres-mcp`: Model Context Protocol server exposing the font database as
//! tools an LLM client can call.
//!
//! Serves local MCP clients over standard input and output.

#![doc = include_str!("../README.md")]

mod auth;
mod config;
mod shared;
mod stdio;
mod tools;

use fontgres_core::cli::{self, LevelFilter, Logging};
use fontgres_core::env::SystemEnv;
use shared::Args;

use crate::auth::Transport;
use crate::config::ServerConfig;

/// The server reports its start and what it serves unless `-v`/`-q`,
/// `FONTGRES_LOG_LEVEL` or `[log] level` says otherwise.
const LOGGING: Logging<'_> = Logging {
    default_level: LevelFilter::INFO,
    targets: &["fontgres_core", "fontgres_mcp", "rmcp"],
};

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

    tracing::info!(
        transport = %args.transport,
        "fontgres-mcp starting"
    );

    match args.transport {
        Transport::Stdio => stdio::serve(&config).await?,
    }

    Ok(())
}
