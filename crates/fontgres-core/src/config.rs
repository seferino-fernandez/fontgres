//! The configuration the `fontgres` binaries share, loaded from layers in
//! precedence order: CLI flags, `FONTGRES_*` environment variables,
//! `config.toml`, then defaults.
//!
//! [`SharedArgs::load_config`](crate::cli::SharedArgs::load_config) is the one
//! way in: the flags name the file and are the top layer, so the loader
//! applies every layer itself. A list setting (font directories or files) is
//! taken whole from the highest layer that sets it; layers are never merged.
//!
//! The file is read strictly: a key or table it does not expect is an error
//! naming it, so a misspelt setting never silently does nothing. Its tables
//! are `[log]`, `[fonts]` and `[mcp]`, which belongs to the MCP server.

use crate::Result;
use crate::env::{Env, env_non_empty, env_parse, env_parse_bool};
use crate::error::ConfigError;
use crate::{GenericFamilies, GenericFamily};
use serde::Deserialize;
use serde::de::DeserializeOwned;
use std::path::{Path, PathBuf};
use std::str::FromStr;
use tracing::level_filters::LevelFilter;

/// Top-level configuration shared by the workspace binaries.
#[derive(Debug, Clone, Default)]
pub struct Config {
    /// Logging settings.
    pub log: LogConfig,
    /// Which fonts to load into the database.
    pub fonts: FontsConfig,
}

/// Logging configuration.
#[derive(Debug, Clone, Copy, Default)]
pub struct LogConfig {
    /// The level `FONTGRES_LOG_LEVEL` or `[log] level` sets: trace, debug,
    /// info, warn, error or off. `None` when neither sets one, so each binary
    /// applies its own default.
    pub level: Option<LevelFilter>,
}

/// What a binary with no use for the `[mcp]` table reads it as: whatever
/// the table holds is accepted and dropped.
pub type UnreadTable = serde::de::IgnoredAny;

/// A log level as `[log] level` and `FONTGRES_LOG_LEVEL` spell it.
struct Level(LevelFilter);

impl FromStr for Level {
    type Err = &'static str;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let level = value.trim().parse().ok();
        level
            .map(Self)
            .ok_or("expected trace, debug, info, warn, error or off")
    }
}

impl<'de> Deserialize<'de> for Level {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let raw = String::deserialize(deserializer)?;
        raw.parse().map_err(|reason| {
            serde::de::Error::custom(format!("invalid log level {raw:?}: {reason}"))
        })
    }
}

/// Which fonts the binaries load into a font database.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FontsConfig {
    /// Whether to load the fonts installed on the system. Defaults to `true`.
    pub system: bool,
    /// Directories to scan recursively for font files.
    pub dirs: Vec<PathBuf>,
    /// Individual font files to load. Each must hold a font, unless an
    /// earlier directory or file already loaded it.
    pub files: Vec<PathBuf>,
    /// The family each CSS generic family name resolves to. `None` keeps the
    /// database's built-in default.
    pub generic: GenericFamilies,
}

impl Default for FontsConfig {
    fn default() -> Self {
        Self {
            system: true,
            dirs: Vec::new(),
            files: Vec::new(),
            generic: GenericFamilies::default(),
        }
    }
}

/// Explicit values that sit above every other layer: the CLI flags.
///
/// A `Some` value or a non-empty list wins over `FONTGRES_*` env vars and
/// `config.toml`.
#[derive(Debug, Default, Clone)]
pub(crate) struct ConfigOverrides {
    /// Overrides whether system fonts load (`--no-system-fonts` sets `false`).
    pub(crate) system: Option<bool>,
    /// Overrides the font directories (`--font-dir`) when non-empty.
    pub(crate) dirs: Vec<PathBuf>,
    /// Overrides the font files (`--font-file`) when non-empty.
    pub(crate) files: Vec<PathBuf>,
}

