//! `fontgres`: query the fonts installed on this machine from the terminal.

#![doc = include_str!("../README.md")]

mod commands;
mod output;

use clap::{Parser, Subcommand};
use output::{OutputFormat, StdoutClosed};
use std::process::ExitCode;

use anyhow::Context as _;
use fontgres_core::cli::{self, LevelFilter, Logging, SetupCommand, SharedArgs};
use fontgres_core::config::UnreadTable;
use fontgres_core::env::SystemEnv;
use fontgres_core::models::FontQuery;

/// Only errors are reported unless `-v`/`-q`, `FONTGRES_LOG_LEVEL` or
/// `[log] level` says otherwise.
const LOGGING: Logging<'_> = Logging {
    default_level: LevelFilter::ERROR,
    targets: &["fontgres_core"],
};

#[derive(Parser)]
#[command(
    name = "fontgres",
    about = "Query installed fonts from the terminal",
    version
)]
struct Cli {
    #[command(subcommand)]
    command: Command,

    /// Output format.
    #[arg(long, global = true, default_value = "table")]
    output: OutputFormat,

    #[command(flatten)]
    shared: SharedArgs,
}

#[derive(Subcommand)]
enum Command {
    /// List font faces.
    Faces(commands::FacesArgs),
    /// List font family names.
    Families,
    /// Find the face that best matches a CSS-like query.
    Query(FontQuery),
    #[command(flatten)]
    Setup(SetupCommand),
}

fn main() -> ExitCode {
    match try_main() {
        Ok(()) => ExitCode::SUCCESS,
        // A closed stdout pipe (`... | head`) means the reader is done, not a failure.
        Err(error) if error.is::<StdoutClosed>() => ExitCode::SUCCESS,
        Err(error) => {
            report_fatal_error(&error);
            ExitCode::FAILURE
        }
    }
}

#[expect(
    clippy::print_stderr,
    reason = "the CLI's single fatal-error report: once main is reached there is no caller left \
              to propagate a failed stderr write to"
)]
fn report_fatal_error(error: &anyhow::Error) {
    eprintln!("fontgres: {error:#}");
}

fn try_main() -> anyhow::Result<()> {
    let cli: Cli = cli::parse();

    // Completion scripts and man pages need no config or fonts.
    if let Command::Setup(setup) = &cli.command {
        setup.run::<Cli>()?;
        return Ok(());
    }

    let (config, _) = cli
        .shared
        .load_config::<UnreadTable>(&SystemEnv)
        .context("loading config")?;
    cli.shared.start_logging(&LOGGING, config.log, &SystemEnv);

    let database = config.fonts.build_database().context("loading fonts")?;
    match &cli.command {
        Command::Faces(args) => commands::faces(&database, args, cli.output),
        Command::Families => commands::families(&database, cli.output),
        Command::Query(query) => commands::query(&database, query, cli.output),
        // Handled by the early return above.
        Command::Setup(_) => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::Cli;

    #[test]
    fn the_command_line_is_well_formed() {
        use clap::CommandFactory as _;

        Cli::command().debug_assert();
    }
}
