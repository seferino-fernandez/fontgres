//! Loads the configuration the `fontgres` binaries share, from layers in
//! precedence order: CLI flags, `FONTGRES_*` environment variables,
//! `config.toml`, then defaults.
//!
//! CLI flags arrive as [`ConfigOverrides`], the top layer, so the loader
//! applies every layer itself. A list setting (font directories or files) is
//! taken whole from the highest layer that sets it; layers are never merged.

use crate::Result;
use crate::env::{Env, SystemEnv, env_non_empty, env_parse_bool};
use crate::error::ConfigError;
use serde::Deserialize;
use serde::de::DeserializeOwned;
use std::path::{Path, PathBuf};
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
#[derive(Debug, Clone, Default)]
pub struct LogConfig {
    /// Level filter: trace|debug|info|warn|error|off. `None` when neither
    /// `FONTGRES_LOG_LEVEL` nor `[log] level` sets one, so each binary applies
    /// its own default (see [`LogConfig::resolve_level`]).
    pub level: Option<String>,
}

impl LogConfig {
    /// Resolves the tracing level a binary starts with; `RUST_LOG` is applied
    /// separately and still overrides the result.
    ///
    /// An explicit `-v`/`-q` level (`flag`) wins over the configured
    /// [`level`](Self::level), which wins over `default`. An unparsable
    /// configured level falls back to `default` and also returns a warning for
    /// the caller to print, since logging is not set up yet.
    ///
    /// # Examples
    ///
    /// ```
    /// use fontgres_core::config::LogConfig;
    /// use tracing::level_filters::LevelFilter;
    ///
    /// let log = LogConfig { level: Some("debug".into()) };
    /// assert_eq!(log.resolve_level(None, LevelFilter::INFO), (LevelFilter::DEBUG, None));
    /// assert_eq!(
    ///     log.resolve_level(Some(LevelFilter::OFF), LevelFilter::INFO),
    ///     (LevelFilter::OFF, None),
    /// );
    ///
    /// let (level, warning) =
    ///     LogConfig { level: Some("loud".into()) }.resolve_level(None, LevelFilter::ERROR);
    /// assert_eq!(level, LevelFilter::ERROR);
    /// assert!(warning.unwrap().contains("\"loud\""));
    /// ```
    pub fn resolve_level(
        &self,
        flag: Option<LevelFilter>,
        default: LevelFilter,
    ) -> (LevelFilter, Option<String>) {
        if let Some(level) = flag {
            return (level, None);
        }
        let Some(value) = &self.level else {
            return (default, None);
        };
        match value.trim().parse() {
            Ok(level) => (level, None),
            Err(_) => (
                default,
                Some(format!(
                    "ignoring invalid log level {value:?} (expected trace, debug, info, \
                     warn, error or off); using {default}"
                )),
            ),
        }
    }
}

/// Which fonts the binaries load into a font database.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FontsConfig {
    /// Whether to load the fonts installed on the system. Defaults to `true`.
    pub system: bool,
    /// Directories to scan recursively for font files.
    pub dirs: Vec<PathBuf>,
    /// Individual font files to load.
    pub files: Vec<PathBuf>,
    /// Families that CSS generic family names resolve to.
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

/// The family each CSS generic family name resolves to. `None` keeps the
/// database's built-in default.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GenericFamilies {
    /// Family used for `serif`.
    pub serif: Option<String>,
    /// Family used for `sans-serif`.
    pub sans_serif: Option<String>,
    /// Family used for `cursive`.
    pub cursive: Option<String>,
    /// Family used for `fantasy`.
    pub fantasy: Option<String>,
    /// Family used for `monospace`.
    pub monospace: Option<String>,
}

