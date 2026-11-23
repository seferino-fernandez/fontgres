//! `fontgres-mcp`: Model Context Protocol server exposing the FONTGRES API as tools
//! an LLM client can call.
//!
//! Serves local MCP clients over standard input and output.

#![doc = include_str!("../README.md")]

mod auth;
mod config;
mod shared;
mod stdio;
mod tools;

use std::io::{Write, stderr};

use clap::{CommandFactory, Parser};
use clap_complete::CompleteEnv;
use shared::{Args, Command};
use tracing::level_filters::LevelFilter;

use crate::auth::Transport;
use fontgres_core::cli;

use crate::config::ServerConfig;

/// Applies when neither `-v`/`-q`, `FONTGRES_LOG_LEVEL` nor `[log] level` sets one.
const DEFAULT_LOG_LEVEL: LevelFilter = LevelFilter::INFO;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Dynamic completions: `COMPLETE=<shell> fontgres-mcp`. A no-op for normal runs; exits
    // the process when handling a completion request, so it must precede any stdout use.
    CompleteEnv::with_factory(Args::command).complete();

    let args = Args::parse();

    // Static completion scripts need no config, so emit and return early.
    if let Some(Command::Completions { shell }) = &args.command {
        cli::write_completions(*shell, Args::command());
        return Ok(());
    }

    // Man pages need no config, so emit and return early.
    if let Some(Command::Man { out_dir }) = &args.command {
        cli::write_man_pages(Args::command(), out_dir)?;
        return Ok(());
    }

    let config = ServerConfig::load(args.config.as_deref(), &args.overrides(), args.transport)?;

    // Default level precedence: explicit `-v`/`-q` overrides config `log.level`,
    // which overrides `info`; `RUST_LOG` (via `try_from_default_env`) still
    // overrides everything.
    let flag = args
        .verbosity
        .is_present()
        .then(|| args.verbosity.tracing_level_filter());
    let (level, warning) = config.core.log.resolve_level(flag, DEFAULT_LOG_LEVEL);
    if let Some(warning) = warning {
        writeln!(stderr().lock(), "fontgres-mcp: warning: {warning}")?;
    }
    tracing_subscriber::fmt()
        .with_writer(stderr)
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| {
                tracing_subscriber::EnvFilter::new(format!(
                    "fontgres_core={level},fontgres_mcp={level},rmcp={level}"
                ))
            }),
        )
        .init();

    tracing::info!(
        transport = %args.transport,
        "fontgres-mcp starting"
    );

    match args.transport {
        Transport::Stdio => stdio::serve(&config).await?,
    }

    Ok(())
}
