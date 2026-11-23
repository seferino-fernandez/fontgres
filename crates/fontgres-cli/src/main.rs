//! `fontgres`: query the fonts installed on this machine from the terminal.

#![doc = include_str!("../README.md")]

mod commands;
mod output;

use clap::{CommandFactory, Parser, Subcommand};
use clap_complete::CompleteEnv;
use clap_verbosity_flag::Verbosity;
use output::{OutputFormat, StdoutClosed};
use std::io::Write as _;
use std::path::PathBuf;
use std::process::ExitCode;
use tracing_subscriber::filter::LevelFilter;

use anyhow::Context as _;
use fontgres_core::cli::{self, CompletionShell};
use fontgres_core::{Config, ConfigOverrides};

/// Applies when neither `-v`/`-q`, `FONTGRES_LOG_LEVEL` nor `[log] level` sets one.
const DEFAULT_LOG_LEVEL: LevelFilter = LevelFilter::ERROR;

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

    #[expect(
        clippy::doc_markdown,
        reason = "clap renders this doc comment verbatim as --help text, so backticks would show"
    )]
    /// Path to config.toml [env: FONTGRES_CONFIG]. A path named here or by
    /// FONTGRES_CONFIG must exist.
    #[arg(long, global = true)]
    config: Option<PathBuf>,

    /// Load fonts from this directory, scanned recursively (repeatable).
    /// Replaces `[fonts] dirs` and `FONTGRES_FONTS_DIRS`.
    #[arg(long = "font-dir", global = true, value_name = "DIR")]
    font_dirs: Vec<PathBuf>,

    /// Load this font file (repeatable). Replaces `[fonts] files` and
    /// `FONTGRES_FONTS_FILES`.
    #[arg(long = "font-file", global = true, value_name = "FILE")]
    font_files: Vec<PathBuf>,

    /// Do not load the fonts installed on this system.
    #[arg(long, global = true)]
    no_system_fonts: bool,

    /// Logging verbosity: `-v` warn, `-vv` info, `-vvv` debug, `-vvvv` trace;
    /// `-q` silences output. When passed, overrides `FONTGRES_LOG_LEVEL` and
    /// `[log] level`; with none of them set, only errors are reported. Place
    /// before the subcommand: `fontgres -v faces`. `RUST_LOG` overrides
    /// everything.
    #[command(flatten)]
    verbosity: Verbosity,
}

#[derive(Subcommand)]
enum Command {
    /// List font faces.
    Faces(commands::FacesArgs),
    /// List font family names.
    Families,
    /// Find the face that best matches a CSS-like query.
    Query(commands::QueryArgs),
    /// Generate a shell completion script (bash, elvish, fish, nushell,
    /// powershell, zsh).
    Completions {
        /// Shell to generate the completion script for.
        shell: CompletionShell,
    },
    #[expect(
        clippy::doc_markdown,
        reason = "clap renders this doc comment verbatim as --help text, so backticks would show"
    )]
    /// Generate man pages for the CLI and all subcommands into <OUT_DIR>.
    Man {
        /// Directory to write the generated man pages into (created if missing).
        out_dir: PathBuf,
    },
}

fn main() -> ExitCode {
    // Dynamic completions: `COMPLETE=<shell> fontgres`. A no-op for normal runs; exits
    // the process when handling a completion request, so it must precede any stdout use.
    CompleteEnv::with_factory(Cli::command).complete();

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
    let cli = Cli::parse();

    // Static completion scripts need no config or fonts, so emit and return early.
    if let Command::Completions { shell } = &cli.command {
        cli::write_completions(*shell, Cli::command());
        return Ok(());
    }

    // Man pages need no config or fonts, so emit and return early.
    if let Command::Man { out_dir } = &cli.command {
        cli::write_man_pages(Cli::command(), out_dir)?;
        return Ok(());
    }

    let overrides = ConfigOverrides {
        system: cli.no_system_fonts.then_some(false),
        dirs: cli.font_dirs.clone(),
        files: cli.font_files.clone(),
    };
    let config = Config::load(cli.config.as_deref(), &overrides).context("loading config")?;

    // Default level precedence: explicit `-v`/`-q` overrides config `log.level`,
    // which overrides `error`; `RUST_LOG` (via `try_from_default_env`) still
    // overrides everything.
    let flag = cli
        .verbosity
        .is_present()
        .then(|| cli.verbosity.tracing_level_filter());
    let (level, warning) = config.log.resolve_level(flag, DEFAULT_LOG_LEVEL);
    if let Some(warning) = warning {
        writeln!(std::io::stderr().lock(), "fontgres: warning: {warning}")?;
    }
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| {
                tracing_subscriber::EnvFilter::new(format!("fontgres_core={level}"))
            }),
        )
        .with_writer(std::io::stderr)
        .init();

    let database = config.fonts.build_database().context("loading fonts")?;
    match &cli.command {
        Command::Faces(args) => commands::faces(&database, args, cli.output),
        Command::Families => commands::families(&database, cli.output),
        Command::Query(args) => commands::query(&database, args, cli.output),
        // Handled by the early returns above.
        Command::Completions { .. } | Command::Man { .. } => Ok(()),
    }
}
