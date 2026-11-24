//! What the workspace binaries do the same way when they start: the flags
//! they share, the `completions` and `man` subcommands, loading the
//! configuration those flags select, and starting logging.
//!
//! A binary flattens [`SharedArgs`] and [`SetupCommand`] into its own
//! parser, parses with [`parse`], and then either runs a
//! [`SetupCommand`], which reads no configuration, or loads its
//! configuration with [`SharedArgs::load_config`] and starts logging with
//! [`SharedArgs::start_logging`].

use std::io;
use std::path::{Path, PathBuf};

use clap::{Command, CommandFactory, Parser, Subcommand, ValueEnum};
use clap_complete::{CompleteEnv, Shell, generate};
use clap_complete_nushell::Nushell;
use clap_verbosity_flag::Verbosity;
use serde::de::DeserializeOwned;
pub use tracing::level_filters::LevelFilter;
use tracing_subscriber::EnvFilter;

use crate::Result;
use crate::config::{Config, ConfigOverrides, LogConfig};
use crate::env::Env;

/// Parses the process's command line as `A`.
///
/// It first answers a dynamic completion request, `COMPLETE=<shell> <binary>`,
/// which prints to stdout and exits the process, so call it before anything
/// else writes there. On a command line that does not parse, and on `--help`
/// and `--version`, it prints and exits as [`Parser::parse`] does.
pub fn parse<A: Parser>() -> A {
    CompleteEnv::with_factory(A::command).complete();
    A::parse()
}

/// The flags every workspace binary takes: which configuration file and
/// fonts to load, and how much to log. They are global, so they are accepted
/// before or after a subcommand.
// Without `about = None`, clap would take the doc comment above as the
// description of the binary that flattens these flags.
#[derive(Debug, Clone, Default, clap::Args)]
#[command(about = None, long_about = None)]
pub struct SharedArgs {
    #[expect(
        clippy::doc_markdown,
        reason = "clap renders this doc comment verbatim as --help text, so backticks would show"
    )]
    /// Path to config.toml [env: FONTGRES_CONFIG]. A path named here or by
    /// FONTGRES_CONFIG must exist.
    #[arg(long, global = true, value_name = "PATH")]
    pub config: Option<PathBuf>,

    /// Load fonts from this directory, scanned recursively (repeatable).
    /// Replaces `[fonts] dirs` and `FONTGRES_FONTS_DIRS`.
    #[arg(long = "font-dir", global = true, value_name = "DIR")]
    pub font_dirs: Vec<PathBuf>,

    /// Load this font file (repeatable). Replaces `[fonts] files` and
    /// `FONTGRES_FONTS_FILES`.
    #[arg(long = "font-file", global = true, value_name = "FILE")]
    pub font_files: Vec<PathBuf>,

    /// Do not load the fonts installed on this system.
    #[arg(long, global = true)]
    pub no_system_fonts: bool,

    /// Logging verbosity: `-v` warn, `-vv` info, `-vvv` debug, `-vvvv` trace;
    /// `-q` silences output. When passed, it overrides `FONTGRES_LOG_LEVEL`
    /// and `[log] level`. `RUST_LOG` overrides everything.
    #[command(flatten)]
    pub verbosity: Verbosity,
}

/// How a binary logs: what it shows when nothing sets a level, and whose
/// records it shows.
#[derive(Debug, Clone, Copy)]
pub struct Logging<'a> {
    /// The level when neither `-v`/`-q`, `FONTGRES_LOG_LEVEL` nor
    /// `[log] level` sets one.
    pub default_level: LevelFilter,
    /// The `tracing` targets shown at the level in force, such as
    /// `fontgres_core`. Records of any other target are not shown.
    pub targets: &'a [&'a str],
}

