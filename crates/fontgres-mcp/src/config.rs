//! Server configuration: core config + MCP-specific settings.
//!
//! [`ServerConfig`] embeds the core [`Config`] and adds an
//! [`McpConfig`] for the MCP server. Both are loaded
//! from the same `config.toml`: core tables such as `[fonts]` and `[log]`,
//! MCP fields under `[mcp]`. Environment overrides use the `FONTGRES_MCP_*`
//! prefix. CLI flags arrive as [`ServerOverrides`], the top layer above all of
//! these.

use fontgres_core::env::{Env, SystemEnv, env_parse};
use fontgres_core::{Config, ConfigOverrides};
use serde::Deserialize;
use std::fmt::{Debug, Formatter};
use std::path::Path;

use crate::auth::Transport;
const ENV_MCP_MAX_RESPONSE_BYTES: &str = "FONTGRES_MCP_MAX_RESPONSE_BYTES";

/// Default cap on serialized tool-result size: 1 MiB. Generous enough not to
/// clip normal results, while guarding against pathological payloads.
const DEFAULT_MAX_RESPONSE_BYTES: usize = 1024 * 1024;

/// MCP-server-specific settings.
#[derive(Clone, Deserialize)]
#[serde(default)]
pub(crate) struct McpConfig {
    /// Maximum exact serialized size (bytes) of a tool result. Results
    /// exceeding this are replaced with a bounded `response_too_large` error;
    /// if even that cannot fit, the call fails with a protocol error. Defaults
    /// to 1 MiB.
    pub max_response_bytes: usize,
}

impl Default for McpConfig {
    fn default() -> Self {
        Self {
            max_response_bytes: DEFAULT_MAX_RESPONSE_BYTES,
        }
    }
}

impl Debug for McpConfig {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("McpConfig")
            .field("max_response_bytes", &self.max_response_bytes)
            .finish()
    }
}

/// Full server configuration.
#[derive(Debug, Clone, Default)]
pub(crate) struct ServerConfig {
    /// Core config: fonts and logging.
    pub core: Config,
    /// MCP server settings.
    pub mcp: McpConfig,
}

/// Explicit values that sit above every other layer: the server's CLI flags.
///
/// Each one beats the matching env var and TOML setting.
#[derive(Debug, Default)]
pub(crate) struct ServerOverrides {
    /// Core overrides (the font flags).
    pub core: ConfigOverrides,
}

/// On-disk shape used only to extract the `[mcp]` table; core fields are parsed
/// by [`Config`].
#[derive(Debug, Default, Deserialize)]
struct McpConfigFile {
    #[serde(default)]
    mcp: McpConfig,
}

impl ServerConfig {
    /// Loads core config + `[mcp]` from `path` (or default), then env, then
    /// `overrides`.
    ///
    /// A file named by `path` or `FONTGRES_CONFIG` must exist, as in
    /// [`Config::load`]; a missing file at a default location means defaults.
    ///
    /// Reads environment variables from the real process environment via
    /// [`SystemEnv`]. For test injection, see [`ServerConfig::load_with_env`].
    pub(crate) fn load(
        path: Option<&Path>,
        overrides: &ServerOverrides,
        transport: Transport,
    ) -> anyhow::Result<Self> {
        Self::load_with_env(path, overrides, &SystemEnv, transport)
    }

    /// Like [`ServerConfig::load`], but reads env vars from the given [`Env`].
    pub(crate) fn load_with_env(
        path: Option<&Path>,
        overrides: &ServerOverrides,
        env: &impl Env,
        transport: Transport,
    ) -> anyhow::Result<Self> {
        let core = Config::load_with_env(path, &overrides.core, env)?;
        let mut mcp = Config::read_config_file::<McpConfigFile>(path, env)?.mcp;

        apply_mcp_env(&mut mcp, env)?;
        match transport {
            Transport::Stdio => {}
        }
        Ok(Self { core, mcp })
    }
}

/// Applies the `FONTGRES_MCP_*` env vars, which beat TOML. A value that
/// does not parse is an error naming the variable and the raw value.
fn apply_mcp_env(mcp: &mut McpConfig, env: &impl Env) -> anyhow::Result<()> {
    if let Some(val) = env_parse::<u64>(env, ENV_MCP_MAX_RESPONSE_BYTES)? {
        // Saturate rather than truncate on 32-bit targets: an absurdly large cap
        // simply becomes "effectively unbounded".
        mcp.max_response_bytes = usize::try_from(val).unwrap_or(usize::MAX);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use fontgres_core::env::MapEnv;
    use std::fs;

    /// Writes `toml` as the config file and returns an env pointing at it,
    /// with the temp directory that holds it. Keeps every load away from the
    /// real user config.
    fn env_with_toml(toml: &str) -> (tempfile::TempDir, MapEnv) {
        let directory = tempfile::tempdir().unwrap();
        let config = directory.path().join("config.toml");
        fs::write(&config, toml).unwrap();
        let env = MapEnv::new().with("FONTGRES_CONFIG", config.to_string_lossy().into_owned());
        (directory, env)
    }

    fn isolated_env() -> (tempfile::TempDir, MapEnv) {
        env_with_toml("")
    }

    fn load_stdio(env: &MapEnv) -> anyhow::Result<ServerConfig> {
        ServerConfig::load_with_env(None, &ServerOverrides::default(), env, Transport::Stdio)
    }

    #[test]
    fn max_response_bytes_defaults_to_one_mib() {
        let config = ServerConfig::default();
        assert_eq!(config.mcp.max_response_bytes, 1024 * 1024);
    }

    #[test]
    fn max_response_bytes_env_beats_toml() {
        let (_directory, env) = env_with_toml("[mcp]\nmax_response_bytes = 2048\n");
        assert_eq!(load_stdio(&env).unwrap().mcp.max_response_bytes, 2048);
        let env = env.with("FONTGRES_MCP_MAX_RESPONSE_BYTES", "4096");
        assert_eq!(load_stdio(&env).unwrap().mcp.max_response_bytes, 4096);
    }

    #[test]
    fn invalid_max_response_bytes_env_fails_naming_variable_and_value() {
        let (_directory, env) = isolated_env();
        let env = env.with("FONTGRES_MCP_MAX_RESPONSE_BYTES", "lots");
        let err = load_stdio(&env).unwrap_err();
        assert!(
            err.to_string()
                .contains(r#"invalid FONTGRES_MCP_MAX_RESPONSE_BYTES value "lots""#),
            "{err}"
        );
    }

    #[test]
    fn core_tables_load_from_the_same_file() {
        let (_directory, env) =
            env_with_toml("[fonts]\nsystem = false\n[mcp]\nmax_response_bytes = 10\n");
        let config = load_stdio(&env).unwrap();
        assert!(!config.core.fonts.system);
        assert_eq!(config.mcp.max_response_bytes, 10);
    }

    #[test]
    fn missing_explicit_config_file_is_an_error() {
        let directory = tempfile::tempdir().unwrap();
        let missing = directory.path().join("typo.toml");
        let err = ServerConfig::load_with_env(
            Some(&missing),
            &ServerOverrides::default(),
            &MapEnv::new(),
            Transport::Stdio,
        )
        .unwrap_err();
        assert!(
            err.to_string()
                .contains(&format!("config file {} does not exist", missing.display())),
            "{err}"
        );
    }
}
