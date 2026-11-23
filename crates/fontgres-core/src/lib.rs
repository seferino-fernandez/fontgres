//! Font database and shared configuration for the fontgres binaries.

#![doc = include_str!("../README.md")]

/// Static shell completions and man pages for the workspace binaries.
#[cfg(feature = "cli")]
pub mod cli;
/// Configuration loading.
pub mod config;
/// Environment-variable source abstraction (testable).
pub mod env;
/// Error types.
pub mod error;

pub use config::{Config, ConfigOverrides, ConfigPathSource, FontsConfig, GenericFamilies};
pub use error::{ConfigError, Error};

/// Crate `Result` alias.
pub type Result<T> = std::result::Result<T, Error>;