/// Identifies which layer named the configuration file path.
///
/// A file named by [`ConfigPathSource::Explicit`] or [`ConfigPathSource::Env`]
/// must exist. A [`ConfigPathSource::Default`] location means there is no
/// configuration file unless [`Path::exists`] confirms one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ConfigPathSource {
    /// The caller passed the path, from the `--config` flag.
    Explicit,
    /// `FONTGRES_CONFIG` named the path.
    Env,
    /// The path is a default location: the platform configuration directory or
    /// `~/.config/fontgres/config.toml`.
    Default,
}

/// On-disk TOML shape. All fields optional; missing fields use defaults.
/// `M` is what the `[mcp]` table is read as.
#[derive(Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct ConfigFile<M> {
    #[serde(default)]
    log: LogFile,
    #[serde(default)]
    fonts: FontsFile,
    #[serde(default)]
    mcp: M,
}

#[derive(Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct LogFile {
    level: Option<Level>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct FontsFile {
    system: Option<bool>,
    dirs: Option<Vec<PathBuf>>,
    files: Option<Vec<PathBuf>>,
    #[serde(default)]
    generic: GenericFamilies,
}

/// The environment variable that sets the family of `generic`:
/// `FONTGRES_FONTS_GENERIC_` and the CSS name in upper snake case, such as
/// `FONTGRES_FONTS_GENERIC_SANS_SERIF`.
fn generic_family_env(generic: GenericFamily) -> String {
    let name = generic.css_name().replace('-', "_").to_ascii_uppercase();
    format!("FONTGRES_FONTS_GENERIC_{name}")
}

/// Splits a path-list variable (`:`-separated on Unix, `;` on Windows) into
/// paths, dropping empty entries.
fn env_paths(env: &impl Env, key: &str) -> Option<Vec<PathBuf>> {
    let raw = env_non_empty(env, key)?;
    Some(
        std::env::split_paths(&raw)
            .filter(|path| !path.as_os_str().is_empty())
            .collect(),
    )
}

impl Config {
    /// Loads the configuration from `overrides`, the `FONTGRES_*` variables
    /// read through `env`, and the file at `path` or, without one, the file
    /// [`resolve_config_path`] finds. Returns it with the file's `[mcp]`
    /// table read as `M`.
    ///
    /// A file named by `path` or `FONTGRES_CONFIG` must exist; a missing file
    /// at a default location means defaults.
    pub(crate) fn load<M: DeserializeOwned + Default>(
        path: Option<&Path>,
        overrides: &ConfigOverrides,
        env: &impl Env,
    ) -> Result<(Self, M)> {
        Self::load_resolved(resolve_config_path(path, env), overrides, env)
    }

    /// Loads configuration from an already resolved configuration file path.
    fn load_resolved<M: DeserializeOwned + Default>(
        resolved: Option<(PathBuf, ConfigPathSource)>,
        overrides: &ConfigOverrides,
        env: &impl Env,
    ) -> Result<(Self, M)> {
        let file: ConfigFile<M> = parse_resolved_config(resolved)?;

        // TOML layer.
        let mut config = Config::default();
        config.log.level = file.log.level.map(|level| level.0);
        if let Some(v) = file.fonts.system {
            config.fonts.system = v;
        }
        if let Some(v) = file.fonts.dirs {
            config.fonts.dirs = v;
        }
        if let Some(v) = file.fonts.files {
            config.fonts.files = v;
        }
        config.fonts.generic = file.fonts.generic;

        // Env layer.
        if let Some(level) = env_parse::<Level>(env, "FONTGRES_LOG_LEVEL")? {
            config.log.level = Some(level.0);
        }
        if let Some(v) = env_parse_bool(env, "FONTGRES_FONTS_SYSTEM")? {
            config.fonts.system = v;
        }
        if let Some(v) = env_paths(env, "FONTGRES_FONTS_DIRS") {
            config.fonts.dirs = v;
        }
        if let Some(v) = env_paths(env, "FONTGRES_FONTS_FILES") {
            config.fonts.files = v;
        }
        for generic in GenericFamily::ALL {
            if let Some(v) = env_non_empty(env, &generic_family_env(generic)) {
                *config.fonts.generic.get_mut(generic) = Some(v);
            }
        }

        // CLI layer (highest precedence).
        if let Some(v) = overrides.system {
            config.fonts.system = v;
        }
        if !overrides.dirs.is_empty() {
            config.fonts.dirs.clone_from(&overrides.dirs);
        }
        if !overrides.files.is_empty() {
            config.fonts.files.clone_from(&overrides.files);
        }

        Ok((config, file.mcp))
    }
}

