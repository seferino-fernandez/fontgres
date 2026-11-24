//! The server's command-line arguments.

use clap::Parser;
use fontgres_core::cli::{SetupCommand, SharedArgs};

use crate::auth::Transport;

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
    pub command: Option<SetupCommand>,

    #[command(flatten)]
    pub shared: SharedArgs,

    /// Transport to serve the MCP protocol over.
    #[arg(long, value_enum, default_value_t = Transport::Stdio)]
    pub transport: Transport,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_command_line_is_well_formed() {
        use clap::CommandFactory as _;

        Args::command().debug_assert();
    }

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