/// Explicit values that sit above every other layer, typically CLI flags.
///
/// A `Some` value or a non-empty list wins over `FONTGRES_*` env vars and
/// `config.toml`.
///
/// # Examples
///
/// ```
/// use std::path::PathBuf;
/// use fontgres_core::env::MapEnv;
/// use fontgres_core::{Config, ConfigOverrides};
///
/// let overrides = ConfigOverrides {
///     system: Some(false),
///     dirs: vec![PathBuf::from("/opt/fonts")],
///     ..ConfigOverrides::default()
/// };
/// let config_file = tempfile::NamedTempFile::new()?;
/// let env = MapEnv::new()
///     .with("FONTGRES_CONFIG", config_file.path().to_string_lossy())
///     .with("FONTGRES_FONTS_SYSTEM", "true");
/// let config = Config::load_with_env(None, &overrides, &env)?;
/// assert!(!config.fonts.system);
/// assert_eq!(config.fonts.dirs, [PathBuf::from("/opt/fonts")]);
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
#[derive(Debug, Default, Clone)]
pub struct ConfigOverrides {
    /// Overrides whether system fonts load (`--no-system-fonts` sets `false`).
    pub system: Option<bool>,
    /// Overrides the font directories (`--font-dir`) when non-empty.
    pub dirs: Vec<PathBuf>,
    /// Overrides the font files (`--font-file`) when non-empty.
    pub files: Vec<PathBuf>,
}

/// Identifies which layer named the configuration file path.
///
/// A file named by [`ConfigPathSource::Explicit`] or [`ConfigPathSource::Env`]
/// must exist. A [`ConfigPathSource::Default`] location means there is no
/// configuration file unless [`Path::exists`] confirms one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfigPathSource {
    /// The caller passed the path, typically from a `--config` flag.
    Explicit,
    /// `FONTGRES_CONFIG` named the path.
    Env,
    /// The path is a default location: the platform configuration directory or
    /// `~/.config/fontgres/config.toml`.
    Default,
}

/// On-disk TOML shape. All fields optional; missing fields use defaults.
#[derive(Debug, Default, Deserialize)]
struct ConfigFile {
    #[serde(default)]
    log: LogFile,
    #[serde(default)]
    fonts: FontsFile,
}

#[derive(Debug, Default, Deserialize)]
struct LogFile {
    level: Option<String>,
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

/// Environment variables for each [`GenericFamilies`] field.
const GENERIC_FAMILY_ENV: [&str; 5] = [
    "FONTGRES_FONTS_GENERIC_SERIF",
    "FONTGRES_FONTS_GENERIC_SANS_SERIF",
    "FONTGRES_FONTS_GENERIC_CURSIVE",
    "FONTGRES_FONTS_GENERIC_FANTASY",
    "FONTGRES_FONTS_GENERIC_MONOSPACE",
];

impl GenericFamilies {
    /// Mutable access to each field, in [`GENERIC_FAMILY_ENV`] order.
    fn fields_mut(&mut self) -> [&mut Option<String>; 5] {
        [
            &mut self.serif,
            &mut self.sans_serif,
            &mut self.cursive,
            &mut self.fantasy,
            &mut self.monospace,
        ]
    }
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
    /// Loads configuration from `overrides`, the process environment, and the
    /// file at `path` (or the default location).
    ///
    /// A file named by `path` or `FONTGRES_CONFIG` must exist; a missing file at a
    /// default location means defaults.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Config`](crate::Error::Config) holding a
    /// [`ConfigError`]: [`FileNotFound`](ConfigError::FileNotFound) when the
    /// file named by `path` or `FONTGRES_CONFIG` does not exist,
    /// [`Parse`](ConfigError::Parse) when the TOML does not parse, or
    /// [`InvalidEnvValue`](ConfigError::InvalidEnvValue) when a `FONTGRES_*`
    /// variable holds a value that does not parse. Returns
    /// [`Error::Io`](crate::Error::Io) when the configuration file cannot be
    /// read.
    pub fn load(path: Option<&Path>, overrides: &ConfigOverrides) -> Result<Self> {
        Self::load_with_env(path, overrides, &SystemEnv)
    }