impl SharedArgs {
    /// Loads the configuration these flags select: the file `--config`,
    /// `FONTGRES_CONFIG` or a default location names, then the `FONTGRES_*`
    /// variables read through `env`, then the font flags on top.
    ///
    /// `M` is what the file's `[mcp]` table is read as, and it is returned
    /// beside the shared settings. The MCP server passes its own settings; a
    /// binary with no use for the table passes
    /// [`UnreadTable`](crate::config::UnreadTable), which accepts whatever it
    /// holds. A key or table the file's shape does not name is an error.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Config`](crate::Error::Config) holding a
    /// [`ConfigError`](crate::ConfigError):
    /// [`FileNotFound`](crate::ConfigError::FileNotFound) when the file named
    /// by `--config` or `FONTGRES_CONFIG` does not exist,
    /// [`Unreadable`](crate::ConfigError::Unreadable) when the file cannot be
    /// read, [`Parse`](crate::ConfigError::Parse) when it is not valid TOML,
    /// names an unknown key or table, or holds an invalid value, and
    /// [`InvalidEnvValue`](crate::ConfigError::InvalidEnvValue) when a
    /// `FONTGRES_*` variable holds a value that does not parse.
    ///
    /// # Examples
    ///
    /// ```
    /// use std::path::PathBuf;
    /// use fontgres_core::cli::SharedArgs;
    /// use fontgres_core::config::UnreadTable;
    /// use fontgres_core::env::MapEnv;
    ///
    /// let config_file = tempfile::NamedTempFile::new()?;
    /// std::fs::write(&config_file, "[fonts]\ndirs = ['/usr/share/fonts']\n")?;
    /// let args = SharedArgs {
    ///     config: Some(config_file.path().to_path_buf()),
    ///     no_system_fonts: true,
    ///     ..SharedArgs::default()
    /// };
    /// let env = MapEnv::new().with("FONTGRES_FONTS_SYSTEM", "true");
    /// let (config, _) = args.load_config::<UnreadTable>(&env)?;
    /// assert!(!config.fonts.system);
    /// assert_eq!(config.fonts.dirs, [PathBuf::from("/usr/share/fonts")]);
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn load_config<M: DeserializeOwned + Default>(
        &self,
        env: &impl Env,
    ) -> Result<(Config, M)> {
        Config::load(self.config.as_deref(), &self.overrides(), env)
    }

    /// Starts logging to stderr for the rest of the process, showing the
    /// records of `logging`'s targets at the level in force: the `-v`/`-q`
    /// flags, else the level `config` holds, else `logging`'s default. A
    /// `RUST_LOG` variable, read through `env`, replaces all of that when it
    /// holds valid filter directives.
    ///
    /// # Panics
    ///
    /// Panics when the process already has a global `tracing` subscriber,
    /// so call it once.
    pub fn start_logging(&self, logging: &Logging<'_>, config: LogConfig, env: &impl Env) {
        tracing_subscriber::fmt()
            .with_env_filter(self.log_filter(logging, config, env))
            .with_writer(io::stderr)
            .init();
    }

    /// The filter logging starts with: `RUST_LOG`'s when it parses, else
    /// `logging`'s targets at the level in force.
    fn log_filter(&self, logging: &Logging<'_>, config: LogConfig, env: &impl Env) -> EnvFilter {
        env.get("RUST_LOG")
            .and_then(|directives| directives.parse().ok())
            .unwrap_or_else(|| EnvFilter::new(logging.filter(self.log_level(config, logging))))
    }

    /// The font flags as the configuration's top layer.
    fn overrides(&self) -> ConfigOverrides {
        ConfigOverrides {
            system: self.no_system_fonts.then_some(false),
            dirs: self.font_dirs.clone(),
            files: self.font_files.clone(),
        }
    }

    /// The level in force before `RUST_LOG`: the flags', else the
    /// configured one, else the binary's default.
    fn log_level(&self, config: LogConfig, logging: &Logging<'_>) -> LevelFilter {
        if self.verbosity.is_present() {
            self.verbosity.tracing_level_filter()
        } else {
            config.level.unwrap_or(logging.default_level)
        }
    }
}

impl Logging<'_> {
    /// The `tracing` filter that shows each target at `level`.
    fn filter(&self, level: LevelFilter) -> String {
        let directives: Vec<String> = self
            .targets
            .iter()
            .map(|target| format!("{target}={level}"))
            .collect();
        directives.join(",")
    }
}