/// Returns the configuration file path and the layer that named it.
///
/// The first of these wins: `path`, the `FONTGRES_CONFIG` variable,
/// `fontgres/config.toml` in the platform configuration directory when the
/// file exists there, and `~/.config/fontgres/config.toml`. Returns `None`
/// only when neither `path` nor the variable is set and the home directory
/// cannot be determined.
fn resolve_config_path(path: Option<&Path>, env: &impl Env) -> Option<(PathBuf, ConfigPathSource)> {
    if let Some(p) = path {
        return Some((p.to_path_buf(), ConfigPathSource::Explicit));
    }
    if let Some(p) = env_non_empty(env, "FONTGRES_CONFIG") {
        return Some((PathBuf::from(p), ConfigPathSource::Env));
    }
    default_config_path(dirs::config_dir(), dirs::home_dir())
        .map(|p| (p, ConfigPathSource::Default))
}

/// The default location of the configuration file: `fontgres/config.toml`
/// in the platform configuration directory when the file exists there, else
/// `.config/fontgres/config.toml` in the home directory, whether or not it
/// exists.
fn default_config_path(
    platform_dir: Option<PathBuf>,
    home_dir: Option<PathBuf>,
) -> Option<PathBuf> {
    let platform_path = platform_dir.map(|dir| dir.join("fontgres").join("config.toml"));
    match platform_path {
        Some(p) if p.exists() => Some(p),
        _ => home_dir.map(|dir| dir.join(".config").join("fontgres").join("config.toml")),
    }
}

/// Reads and parses a resolved configuration file, treating only a missing
/// default as absent.
fn parse_resolved_config<T: DeserializeOwned + Default>(
    resolved: Option<(PathBuf, ConfigPathSource)>,
) -> Result<T> {
    let Some((path, text)) = read_resolved_config(resolved)? else {
        return Ok(T::default());
    };
    toml::from_str(&text).map_err(|e| toml_parse_error(path, &text, &e).into())
}

/// Reads a resolved configuration file. A missing file is an error unless the
/// path is a default location, and so is a file that cannot be read.
fn read_resolved_config(
    resolved: Option<(PathBuf, ConfigPathSource)>,
) -> Result<Option<(PathBuf, String)>> {
    let Some((path, source)) = resolved else {
        return Ok(None);
    };
    let named = source != ConfigPathSource::Default;
    if !named && !path.exists() {
        return Ok(None);
    }
    match std::fs::read_to_string(&path) {
        Ok(text) => Ok(Some((path, text))),
        // A path under a regular file (ENOTDIR) does not exist either.
        Err(error)
            if named
                && matches!(
                    error.kind(),
                    std::io::ErrorKind::NotFound | std::io::ErrorKind::NotADirectory
                ) =>
        {
            let set_by_env = source == ConfigPathSource::Env;
            Err(ConfigError::FileNotFound { path, set_by_env }.into())
        }
        Err(error) => Err(ConfigError::Unreadable {
            path,
            reason: error.to_string(),
        }
        .into()),
    }
}

