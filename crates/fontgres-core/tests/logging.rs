//! Starting logging installs the process's one `tracing` subscriber, so
//! this file holds a single test and nothing else may start logging here.

#![cfg(feature = "cli")]

use fontgres_core::cli::{LevelFilter, Logging, SharedArgs};
use fontgres_core::config::LogConfig;
use fontgres_core::env::MapEnv;
use tracing::Level;

#[test]
fn logging_starts_for_the_named_targets_at_the_level_in_force() {
    let logging = Logging {
        default_level: LevelFilter::ERROR,
        targets: &["fontgres_core"],
    };
    let config = LogConfig {
        level: Some(LevelFilter::WARN),
    };
    assert!(!tracing::enabled!(target: "fontgres_core", Level::ERROR));

    SharedArgs::default().start_logging(&logging, config, &MapEnv::new());

    assert!(tracing::enabled!(target: "fontgres_core", Level::WARN));
    assert!(!tracing::enabled!(target: "fontgres_core", Level::INFO));
    assert!(!tracing::enabled!(target: "another_crate", Level::ERROR));
}