/// Shells a static completion script can be generated for.
///
/// Extends `clap_complete`'s built-in [`Shell`] set with Nushell (via
/// `clap_complete_nushell`), which `clap_complete` does not ship.
#[derive(Clone, Copy, Debug, ValueEnum)]
pub enum CompletionShell {
    /// Bash.
    Bash,
    /// Elvish.
    Elvish,
    /// Fish.
    Fish,
    /// Nushell.
    Nushell,
    /// `PowerShell`.
    #[value(name = "powershell")]
    PowerShell,
    /// Zsh.
    Zsh,
}

/// The subcommands every workspace binary has for setting itself up in a
/// shell. Neither reads configuration or fonts, so a binary runs one before
/// it loads any.
#[derive(Debug, Clone, Subcommand)]
pub enum SetupCommand {
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
    /// Generate man pages for this command and all subcommands into <OUT_DIR>.
    Man {
        /// Directory to write the generated man pages into (created if missing).
        out_dir: PathBuf,
    },
}

impl SetupCommand {
    /// Runs the subcommand for the binary whose command line is `A`: prints
    /// the completion script to stdout, or writes the man pages.
    ///
    /// # Errors
    ///
    /// Returns an I/O error if the man page directory cannot be created or a
    /// page cannot be written.
    ///
    /// # Panics
    ///
    /// Panics if writing the completion script to stdout fails, for example
    /// when the reader closes the pipe early, and if `A` fails clap's debug
    /// assertions.
    pub fn run<A: CommandFactory>(&self) -> io::Result<()> {
        match self {
            Self::Completions { shell } => {
                write_completions(*shell, A::command());
                Ok(())
            }
            Self::Man { out_dir } => write_man_pages(A::command(), out_dir),
        }
    }
}

/// Writes a static shell completion script for `cmd` to stdout.
fn write_completions(shell: CompletionShell, mut cmd: Command) {
    let name = cmd.get_name().to_string();
    let mut out = io::stdout();
    match shell {
        CompletionShell::Bash => generate(Shell::Bash, &mut cmd, name, &mut out),
        CompletionShell::Elvish => generate(Shell::Elvish, &mut cmd, name, &mut out),
        CompletionShell::Fish => generate(Shell::Fish, &mut cmd, name, &mut out),
        CompletionShell::PowerShell => generate(Shell::PowerShell, &mut cmd, name, &mut out),
        CompletionShell::Zsh => generate(Shell::Zsh, &mut cmd, name, &mut out),
        CompletionShell::Nushell => generate(Nushell, &mut cmd, name, &mut out),
    }
}