/// Builds the [`ConfigError::Parse`] for a TOML parse error in the file at
/// `path`, whose message reads `line {line}, column {column}: {message}`.
///
/// Line and column are 1-based and computed over `text`; when the error has
/// no span, both are zero and only the message is shown.
fn toml_parse_error(path: PathBuf, text: &str, error: &toml::de::Error) -> ConfigError {
    let message = error.message().to_string();
    let (line, column) = error.span().map_or((0, 0), |span| {
        let before = text.get(..span.start).unwrap_or(text);
        // `split` yields one more piece than there are newlines, so the count
        // is already the 1-based line number.
        let line = before.split('\n').count();
        let current_line = before.rsplit('\n').next().unwrap_or(before);
        (line, current_line.chars().count().saturating_add(1))
    });
    ConfigError::Parse {
        path,
        line,
        column,
        message,
    }
}

/// The part of [`toml_parse_error`]'s message after the path:
/// `line {line}, column {column}: {message}`, or the message alone without a
/// span.
#[cfg(test)]
fn describe_toml_error(text: &str, error: &toml::de::Error) -> String {
    let config_error = toml_parse_error(PathBuf::new(), text, error);
    if let ConfigError::Parse {
        line,
        column,
        message,
        ..
    } = &config_error
    {
        crate::error::Located::new(*line, *column, message).to_string()
    } else {
        config_error.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Error;
    use crate::env::MapEnv;

    /// Writes `contents` as `config.toml` in a new temp directory, returning
    /// both so the directory outlives the test body.
    fn config_file(contents: &str) -> (tempfile::TempDir, PathBuf) {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("config.toml");
        std::fs::write(&path, contents).unwrap();
        (directory, path)
    }

    fn load(path: &Path, overrides: &ConfigOverrides, env: &MapEnv) -> Result<Config> {
        Config::load::<UnreadTable>(Some(path), overrides, env).map(|(config, _)| config)
    }

    /// Loads the file at `path` with nothing above it, reading `[mcp]` as `M`.
    fn load_with_table<M: DeserializeOwned + Default>(path: &Path) -> Result<(Config, M)> {
        Config::load(Some(path), &ConfigOverrides::default(), &MapEnv::new())
    }

    /// Loads the file holding `contents` with nothing above it.
    fn load_file(contents: &str) -> Result<Config> {
        let (_directory, path) = config_file(contents);
        load(&path, &ConfigOverrides::default(), &MapEnv::new())
    }

    /// Asserts that `result` is a [`ConfigError::Parse`] and returns its
    /// message.
    fn parse_error_message(result: Result<Config>) -> String {
        match result {
            Err(Error::Config(error @ ConfigError::Parse { .. })) => error.to_string(),
            other => panic!("expected ConfigError::Parse, got {other:?}"),
        }
    }

    fn paths(items: &[&str]) -> Vec<PathBuf> {
        items.iter().map(PathBuf::from).collect()
    }

    /// Joins `items` with the platform's path-list separator.
    fn path_list(items: &[&str]) -> String {
        std::env::join_paths(items)
            .unwrap()
            .to_string_lossy()
            .into_owned()
    }

    #[test]
    fn defaults_load_system_fonts_only() {
        let fonts = Config::default().fonts;
        assert!(fonts.system);
        assert_eq!(fonts.dirs, Vec::<PathBuf>::new());
        assert_eq!(fonts.files, Vec::<PathBuf>::new());
        assert_eq!(fonts.generic, GenericFamilies::default());
    }

    #[test]
    fn toml_layer_applies_when_nothing_overrides_it() {
        let (_directory, path) = config_file(
            "[log]\nlevel = \"warn\"\n\
             [fonts]\nsystem = false\ndirs = [\"/a\", \"/b\"]\nfiles = [\"/c.ttf\"]\n\
             [fonts.generic]\nserif = \"Noto Serif\"\nmonospace = \"Iosevka\"\n",
        );
        let config = load(&path, &ConfigOverrides::default(), &MapEnv::new()).unwrap();
        assert_eq!(config.log.level, Some(LevelFilter::WARN));
        assert!(!config.fonts.system);
        assert_eq!(config.fonts.dirs, paths(&["/a", "/b"]));
        assert_eq!(config.fonts.files, paths(&["/c.ttf"]));
        assert_eq!(config.fonts.generic.serif.as_deref(), Some("Noto Serif"));
        assert_eq!(config.fonts.generic.monospace.as_deref(), Some("Iosevka"));
        assert_eq!(config.fonts.generic.sans_serif, None);
    }

    #[test]
    fn env_beats_toml() {
        let (_directory, path) = config_file(
            "[log]\nlevel = \"warn\"\n[fonts]\nsystem = false\ndirs = [\"/toml\"]\n\
             files = [\"/toml.ttf\"]\n[fonts.generic]\nserif = \"Toml Serif\"\n",
        );
        let env = MapEnv::new()
            .with("FONTGRES_LOG_LEVEL", "debug")
            .with("FONTGRES_FONTS_SYSTEM", " yes\n")
            .with("FONTGRES_FONTS_DIRS", path_list(&["/env/a", "/env/b"]))
            .with("FONTGRES_FONTS_FILES", path_list(&["/env.otf"]))
            .with("FONTGRES_FONTS_GENERIC_SERIF", "Env Serif")
            .with("FONTGRES_FONTS_GENERIC_SANS_SERIF", "Env Sans")
            .with("FONTGRES_FONTS_GENERIC_CURSIVE", "Env Cursive")
            .with("FONTGRES_FONTS_GENERIC_FANTASY", "Env Fantasy")
            .with("FONTGRES_FONTS_GENERIC_MONOSPACE", "Env Mono");
        let config = load(&path, &ConfigOverrides::default(), &env).unwrap();
        assert_eq!(config.log.level, Some(LevelFilter::DEBUG));
        assert!(config.fonts.system);
        assert_eq!(config.fonts.dirs, paths(&["/env/a", "/env/b"]));
        assert_eq!(config.fonts.files, paths(&["/env.otf"]));
        assert_eq!(
            config.fonts.generic,
            GenericFamilies {
                serif: Some("Env Serif".into()),
                sans_serif: Some("Env Sans".into()),
                cursive: Some("Env Cursive".into()),
                fantasy: Some("Env Fantasy".into()),
                monospace: Some("Env Mono".into()),
            }
        );
    }

    #[test]
    fn blank_env_keeps_toml_values() {
        let (_directory, path) = config_file(
            "[fonts]\nsystem = false\ndirs = [\"/toml\"]\n[fonts.generic]\nserif = \"S\"\n",
        );
        let env = MapEnv::new()
            .with("FONTGRES_FONTS_SYSTEM", " \t ")
            .with("FONTGRES_FONTS_DIRS", "")
            .with("FONTGRES_FONTS_GENERIC_SERIF", "");
        let config = load(&path, &ConfigOverrides::default(), &env).unwrap();
        assert!(!config.fonts.system);
        assert_eq!(config.fonts.dirs, paths(&["/toml"]));
        assert_eq!(config.fonts.generic.serif.as_deref(), Some("S"));
    }

    #[test]
    fn invalid_system_env_fails_naming_variable_and_value() {
        let (_directory, path) = config_file("");
        let env = MapEnv::new().with("FONTGRES_FONTS_SYSTEM", "maybe");
        let err = load(&path, &ConfigOverrides::default(), &env).unwrap_err();
        assert!(
            matches!(err, Error::Config(ConfigError::InvalidEnvValue { .. })),
            "{err:?}"
        );
        let message = err.to_string();
        assert!(
            message.contains(r#"invalid FONTGRES_FONTS_SYSTEM value "maybe""#),
            "{message}"
        );
    }

    #[test]
    fn overrides_beat_env_and_toml() {
        let (_directory, path) =
            config_file("[fonts]\nsystem = true\ndirs = [\"/toml\"]\nfiles = [\"/toml.ttf\"]\n");
        let env = MapEnv::new()
            .with("FONTGRES_FONTS_SYSTEM", "true")
            .with("FONTGRES_FONTS_DIRS", "/env")
            .with("FONTGRES_FONTS_FILES", "/env.ttf");
        let overrides = ConfigOverrides {
            system: Some(false),
            dirs: paths(&["/cli"]),
            files: paths(&["/cli.ttf"]),
        };
        let config = load(&path, &overrides, &env).unwrap();
        assert!(!config.fonts.system);
        assert_eq!(config.fonts.dirs, paths(&["/cli"]));
        assert_eq!(config.fonts.files, paths(&["/cli.ttf"]));
    }

    #[test]
    fn empty_override_lists_keep_lower_layers() {
        let (_directory, path) = config_file("[fonts]\ndirs = [\"/toml\"]\n");
        let config = load(&path, &ConfigOverrides::default(), &MapEnv::new()).unwrap();
        assert_eq!(config.fonts.dirs, paths(&["/toml"]));
        assert!(config.fonts.system);
    }

    #[test]
    fn unknown_fonts_key_is_a_parse_error() {
        let message = parse_error_message(load_file("[fonts]\nunknown_key = false\n"));
        assert!(message.contains("unknown_key"), "{message}");
    }

    #[test]
    fn a_key_or_table_the_file_does_not_expect_is_a_parse_error_naming_it() {
        for (contents, unknown, position) in [
            (
                "[log]\nlevels = \"debug\"\n",
                "levels",
                "line 2, column 1: ",
            ),
            ("[font]\nsystem = false\n", "font", "line 1, column 2: "),
            ("[fonts]\n\nbogus = 1\n", "bogus", "line 3, column 1: "),
            ("bogus = 1\n", "bogus", "line 1, column 1: "),
            (
                "[fonts.generic]\nsans = \"Arial\"\n",
                "sans",
                "line 2, column 1: ",
            ),
        ] {
            let message = parse_error_message(load_file(contents));
            assert!(
                message.contains(&format!("unknown field `{unknown}`")),
                "{contents:?}: {message}"
            );
            assert!(message.contains(position), "{contents:?}: {message}");
        }
    }

    /// What the MCP server reads its table as, in miniature.
    #[derive(Debug, Default, PartialEq, Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Table {
        limit: Option<u32>,
    }

    #[test]
    fn the_mcp_table_is_read_as_the_type_the_binary_names() {
        let (_directory, path) = config_file("[mcp]\nlimit = 7\n[fonts]\nsystem = false\n");

        let (config, table) = load_with_table::<Table>(&path).unwrap();
        assert_eq!(table, Table { limit: Some(7) });
        assert!(!config.fonts.system);
        // A binary that does not read the table accepts whatever it holds.
        let (unread, _) = load_with_table::<UnreadTable>(&path).unwrap();
        assert_eq!(unread.fonts, config.fonts);
    }

    #[test]
    fn the_mcp_table_is_as_strict_as_its_type() {
        let (_directory, path) = config_file("[mcp]\nlimits = 7\nnested = { a = [1, 2] }\n");

        let unread = load_with_table::<UnreadTable>(&path);
        assert!(unread.is_ok(), "{unread:?}");
        let typed = load_with_table::<Table>(&path);
        let message = parse_error_message(typed.map(|(config, _)| config));
        assert!(message.contains("unknown field `limits`"), "{message}");
    }

    #[test]
    fn a_file_without_the_mcp_table_gives_its_default() {
        let (_directory, path) = config_file("");
        let (_, table) = load_with_table::<Table>(&path).unwrap();
        assert_eq!(table, Table::default());
    }

    #[test]
    fn missing_default_config_file_loads_defaults() {
        // The default location is injected so the test never reads the real home config.
        let directory = tempfile::tempdir().unwrap();
        let missing = directory.path().join("config.toml");
        let (config, _): (Config, UnreadTable) = Config::load_resolved(
            Some((missing, ConfigPathSource::Default)),
            &ConfigOverrides::default(),
            &MapEnv::new(),
        )
        .unwrap();
        assert_eq!(config.fonts, FontsConfig::default());
        assert!(config.log.level.is_none());
    }

    #[test]
    fn toml_syntax_error_reports_path_and_position() {
        let (_directory, path) = config_file("[fonts]\nsystem = true junk\n");
        let err = load(&path, &ConfigOverrides::default(), &MapEnv::new()).unwrap_err();
        let message = err.to_string();
        assert!(message.contains(&path.display().to_string()), "{message}");
        assert!(message.contains("line 2, column "), "{message}");
    }

    #[test]
    fn unset_log_level_is_none() {
        let (_directory, path) = config_file("");
        let config = load(&path, &ConfigOverrides::default(), &MapEnv::new()).unwrap();
        assert!(config.log.level.is_none());
        assert!(Config::default().log.level.is_none());
    }

    /// Asserts that `err` is a [`ConfigError::FileNotFound`] and returns its
    /// message.
    fn file_not_found_message(err: Error) -> String {
        let Error::Config(config @ ConfigError::FileNotFound { .. }) = err else {
            panic!("expected ConfigError::FileNotFound, got {err:?}");
        };
        config.to_string()
    }

    #[test]
    fn missing_explicit_config_file_errors_with_path() {
        let directory = tempfile::tempdir().unwrap();
        let missing = directory.path().join("typo.toml");
        let err = load(&missing, &ConfigOverrides::default(), &MapEnv::new()).unwrap_err();
        let message = file_not_found_message(err);
        assert_eq!(
            message,
            format!("config file {} does not exist", missing.display())
        );
    }

    #[test]
    fn missing_env_config_file_errors_with_path_and_variable() {
        let directory = tempfile::tempdir().unwrap();
        let missing = directory.path().join("typo.toml");
        let env = MapEnv::new().with("FONTGRES_CONFIG", missing.to_string_lossy());
        let err = Config::load::<UnreadTable>(None, &ConfigOverrides::default(), &env).unwrap_err();
        let message = file_not_found_message(err);
        assert_eq!(
            message,
            format!(
                "config file {} set by FONTGRES_CONFIG does not exist",
                missing.display()
            )
        );
    }

    #[test]
    fn explicit_config_path_under_a_regular_file_errors_with_path() {
        let (_directory, path) = config_file("");
        let missing = path.join("x.toml");
        let err = load(&missing, &ConfigOverrides::default(), &MapEnv::new()).unwrap_err();
        let message = file_not_found_message(err);
        assert!(
            message.contains(&missing.display().to_string()),
            "{message}"
        );
    }

    #[test]
    fn an_unreadable_config_file_errors_naming_the_path() {
        // A directory can be opened and cannot be read as a file.
        let directory = tempfile::tempdir().unwrap();
        let named = load(
            directory.path(),
            &ConfigOverrides::default(),
            &MapEnv::new(),
        );
        let found: Result<(Config, UnreadTable)> = Config::load_resolved(
            Some((directory.path().to_path_buf(), ConfigPathSource::Default)),
            &ConfigOverrides::default(),
            &MapEnv::new(),
        );
        for err in [named.unwrap_err(), found.unwrap_err()] {
            assert!(
                matches!(err, Error::Config(ConfigError::Unreadable { .. })),
                "got {err:?}"
            );
            let message = err.to_string();
            let expected = format!("cannot read config file {}: ", directory.path().display());
            assert!(message.contains(&expected), "{message}");
        }
    }

    #[test]
    fn the_default_location_is_the_platform_file_when_it_exists_else_under_home() {
        let directory = tempfile::tempdir().unwrap();
        let platform = directory.path().join("platform");
        let home = directory.path().join("home");
        let default = || default_config_path(Some(platform.clone()), Some(home.clone()));
        let under_home = home.join(".config/fontgres/config.toml");

        assert_eq!(default(), Some(under_home.clone()));
        assert_eq!(
            default_config_path(None, Some(home.clone())),
            Some(under_home)
        );
        assert_eq!(default_config_path(Some(platform.clone()), None), None);

        let platform_file = platform.join("fontgres/config.toml");
        std::fs::create_dir_all(platform_file.parent().unwrap()).unwrap();
        std::fs::write(&platform_file, "").unwrap();
        assert_eq!(default(), Some(platform_file.clone()));
        assert_eq!(
            default_config_path(Some(platform), None),
            Some(platform_file)
        );
    }

    #[test]
    fn an_explicit_path_beats_the_variable_which_beats_the_default_locations() {
        let env = MapEnv::new().with("FONTGRES_CONFIG", "/tmp/fontgres.toml");
        assert_eq!(
            resolve_config_path(None, &env),
            Some((PathBuf::from("/tmp/fontgres.toml"), ConfigPathSource::Env)),
        );
        assert_eq!(
            resolve_config_path(Some(Path::new("explicit.toml")), &env),
            Some((PathBuf::from("explicit.toml"), ConfigPathSource::Explicit)),
        );
        // An empty variable names no file.
        let blank = MapEnv::new().with("FONTGRES_CONFIG", "");
        let default = resolve_config_path(None, &blank);
        assert!(
            default
                .as_ref()
                .is_none_or(|(_, source)| *source == ConfigPathSource::Default),
            "{default:?}"
        );
    }

    #[test]
    fn describe_toml_error_without_a_span_is_the_message_alone() {
        use serde::de::Error as _;

        let err = toml::de::Error::custom("boom");
        assert_eq!(describe_toml_error("system = true", &err), "boom");
    }

    #[test]
    fn log_levels_parse_in_any_case_and_trimmed() {
        for (raw, expected) in [
            ("trace", LevelFilter::TRACE),
            (" Debug\n", LevelFilter::DEBUG),
            ("INFO", LevelFilter::INFO),
            ("warn", LevelFilter::WARN),
            ("error", LevelFilter::ERROR),
            ("off", LevelFilter::OFF),
        ] {
            let from_file = load_file(&format!("[log]\nlevel = {raw:?}\n")).unwrap();
            assert_eq!(from_file.log.level, Some(expected), "{raw:?}");

            let (_directory, path) = config_file("[log]\nlevel = \"off\"\n");
            let env = MapEnv::new().with("FONTGRES_LOG_LEVEL", raw);
            let from_env = load(&path, &ConfigOverrides::default(), &env).unwrap();
            assert_eq!(from_env.log.level, Some(expected), "{raw:?}");
        }
    }

    #[test]
    fn an_invalid_log_level_in_the_file_is_a_parse_error_at_its_position() {
        let (_directory, path) = config_file("[log]\nlevel = \"loud\"\n");
        // A valid level above the file does not excuse the file's.
        let valid_above = MapEnv::new().with("FONTGRES_LOG_LEVEL", "debug");
        for env in [MapEnv::new(), valid_above] {
            let message = parse_error_message(load(&path, &ConfigOverrides::default(), &env));
            assert!(
                message.ends_with(
                    "line 2, column 9: invalid log level \"loud\": expected trace, debug, info, \
                     warn, error or off"
                ),
                "{message}"
            );
        }
    }

    #[test]
    fn an_invalid_log_level_variable_fails_naming_variable_and_value() {
        let (_directory, path) = config_file("[log]\nlevel = \"warn\"\n");
        let env = MapEnv::new().with("FONTGRES_LOG_LEVEL", "loud");
        let err = load(&path, &ConfigOverrides::default(), &env).unwrap_err();
        assert_eq!(
            err.to_string(),
            "configuration error: invalid FONTGRES_LOG_LEVEL value \"loud\": expected trace, \
             debug, info, warn, error or off"
        );
        // A blank variable is unset.
        let blank = MapEnv::new().with("FONTGRES_LOG_LEVEL", " ");
        let config = load(&path, &ConfigOverrides::default(), &blank).unwrap();
        assert_eq!(config.log.level, Some(LevelFilter::WARN));
    }
}
