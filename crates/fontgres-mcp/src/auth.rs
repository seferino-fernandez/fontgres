//! Transport and endpoint authentication vocabulary.

use std::fmt::{Display, Formatter};

/// How the server talks to its MCP client, chosen with `--transport`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, clap::ValueEnum)]
pub(crate) enum Transport {
    /// Standard input and output, for a local client.
    #[default]
    Stdio,
}

impl Display for Transport {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Stdio => "stdio",
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::ValueEnum;

    #[test]
    fn transport_names_round_trip_through_clap() {
        for transport in Transport::value_variants() {
            let name = transport.to_string();
            assert_eq!(Transport::from_str(&name, false), Ok(*transport), "{name}");
        }
        assert_eq!(Transport::default(), Transport::Stdio);
    }
}