/// Generates man pages for `cmd` and all subcommands into `out_dir`, creating
/// the directory if it does not exist.
fn write_man_pages(cmd: Command, out_dir: &Path) -> io::Result<()> {
    std::fs::create_dir_all(out_dir)?;
    clap_mangen::generate_to(cmd, out_dir)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A binary's command line with the shared flags and subcommands, as the
    /// workspace binaries flatten them.
    #[derive(Parser)]
    #[command(name = "example", about = "An example binary")]
    struct Example {
        #[command(subcommand)]
        command: Option<SetupCommand>,
        #[command(flatten)]
        shared: SharedArgs,
    }

    fn shared(args: &[&str]) -> SharedArgs {
        let argv = ["example"].iter().chain(args);
        Example::try_parse_from(argv).unwrap().shared
    }

    const LOGGING: Logging<'_> = Logging {
        default_level: LevelFilter::INFO,
        targets: &["fontgres_core", "example"],
    };

    #[test]
    fn the_shared_flags_leave_the_binary_its_own_description() {
        let command = Example::command();
        command.clone().debug_assert();
        assert_eq!(
            command.get_about().map(ToString::to_string).as_deref(),
            Some("An example binary")
        );
        assert!(command.get_long_about().is_none());
    }

    #[test]
    fn font_flags_become_the_top_config_layer() {
        let overrides = shared(&[
            "--no-system-fonts",
            "--font-dir",
            "/a",
            "--font-dir",
            "/b",
            "--font-file",
            "/c.ttf",
        ])
        .overrides();
        assert_eq!(overrides.system, Some(false));
        assert_eq!(overrides.dirs, [Path::new("/a"), Path::new("/b")]);
        assert_eq!(overrides.files, [Path::new("/c.ttf")]);

        let defaults = shared(&[]).overrides();
        assert_eq!(defaults.system, None);
        assert!(defaults.dirs.is_empty() && defaults.files.is_empty());
    }

    #[test]
    fn the_shared_flags_are_accepted_after_a_subcommand() {
        let parsed = Example::try_parse_from([
            "example",
            "completions",
            "bash",
            "--config",
            "/c.toml",
            "-v",
        ])
        .unwrap();
        assert!(matches!(
            parsed.command,
            Some(SetupCommand::Completions { .. })
        ));
        assert_eq!(parsed.shared.config.as_deref(), Some(Path::new("/c.toml")));
        assert!(parsed.shared.verbosity.is_present());
    }

    #[test]
    fn the_flag_level_beats_the_configured_level_which_beats_the_default() {
        let unset = LogConfig::default();
        let configured = LogConfig {
            level: Some(LevelFilter::DEBUG),
        };
        let level = |args: &[&str], config| shared(args).log_level(config, &LOGGING);
        assert_eq!(level(&[], unset), LevelFilter::INFO);
        assert_eq!(level(&[], configured), LevelFilter::DEBUG);
        assert_eq!(level(&["-v"], configured), LevelFilter::WARN);
        assert_eq!(level(&["-vv"], unset), LevelFilter::INFO);
        assert_eq!(level(&["-q"], configured), LevelFilter::OFF);

        let errors_only = Logging {
            default_level: LevelFilter::ERROR,
            ..LOGGING
        };
        assert_eq!(
            shared(&[]).log_level(unset, &errors_only),
            LevelFilter::ERROR
        );
    }

    #[test]
    fn rust_log_replaces_the_filter_when_it_parses() {
        use crate::env::MapEnv;

        let config = LogConfig::default();
        let filter = |env: &MapEnv| {
            shared(&["-v"])
                .log_filter(&LOGGING, config, env)
                .to_string()
        };
        let ours = "fontgres_core=warn,example=warn";
        assert_eq!(filter(&MapEnv::new()), ours);
        assert_eq!(
            filter(&MapEnv::new().with("RUST_LOG", "example=trace")),
            "example=trace"
        );
        assert_eq!(
            filter(&MapEnv::new().with("RUST_LOG", "example=loud")),
            ours
        );
    }

    #[test]
    fn the_flags_name_the_config_file_and_are_its_top_layer() {
        use crate::config::UnreadTable;
        use crate::env::MapEnv;

        let directory = tempfile::tempdir().unwrap();
        let file = directory.path().join("config.toml");
        std::fs::write(&file, "[fonts]\nsystem = true\ndirs = ['/toml']\n").unwrap();
        let args = shared(&["--config", file.to_str().unwrap(), "--no-system-fonts"]);
        let env = MapEnv::new().with("FONTGRES_FONTS_SYSTEM", "true");

        let (config, _) = args.load_config::<UnreadTable>(&env).unwrap();
        assert!(!config.fonts.system);
        assert_eq!(config.fonts.dirs, [Path::new("/toml")]);
    }

    #[test]
    fn the_filter_shows_each_target_at_the_level() {
        assert_eq!(
            LOGGING.filter(LevelFilter::WARN),
            "fontgres_core=warn,example=warn"
        );
        let one = Logging {
            targets: &["fontgres_core"],
            ..LOGGING
        };
        assert_eq!(one.filter(LevelFilter::OFF), "fontgres_core=off");
    }

    #[test]
    fn man_pages_are_written_for_the_command_and_its_subcommands() {
        let directory = tempfile::tempdir().unwrap();
        let out_dir = directory.path().join("man");
        let man = SetupCommand::Man {
            out_dir: out_dir.clone(),
        };
        man.run::<Example>().unwrap();
        for page in ["example.1", "example-completions.1", "example-man.1"] {
            assert!(out_dir.join(page).is_file(), "{page}");
        }
    }
}