    /// [`load`](Self::load) with an injectable [`Env`] for testing.
    ///
    /// # Errors
    ///
    /// As for [`load`](Self::load).
    pub fn load_with_env(
        path: Option<&Path>,
        overrides: &ConfigOverrides,
        env: &impl Env,
    ) -> Result<Self> {
        Self::load_resolved(Self::resolve_config_path(path, env), overrides, env)
    }

    /// Loads configuration from an already resolved configuration file path.
    fn load_resolved(
        resolved: Option<(PathBuf, ConfigPathSource)>,
        overrides: &ConfigOverrides,
        env: &impl Env,
    ) -> Result<Self> {
        let file: ConfigFile = parse_resolved_config(resolved)?;

        // TOML layer.
        let mut config = Config::default();
        config.log.level = file.log.level;
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
        if let Some(v) = env_non_empty(env, "FONTGRES_LOG_LEVEL") {
            config.log.level = Some(v);
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
        for (field, key) in config
            .fonts
            .generic
            .fields_mut()
            .into_iter()
            .zip(GENERIC_FAMILY_ENV)
        {
            if let Some(v) = env_non_empty(env, key) {
                *field = Some(v);
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

        Ok(config)
    }

    /// Returns the configuration file path and the layer that named it.
    ///
    /// The first of these wins: `path`, the `FONTGRES_CONFIG` variable,
    /// `fontgres/config.toml` in the platform configuration directory when the
    /// file exists there, and `~/.config/fontgres/config.toml`. Returns `None`
    /// only when neither `path` nor the variable is set and the home directory
    /// cannot be determined.
    ///
    /// # Examples
    ///
    /// ```
    /// use std::path::{Path, PathBuf};
    /// use fontgres_core::env::MapEnv;
    /// use fontgres_core::{Config, ConfigPathSource};
    ///
    /// let env = MapEnv::new().with("FONTGRES_CONFIG", "/tmp/fontgres.toml");
    /// assert_eq!(
    ///     Config::resolve_config_path(None, &env),
    ///     Some((PathBuf::from("/tmp/fontgres.toml"), ConfigPathSource::Env)),
    /// );
    /// assert_eq!(
    ///     Config::resolve_config_path(Some(Path::new("explicit.toml")), &env),
    ///     Some((PathBuf::from("explicit.toml"), ConfigPathSource::Explicit)),
    /// );
    /// ```
    pub fn resolve_config_path(
        path: Option<&Path>,
        env: &impl Env,
    ) -> Option<(PathBuf, ConfigPathSource)> {
        if let Some(p) = path {
            return Some((p.to_path_buf(), ConfigPathSource::Explicit));
        }
        if let Some(p) = env_non_empty(env, "FONTGRES_CONFIG") {
            return Some((PathBuf::from(p), ConfigPathSource::Env));
        }
        let platform_path = dirs::config_dir().map(|dir| dir.join("fontgres").join("config.toml"));
        let default_path = match platform_path {
            Some(p) if p.exists() => Some(p),
            _ => {
                dirs::home_dir().map(|dir| dir.join(".config").join("fontgres").join("config.toml"))
            }
        };
        default_path.map(|p| (p, ConfigPathSource::Default))
    }

    /// Reads the configuration file that [`resolve_config_path`] selects and
    /// deserializes it as `T`, or returns `T::default()` when a default location
    /// holds no file.
    ///
    /// Lets a binary read tables of its own from the same file, under the same
    /// missing-file and error rules as [`load`](Self::load).
    ///
    /// [`resolve_config_path`]: Self::resolve_config_path
    ///
    /// # Errors
    ///
    /// Returns [`Error::Config`](crate::Error::Config) naming the path, holding
    /// [`ConfigError::FileNotFound`] when the file named by `path` or
    /// `FONTGRES_CONFIG` does not exist, or [`ConfigError::Parse`] when the TOML
    /// does not parse as `T` (reporting line and column, never the source
    /// line). Returns [`Error::Io`](crate::Error::Io) when the file exists but
    /// cannot be read.
    pub fn read_config_file<T: DeserializeOwned + Default>(
        path: Option<&Path>,
        env: &impl Env,
    ) -> Result<T> {
        parse_resolved_config(Self::resolve_config_path(path, env))
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
/// path is a default location.
fn read_resolved_config(
    resolved: Option<(PathBuf, ConfigPathSource)>,
) -> Result<Option<(PathBuf, String)>> {
    let Some((path, source)) = resolved else {
        return Ok(None);
    };
    let set_by_env = match source {
        ConfigPathSource::Default if !path.exists() => return Ok(None),
        ConfigPathSource::Default => {
            let text = std::fs::read_to_string(&path)?;
            return Ok(Some((path, text)));
        }
        ConfigPathSource::Explicit => false,
        ConfigPathSource::Env => true,
    };
    match std::fs::read_to_string(&path) {
        Ok(text) => Ok(Some((path, text))),
        // A path under a regular file (ENOTDIR) does not exist either.
        Err(e)
            if matches!(
                e.kind(),
                std::io::ErrorKind::NotFound | std::io::ErrorKind::NotADirectory
            ) =>
        {
            Err(ConfigError::FileNotFound { path, set_by_env }.into())
        }
        Err(e) => Err(e.into()),
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
        Config::load_with_env(Some(path), overrides, env)
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
        assert_eq!(config.log.level.as_deref(), Some("warn"));
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
        assert_eq!(config.log.level.as_deref(), Some("debug"));
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
        let (_directory, path) = config_file("[fonts]\nunknown_key = false\n");
        let err = load(&path, &ConfigOverrides::default(), &MapEnv::new()).unwrap_err();
        let message = err.to_string();
        assert!(
            matches!(err, Error::Config(ConfigError::Parse { .. })),
            "{err:?}"
        );
        assert!(message.contains("unknown_key"), "{message}");
    }

    #[test]
    fn missing_default_config_file_loads_defaults() {
        // The default location is injected so the test never reads the real home config.
        let directory = tempfile::tempdir().unwrap();
        let missing = directory.path().join("config.toml");
        let config = Config::load_resolved(
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
        let err = Config::load_with_env(None, &ConfigOverrides::default(), &env).unwrap_err();
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
    fn unreadable_explicit_config_file_keeps_io_error() {
        let directory = tempfile::tempdir().unwrap();
        let err = load(
            directory.path(),
            &ConfigOverrides::default(),
            &MapEnv::new(),
        )
        .unwrap_err();
        assert!(matches!(err, Error::Io(_)), "got {err:?}");
    }

    #[test]
    fn describe_toml_error_without_a_span_is_the_message_alone() {
        use serde::de::Error as _;

        let err = toml::de::Error::custom("boom");
        assert_eq!(describe_toml_error("system = true", &err), "boom");
    }

    #[test]
    fn resolve_level_flag_beats_config_beats_default() {
        let unset = LogConfig::default();
        let configured = LogConfig {
            level: Some("debug".into()),
        };
        assert_eq!(
            unset.resolve_level(None, LevelFilter::INFO),
            (LevelFilter::INFO, None)
        );
        assert_eq!(
            unset.resolve_level(None, LevelFilter::ERROR),
            (LevelFilter::ERROR, None)
        );
        assert_eq!(
            configured.resolve_level(None, LevelFilter::INFO),
            (LevelFilter::DEBUG, None)
        );
        assert_eq!(
            configured.resolve_level(Some(LevelFilter::WARN), LevelFilter::INFO),
            (LevelFilter::WARN, None)
        );
    }

    #[test]
    fn resolve_level_warns_and_falls_back_on_an_invalid_value() {
        let log = LogConfig {
            level: Some("loud".into()),
        };
        let (level, warning) = log.resolve_level(None, LevelFilter::INFO);
        assert_eq!(level, LevelFilter::INFO);
        assert_eq!(
            warning.as_deref(),
            Some(
                "ignoring invalid log level \"loud\" (expected trace, debug, info, \
                 warn, error or off); using info"
            )
        );
    }
}
