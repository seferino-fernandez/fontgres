//! Error types for the configuration and font loading the workspace binaries
//! share.
//!
//! Every fallible operation returns [`Error`](enum@Error) through the crate-level
//! [`Result`](crate::Result) alias.

use std::fmt;
use std::path::PathBuf;

use thiserror::Error;

/// Errors returned by configuration and font loading.
///
/// A variant either shows its inner error in its message or returns it from
/// [`std::error::Error::source`], never both, so a chain printer such as
/// anyhow's `{:#}` prints each cause once.
///
/// The enum is non-exhaustive, so callers should include a fallback match arm.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum Error {
    /// Configuration is invalid (bad value, missing file, etc.).
    #[error("configuration error: {0}")]
    Config(ConfigError),

    /// A configured font directory cannot be read: it does not exist, is not
    /// a directory, or may not be listed.
    #[error("cannot read font directory {}", .path.display())]
    FontDir {
        /// The directory that cannot be read.
        path: PathBuf,
        /// Why it cannot be read.
        #[source]
        source: std::io::Error,
    },

    /// A configured font file cannot be read.
    #[error("cannot read font file {}", .path.display())]
    FontFile {
        /// The file that cannot be read.
        path: PathBuf,
        /// Why it cannot be read.
        #[source]
        source: std::io::Error,
    },

    /// A configured font file was read and gave no face.
    #[error(
        "no font faces in {}{}",
        .path.display(),
        .reason.as_ref().map_or_default(|reason| format!(": {reason}"))
    )]
    NoFaces {
        /// The file that gave no face.
        path: PathBuf,
        /// Why its first face did not load; `None` when the file holds no
        /// face at all.
        reason: Option<crate::FontLoadFailureReason>,
    },
}

impl From<ConfigError> for Error {
    fn from(error: ConfigError) -> Self {
        Self::Config(error)
    }
}

/// An invalid configuration value or file.
///
/// # Examples
///
/// ```
/// use fontgres_core::ConfigError;
///
/// let error = ConfigError::InvalidEnvValue {
///     key: "FONTGRES_FONTS_SYSTEM".into(),
///     raw: "maybe".into(),
///     reason: "expected a boolean".into(),
/// };
/// assert_eq!(
///     error.to_string(),
///     r#"invalid FONTGRES_FONTS_SYSTEM value "maybe": expected a boolean"#
/// );
/// ```
#[derive(Debug, Clone, Error)]
#[non_exhaustive]
pub enum ConfigError {
    /// The file named by a path argument or `FONTGRES_CONFIG` does not exist.
    #[error(
        "config file {}{} does not exist",
        .path.display(),
        if *.set_by_env { " set by FONTGRES_CONFIG" } else { "" }
    )]
    FileNotFound {
        /// The path that does not exist.
        path: PathBuf,
        /// Whether `FONTGRES_CONFIG` named the path, rather than the caller.
        set_by_env: bool,
    },

    /// The configuration file exists and cannot be read.
    #[error("cannot read config file {}: {reason}", .path.display())]
    Unreadable {
        /// The file that cannot be read.
        path: PathBuf,
        /// Why it cannot be read, as the operating system says it.
        reason: String,
    },

    /// The configuration file is not valid TOML for the expected shape: a
    /// syntax error, an unknown key or table, or a value of the wrong kind.
    #[error(
        "invalid config file {}: {}",
        .path.display(),
        Located::new(*.line, *.column, .message)
    )]
    Parse {
        /// Path of the file that failed to parse.
        path: PathBuf,
        /// One-based line of the error, or zero when unknown.
        line: usize,
        /// One-based column of the error, or zero when unknown.
        column: usize,
        /// Parser message.
        message: String,
    },

    /// A `FONTGRES_*` environment variable holds a value that does not parse.
    #[error("invalid {key} value {raw:?}: {reason}")]
    InvalidEnvValue {
        /// Name of the variable.
        key: String,
        /// The value as set, before trimming.
        raw: String,
        /// Why the value does not parse.
        reason: String,
    },
}

