//! Command-line arguments and the helpers both transports share.

use clap::{Parser, Subcommand};
use clap_verbosity_flag::Verbosity;

use crate::auth::Transport;
use fontgres_core::cli::CompletionShell;

use crate::config::ServerOverrides;
use fontgres_core::ConfigOverrides;

/// CLI arguments for the MCP server.
#[derive(Parser)]
#[command(
    name = "fontgres-mcp",
    about = "MCP server for querying installed fonts",
    version
)]
pub(crate) struct Args {
    /// Optional subcommand. When omitted, the MCP server runs.
    #[command(subcommand)]
    pub command: Option<Command>,

    #[expect(
        clippy::doc_markdown,
        reason = "clap renders this doc comment verbatim as --help text, so backticks would show"
    )]
    /// Path to config.toml [env: FONTGRES_CONFIG]. A path named here or by
    /// FONTGRES_CONFIG must exist.
    #[arg(long, value_name = "PATH")]
    pub config: Option<std::path::PathBuf>,

    /// Transport to serve the MCP protocol over.
    #[arg(long, value_enum, default_value_t = Transport::Stdio)]
    pub transport: Transport,

    /// Logging verbosity: `-v` warn, `-vv` info, `-vvv` debug, `-vvvv` trace;
    /// `-q` silences output. When passed, overrides `FONTGRES_LOG_LEVEL` and
    /// `[log] level`; with none of them set, the default is info. `RUST_LOG`
    /// still overrides everything.
    #[command(flatten)]
    pub verbosity: Verbosity,
}

/// Subcommands for the MCP server binary.
#[derive(Subcommand)]
pub(crate) enum Command {
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
    /// Generate man pages for the server and all subcommands into <OUT_DIR>.
    Man {
        /// Directory to write the generated man pages into (created if missing).
        out_dir: std::path::PathBuf,
    },
}

impl Args {
    /// Returns the CLI flags as the loader's top layer.
    pub(crate) fn overrides(&self) -> ServerOverrides {
        ServerOverrides {
            core: ConfigOverrides::default(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transport_defaults_to_stdio_and_rejects_unknown_names() {
        let default_args = Args::try_parse_from(["fontgres-mcp"]).unwrap();
        assert_eq!(default_args.transport, Transport::Stdio);
        let parsed = Args::try_parse_from(["fontgres-mcp", "--transport", "sse"]);
        assert_eq!(
            parsed.err().map(|err| err.kind()),
            Some(clap::error::ErrorKind::InvalidValue)
        );
    }
}
