//! Server configuration: core config + MCP-specific settings.
//!
//! [`ServerConfig`] holds the core [`Config`] and an [`McpConfig`] for the
//! MCP server. One read of `config.toml` gives both: core tables such as
//! `[fonts]` and `[log]`, MCP fields under `[mcp]`. Environment overrides
//! use the `FONTGRES_MCP_*` prefix.

use fontgres_core::Config;
use fontgres_core::cli::SharedArgs;
use fontgres_core::env::{Env, env_parse};
use serde::Deserialize;
use std::fmt::{Debug, Formatter};

const ENV_MCP_MAX_RESPONSE_BYTES: &str = "FONTGRES_MCP_MAX_RESPONSE_BYTES";

/// Default cap on serialized tool-result size: 1 MiB. A default page of a
/// listing is far below it; a call that asks for a larger page gets one cut to
/// fit.
const DEFAULT_MAX_RESPONSE_BYTES: usize = 1024 * 1024;

/// MCP-server-specific settings.
#[derive(Clone, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub(crate) struct McpConfig {
    /// Maximum exact serialized size (bytes) of a tool result. The list
    /// tools cut each page to fit it. A result that one item alone pushes
    /// over it is replaced with a bounded `response_too_large` error; if even
    /// that cannot fit, the call fails with a protocol error. Defaults to
    /// 1 MiB.
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

impl ServerConfig {
    /// Loads the configuration `args` select, as
    /// [`SharedArgs::load_config`] does, with the `[mcp]` table and the
    /// `FONTGRES_MCP_*` variables, read through `env`, on top of it.
    pub(crate) fn load(args: &SharedArgs, env: &impl Env) -> anyhow::Result<Self> {
        let (core, mut mcp) = args.load_config::<McpConfig>(env)?;
        apply_mcp_env(&mut mcp, env)?;
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

    fn load(env: &MapEnv) -> anyhow::Result<ServerConfig> {
        ServerConfig::load(&SharedArgs::default(), env)
    }

    #[test]
    fn max_response_bytes_defaults_to_one_mib() {
        let config = ServerConfig::default();
        assert_eq!(config.mcp.max_response_bytes, 1024 * 1024);
    }

    #[test]
    fn max_response_bytes_env_beats_toml() {
        let (_directory, env) = env_with_toml("[mcp]\nmax_response_bytes = 2048\n");
        assert_eq!(load(&env).unwrap().mcp.max_response_bytes, 2048);
        let env = env.with("FONTGRES_MCP_MAX_RESPONSE_BYTES", "4096");
        assert_eq!(load(&env).unwrap().mcp.max_response_bytes, 4096);
    }

    #[test]
    fn invalid_max_response_bytes_env_fails_naming_variable_and_value() {
        let (_directory, env) = isolated_env();
        let env = env.with("FONTGRES_MCP_MAX_RESPONSE_BYTES", "lots");
        let err = load(&env).unwrap_err();
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
        let config = load(&env).unwrap();
        assert!(!config.core.fonts.system);
        assert_eq!(config.mcp.max_response_bytes, 10);
    }

    #[test]
    fn missing_explicit_config_file_is_an_error() {
        let directory = tempfile::tempdir().unwrap();
        let missing = directory.path().join("typo.toml");
        let args = SharedArgs {
            config: Some(missing.clone()),
            ..SharedArgs::default()
        };
        let err = ServerConfig::load(&args, &MapEnv::new()).unwrap_err();
        assert!(
            err.to_string()
                .contains(&format!("config file {} does not exist", missing.display())),
            "{err}"
        );
    }

    #[test]
    fn an_unknown_mcp_key_is_an_error_naming_it() {
        let (_directory, env) = env_with_toml("[mcp]\nmax_response_byts = 10\n");
        let err = load(&env).unwrap_err();
        let message = err.to_string();
        assert!(
            message.contains("line 2, column 1: unknown field `max_response_byts`"),
            "{message}"
        );
    }

    #[test]
    fn the_font_flags_reach_the_core_config() {
        let (_directory, env) = env_with_toml("[fonts]\ndirs = [\"/toml\"]\n");
        let args = SharedArgs {
            font_dirs: vec!["/flag".into()],
            no_system_fonts: true,
            ..SharedArgs::default()
        };
        let fonts = ServerConfig::load(&args, &env).unwrap().core.fonts;
        assert!(!fonts.system);
        assert_eq!(fonts.dirs, [std::path::Path::new("/flag")]);
    }

    /// Every setting the example file shows, commented out or not, is one
    /// the server reads.
    #[test]
    fn every_setting_in_the_example_config_file_loads() {
        let example = include_str!("../../../config.toml.example");
        let is_setting = |line: &str| {
            line.split_once(" = ").is_some_and(|(key, _)| {
                key.chars()
                    .all(|letter| letter.is_ascii_lowercase() || letter == '_')
            })
        };
        let settings: Vec<&str> = example
            .lines()
            .map(|line| match line.strip_prefix("# ") {
                Some(setting) if is_setting(setting) => setting,
                _ => line,
            })
            .collect();
        let (_directory, env) = env_with_toml(&settings.join("\n"));

        let config = load(&env).unwrap();
        let generic = &config.core.fonts.generic;
        assert_eq!(generic.sans_serif.as_deref(), Some("Arial"));
        assert_eq!(generic.monospace.as_deref(), Some("Courier New"));
        assert_eq!(
            config.core.log.level.map(|level| level.to_string()),
            Some("info".into())
        );
        assert_eq!(config.mcp.max_response_bytes, 1024 * 1024);
    }
}