/// Formats a parse message as `line {line}, column {column}: {message}`, or the
/// message alone when the line is zero (unknown).
pub(crate) struct Located<'a> {
    line: usize,
    column: usize,
    message: &'a str,
}

impl<'a> Located<'a> {
    /// Pairs `message` with its `line` and `column`; a zero line is unknown.
    pub(crate) const fn new(line: usize, column: usize, message: &'a str) -> Self {
        Self {
            line,
            column,
            message,
        }
    }
}

impl fmt::Display for Located<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.line > 0 {
            write!(f, "line {}, column {}: ", self.line, self.column)?;
        }
        f.write_str(self.message)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::error::Error as _;
    use std::io;

    /// Joins the `Display` of `error` and every error in its source chain.
    fn chain_text(error: &Error) -> String {
        let mut text = error.to_string();
        let mut source = error.source();
        while let Some(cause) = source {
            text.push_str(": ");
            text.push_str(&cause.to_string());
            source = cause.source();
        }
        text
    }

    #[test]
    fn chain_prints_each_cause_once() {
        let file = Error::FontFile {
            path: PathBuf::from("/fonts/a.ttf"),
            source: io::Error::new(io::ErrorKind::NotFound, "no such file"),
        };
        assert_eq!(chain_text(&file).matches("no such file").count(), 1);

        let config = Error::from(ConfigError::InvalidEnvValue {
            key: "FONTGRES_X".to_string(),
            raw: "soon".to_string(),
            reason: "invalid digit".to_string(),
        });
        assert_eq!(chain_text(&config).matches("invalid digit").count(), 1);
    }

    #[test]
    fn font_errors_name_the_path_and_keep_the_cause_for_the_chain() {
        let dir = Error::FontDir {
            path: PathBuf::from("/fonts/missing"),
            source: io::Error::new(io::ErrorKind::NotFound, "no such directory"),
        };
        assert_eq!(dir.to_string(), "cannot read font directory /fonts/missing");
        assert_eq!(
            chain_text(&dir),
            "cannot read font directory /fonts/missing: no such directory"
        );

        let path = PathBuf::from("/fonts/notes.ttf");
        let not_a_font = Error::NoFaces {
            path: path.clone(),
            reason: Some(crate::FontLoadFailureReason::MalformedFont),
        };
        assert_eq!(
            chain_text(&not_a_font),
            "no font faces in /fonts/notes.ttf: malformed font"
        );
        let empty = Error::NoFaces { path, reason: None };
        assert_eq!(chain_text(&empty), "no font faces in /fonts/notes.ttf");
    }

    #[test]
    fn config_errors_keep_their_messages() {
        let path = PathBuf::from("/tmp/typo.toml");
        for (error, expected) in [
            (
                ConfigError::FileNotFound {
                    path: path.clone(),
                    set_by_env: false,
                },
                "config file /tmp/typo.toml does not exist",
            ),
            (
                ConfigError::FileNotFound {
                    path: path.clone(),
                    set_by_env: true,
                },
                "config file /tmp/typo.toml set by FONTGRES_CONFIG does not exist",
            ),
            (
                ConfigError::Unreadable {
                    path: path.clone(),
                    reason: "Is a directory (os error 21)".to_string(),
                },
                "cannot read config file /tmp/typo.toml: Is a directory (os error 21)",
            ),
            (
                ConfigError::Parse {
                    path: path.clone(),
                    line: 2,
                    column: 5,
                    message: "boom".to_string(),
                },
                "invalid config file /tmp/typo.toml: line 2, column 5: boom",
            ),
            (
                ConfigError::Parse {
                    path,
                    line: 0,
                    column: 0,
                    message: "boom".to_string(),
                },
                "invalid config file /tmp/typo.toml: boom",
            ),
            (
                ConfigError::InvalidEnvValue {
                    key: "FONTGRES_X".to_string(),
                    raw: " soon".to_string(),
                    reason: "invalid digit".to_string(),
                },
                r#"invalid FONTGRES_X value " soon": invalid digit"#,
            ),
        ] {
            assert_eq!(error.to_string(), expected);
        }
    }
}
