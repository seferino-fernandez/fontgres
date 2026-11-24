//! Where each platform keeps its system fonts.
//!
//! A platform's rule reads the environment and the file system and returns
//! [`SystemFonts`]: the font directories to load, in load order, and the
//! fontconfig aliases that name the generic families. A rule loads no font.
//! [`find`] applies the rule of the platform the crate is built for, and
//! every rule also builds for tests, whatever the platform.
//!
//! A rule takes the root directory that the fixed paths it names sit under,
//! such as `etc/fonts`, so a test can lay them out in a temporary
//! directory. A path that comes from an environment variable or from inside
//! a fontconfig file is used as written.

use std::path::{Path, PathBuf};

use crate::env::Env;
use crate::generic_family::Alias;

/// The system fonts a platform's rule found.
#[derive(Default)]
pub(crate) struct SystemFonts {
    /// The font directories, in the order they load.
    pub(crate) dirs: Vec<PathBuf>,
    /// fontconfig's `<alias>` rules in configuration order. Only the
    /// fontconfig rule gives any.
    pub(crate) aliases: Vec<Alias>,
}

impl From<Vec<PathBuf>> for SystemFonts {
    fn from(dirs: Vec<PathBuf>) -> Self {
        Self {
            dirs,
            aliases: Vec::new(),
        }
    }
}

/// Finds the system fonts by the rule of the platform the crate is built
/// for, reading the environment through `env` and the rule's fixed paths
/// under `root`. A platform without a rule has no system fonts.
pub(crate) fn find(env: &impl Env, root: &Path) -> SystemFonts {
    #[cfg(target_os = "windows")]
    let found = {
        // The one fixed path Windows has names its drive, so no root applies.
        let _ = root;
        SystemFonts::from(windows(env))
    };

    #[cfg(any(target_os = "macos", target_os = "ios"))]
    let found = SystemFonts::from(apple(env, root));

    #[cfg(all(
        feature = "fontconfig",
        unix,
        not(any(target_os = "macos", target_os = "ios", target_os = "android"))
    ))]
    let found = {
        let bsd = cfg!(any(target_os = "freebsd", target_os = "dragonfly"));
        fontconfig::find(env, root, fontconfig::config_dirs(bsd))
    };

    #[cfg(all(
        not(feature = "fontconfig"),
        unix,
        not(any(target_os = "macos", target_os = "ios", target_os = "android"))
    ))]
    let found = SystemFonts::from(unix(env, root));

    #[cfg(not(any(
        target_os = "windows",
        target_os = "macos",
        target_os = "ios",
        all(unix, not(target_os = "android"))
    )))]
    let found = {
        let _ = (env, root);
        SystemFonts::default()
    };

    found
}

/// Windows: `%SYSTEMROOT%\Fonts`, or `C:\Windows\Fonts` when the variable
/// is unset, then the per-user font directories under `%USERPROFILE%`.
#[cfg(any(test, target_os = "windows"))]
fn windows(env: &impl Env) -> Vec<PathBuf> {
    let mut dirs = vec![match env.get("SYSTEMROOT") {
        Some(system_root) => Path::new(&system_root).join("Fonts"),
        None => PathBuf::from("C:\\Windows\\Fonts\\"),
    }];
    if let Some(profile) = env.get("USERPROFILE") {
        let profile = Path::new(&profile);
        dirs.push(profile.join("AppData\\Local\\Microsoft\\Windows\\Fonts"));
        dirs.push(profile.join("AppData\\Roaming\\Microsoft\\Windows\\Fonts"));
    }
    dirs
}

/// macOS and iOS: the system-wide directories, the downloadable fonts in
/// the order of their directory names, the network directory, then the
/// user's own.
#[cfg(any(test, target_os = "macos", target_os = "ios"))]
fn apple(env: &impl Env, root: &Path) -> Vec<PathBuf> {
    let mut dirs = vec![
        root.join("Library/Fonts"),
        root.join("System/Library/Fonts"),
    ];
    // Downloadable fonts; the directory's name varies between major macOS
    // releases.
    let mut downloads: Vec<_> = std::fs::read_dir(root.join("System/Library/AssetsV2"))
        .into_iter()
        .flatten()
        .flatten()
        .filter(|entry| {
            entry
                .file_name()
                .to_string_lossy()
                .starts_with("com_apple_MobileAsset_Font")
        })
        .map(|entry| entry.path())
        .collect();
    // The file system's order is not the same on every machine.
    downloads.sort_unstable();
    dirs.append(&mut downloads);
    dirs.push(root.join("Network/Library/Fonts"));
    if let Some(home) = env.get("HOME") {
        dirs.push(Path::new(&home).join("Library/Fonts"));
    }
    dirs
}

/// Other Unix systems, when fontconfig is not read or lists no directory:
/// the two system-wide directories, then the two under `$HOME`.
#[cfg(any(
    test,
    all(
        unix,
        not(any(target_os = "macos", target_os = "ios", target_os = "android"))
    )
))]
fn unix(env: &impl Env, root: &Path) -> Vec<PathBuf> {
    let mut dirs = vec![
        root.join("usr/share/fonts/"),
        root.join("usr/local/share/fonts/"),
    ];
    if let Some(home) = env.get("HOME") {
        let home = Path::new(&home);
        dirs.push(home.join(".fonts"));
        dirs.push(home.join(".local/share/fonts"));
    }
    dirs
}

/// The rule for the Unix systems that read fontconfig's configuration.
#[cfg(any(
    test,
    all(
        feature = "fontconfig",
        unix,
        not(any(target_os = "macos", target_os = "ios", target_os = "android"))
    )
))]
mod fontconfig {
    use std::path::{Path, PathBuf};

    use fontconfig_parser::FontConfig;

    use super::{Alias, Env, SystemFonts};

    /// The directories, under the root, that hold the system-wide
    /// configuration files `fonts.conf` and `local.conf`. On the BSDs that
    /// install fontconfig from ports, `bsd`, it also reads
    /// `/usr/local/etc/fonts`.
    pub(super) fn config_dirs(bsd: bool) -> &'static [&'static str] {
        if bsd {
            &["etc/fonts", "usr/local/etc/fonts"]
        } else {
            &["etc/fonts"]
        }
    }

    /// Reads fontconfig's configuration and returns the directories it lists
    /// and its aliases.
    ///
    /// The file `$FONTCONFIG_FILE` names is the whole configuration. Without
    /// it, the user's `fontconfig/fonts.conf` under `$XDG_CONFIG_HOME`, or
    /// under `$HOME/.config`, comes first, then for each of `config_dirs` its
    /// `local.conf`, read only when the user's file did not merge, and its
    /// `fonts.conf`. A configuration that lists no directory gives the
    /// directories of [`unix`](super::unix).
    ///
    /// The parser resolves the paths inside a configuration file itself, so
    /// a `~` or an `xdg` prefix there follows the process environment, not
    /// `env`.
    pub(super) fn find(env: &impl Env, root: &Path, config_dirs: &[&str]) -> SystemFonts {
        let mut fontconfig = FontConfig::default();
        let home = env.get("HOME");

        if let Some(config_file) = env.get("FONTCONFIG_FILE") {
            merge(&mut fontconfig, Path::new(&config_file));
        } else {
            // $XDG_CONFIG_HOME defaults to $HOME/.config:
            // https://specifications.freedesktop.org/basedir-spec/basedir-spec-latest.html
            let user_config_dir = env
                .get("XDG_CONFIG_HOME")
                .map(PathBuf::from)
                .or_else(|| home.as_ref().map(|home| Path::new(home).join(".config")));
            let read_local = match user_config_dir {
                Some(dir) => !merge(&mut fontconfig, &dir.join("fontconfig/fonts.conf")),
                None => true,
            };
            for dir in config_dirs {
                let dir = root.join(dir);
                if read_local {
                    merge(&mut fontconfig, &dir.join("local.conf"));
                }
                merge(&mut fontconfig, &dir.join("fonts.conf"));
            }
        }

        let dirs = if fontconfig.dirs.is_empty() {
            log::warn!("Fallback to loading from known font dir paths.");
            super::unix(env, root)
        } else {
            let home = home.as_deref().map(Path::new);
            fontconfig
                .dirs
                .into_iter()
                .filter_map(|dir| expand_home(dir.path, home))
                .collect()
        };
        let aliases = fontconfig
            .aliases
            .into_iter()
            .map(|alias| Alias {
                family: alias.alias,
                prefer: alias.prefer,
                accept: alias.accept,
                default: alias.default,
            })
            .collect();
        SystemFonts { dirs, aliases }
    }

    /// Returns `path` with a leading `~` replaced by `home`, or `None` when
    /// it has one and the home directory is unknown.
    pub(super) fn expand_home(path: PathBuf, home: Option<&Path>) -> Option<PathBuf> {
        match path.strip_prefix("~") {
            Ok(in_home) => Some(home?.join(in_home)),
            Err(_) => Some(path),
        }
    }

    /// Merges the configuration file at `path` into `fontconfig` and returns
    /// whether it merged. A file that is missing or does not parse is skipped
    /// and logged at debug level, since most of the files fontconfig looks
    /// for are optional.
    pub(super) fn merge(fontconfig: &mut FontConfig, path: &Path) -> bool {
        match fontconfig.merge_config(path) {
            Ok(()) => true,
            Err(error) => {
                log::debug!("Skipped fontconfig file '{}': {error}.", path.display());
                false
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::path::{MAIN_SEPARATOR, Path, PathBuf};

    use super::{SystemFonts, apple, fontconfig, unix, windows};
    use crate::env::MapEnv;
    use crate::{Database, Family, GenericFamily, Source};

    /// A temporary directory laid out as a machine: the root the rules read
    /// under at `root`, and beside it the user's directories and the font
    /// directories the configuration files list.
    struct Machine(tempfile::TempDir);

    impl Machine {
        fn new() -> Self {
            Self(tempfile::tempdir().unwrap())
        }

        fn path(&self, relative: &str) -> PathBuf {
            self.0.path().join(relative)
        }

        fn root(&self) -> PathBuf {
            self.path("root")
        }

        fn var(&self, relative: &str) -> String {
            self.path(relative).to_str().unwrap().to_owned()
        }

        /// Writes a fontconfig file at `relative` holding `body`.
        fn write_config(&self, relative: &str, body: &str) {
            let path = self.path(relative);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, format!("<fontconfig>{body}</fontconfig>")).unwrap();
        }

        /// Writes a fontconfig file at `relative` that lists one font
        /// directory, `fonts/<name>`.
        fn config(&self, relative: &str, name: &str) {
            let dir = self.font_dirs(&[name]).remove(0);
            self.write_config(relative, &format!("<dir>{}</dir>", dir.display()));
        }

        fn font_dirs(&self, names: &[&str]) -> Vec<PathBuf> {
            names
                .iter()
                .map(|name| self.path("fonts").join(name))
                .collect()
        }

        /// The directories the fontconfig rule finds on a Linux system.
        fn fontconfig_dirs(&self, env: &MapEnv) -> Vec<PathBuf> {
            fontconfig::find(env, &self.root(), fontconfig::config_dirs(false)).dirs
        }
    }

    fn paths<const N: usize>(paths: [&str; N]) -> [PathBuf; N] {
        paths.map(PathBuf::from)
    }

    #[test]
    fn windows_fonts_are_under_the_system_root_then_the_user_profile() {
        let env = MapEnv::new()
            .with("SYSTEMROOT", "D:\\WinNT")
            .with("USERPROFILE", "D:\\Users\\me");
        // Only the join of a variable and the rule's own path uses the
        // host's separator.
        let sep = MAIN_SEPARATOR;
        assert_eq!(
            windows(&env),
            [
                format!("D:\\WinNT{sep}Fonts"),
                format!("D:\\Users\\me{sep}AppData\\Local\\Microsoft\\Windows\\Fonts"),
                format!("D:\\Users\\me{sep}AppData\\Roaming\\Microsoft\\Windows\\Fonts"),
            ]
            .map(PathBuf::from)
        );
    }

    #[test]
    fn windows_without_its_variables_has_only_the_default_system_directory() {
        assert_eq!(windows(&MapEnv::new()), paths(["C:\\Windows\\Fonts\\"]));
    }

    #[test]
    fn apple_fonts_are_system_wide_then_downloads_by_name_then_the_users() {
        let machine = Machine::new();
        let root = machine.root();
        let assets = root.join("System/Library/AssetsV2");
        for name in ["Font8", "Font6", "Voice", "Font7", "Font5"] {
            std::fs::create_dir_all(assets.join(format!("com_apple_MobileAsset_{name}"))).unwrap();
        }
        let env = MapEnv::new().with("HOME", "/Users/me");
        assert_eq!(
            apple(&env, &root),
            [
                root.join("Library/Fonts"),
                root.join("System/Library/Fonts"),
                assets.join("com_apple_MobileAsset_Font5"),
                assets.join("com_apple_MobileAsset_Font6"),
                assets.join("com_apple_MobileAsset_Font7"),
                assets.join("com_apple_MobileAsset_Font8"),
                root.join("Network/Library/Fonts"),
                PathBuf::from("/Users/me/Library/Fonts"),
            ]
        );
    }

    #[test]
    fn apple_without_a_home_or_downloads_has_only_the_fixed_directories() {
        assert_eq!(
            apple(&MapEnv::new(), Path::new("/machine")),
            paths([
                "/machine/Library/Fonts",
                "/machine/System/Library/Fonts",
                "/machine/Network/Library/Fonts",
            ])
        );
    }

    #[test]
    fn unix_fonts_are_system_wide_then_under_home() {
        let root = Path::new("/machine");
        assert_eq!(
            unix(&MapEnv::new().with("HOME", "/home/me"), root),
            paths([
                "/machine/usr/share/fonts/",
                "/machine/usr/local/share/fonts/",
                "/home/me/.fonts",
                "/home/me/.local/share/fonts",
            ])
        );
        assert_eq!(
            unix(&MapEnv::new(), root),
            paths([
                "/machine/usr/share/fonts/",
                "/machine/usr/local/share/fonts/"
            ])
        );
    }

    /// Whatever the platform's rule, the fixed paths it names are under the
    /// root, so a test never reads the machine it runs on.
    #[cfg(all(unix, not(target_os = "android")))]
    #[test]
    fn the_platform_rule_looks_only_under_the_root() {
        let machine = Machine::new();
        let found = super::find(&MapEnv::new(), &machine.root());
        assert_ne!(found.dirs, Vec::<PathBuf>::new());
        for dir in &found.dirs {
            assert!(dir.starts_with(machine.root()), "{}", dir.display());
        }
    }

    #[test]
    fn fontconfig_file_names_the_whole_configuration() {
        let machine = Machine::new();
        machine.config("custom.conf", "custom");
        machine.config("home/.config/fontconfig/fonts.conf", "user");
        machine.config("root/etc/fonts/fonts.conf", "system");
        let env = MapEnv::new()
            .with("FONTCONFIG_FILE", machine.var("custom.conf"))
            .with("HOME", machine.var("home"));
        assert_eq!(
            machine.fontconfig_dirs(&env),
            machine.font_dirs(&["custom"])
        );
    }

    #[test]
    fn the_users_configuration_comes_first_and_stands_in_for_local_conf() {
        let machine = Machine::new();
        machine.config("xdg/fontconfig/fonts.conf", "xdg");
        machine.config("home/.config/fontconfig/fonts.conf", "home");
        machine.config("root/etc/fonts/local.conf", "local");
        machine.config("root/etc/fonts/fonts.conf", "system");
        let home = MapEnv::new().with("HOME", machine.var("home"));
        assert_eq!(
            machine.fontconfig_dirs(&home),
            machine.font_dirs(&["home", "system"])
        );
        // $XDG_CONFIG_HOME replaces $HOME/.config.
        let xdg = home.with("XDG_CONFIG_HOME", machine.var("xdg"));
        assert_eq!(
            machine.fontconfig_dirs(&xdg),
            machine.font_dirs(&["xdg", "system"])
        );
    }

    #[test]
    fn local_conf_is_read_when_no_users_configuration_merges() {
        let machine = Machine::new();
        machine.config("home/.config/fontconfig/fonts.conf", "home");
        machine.config("root/etc/fonts/local.conf", "local");
        machine.config("root/etc/fonts/fonts.conf", "system");
        let expected = machine.font_dirs(&["local", "system"]);
        // No user directory is known.
        assert_eq!(machine.fontconfig_dirs(&MapEnv::new()), expected);
        // The user directory holds no configuration.
        let empty = MapEnv::new()
            .with("HOME", machine.var("home"))
            .with("XDG_CONFIG_HOME", machine.var("empty"));
        assert_eq!(machine.fontconfig_dirs(&empty), expected);
    }

    #[test]
    fn every_system_configuration_directory_is_read_in_order() {
        let machine = Machine::new();
        machine.config("root/usr/local/etc/fonts/local.conf", "ports-local");
        machine.config("root/usr/local/etc/fonts/fonts.conf", "ports");
        machine.config("root/etc/fonts/local.conf", "local");
        machine.config("root/etc/fonts/fonts.conf", "system");
        let find = |bsd| {
            fontconfig::find(
                &MapEnv::new(),
                &machine.root(),
                fontconfig::config_dirs(bsd),
            )
            .dirs
        };
        assert_eq!(find(false), machine.font_dirs(&["local", "system"]));
        assert_eq!(
            find(true),
            machine.font_dirs(&["local", "system", "ports-local", "ports"])
        );
    }

    #[test]
    fn a_configuration_that_lists_no_directory_gives_the_fixed_ones_and_its_aliases() {
        let machine = Machine::new();
        machine.write_config(
            "root/etc/fonts/fonts.conf",
            "<alias><family>serif</family><prefer><family>Preferred</family></prefer>\
             <accept><family>Accepted</family></accept>\
             <default><family>Defaulted</family></default></alias>\
             <alias><family>monospace</family></alias>",
        );
        let env = MapEnv::new().with("HOME", "/home/me");
        let found = fontconfig::find(&env, &machine.root(), fontconfig::config_dirs(false));

        assert_eq!(found.dirs, unix(&env, &machine.root()));
        let aliases: Vec<_> = found
            .aliases
            .iter()
            .map(|alias| {
                (
                    alias.family.as_str(),
                    [&alias.prefer, &alias.accept, &alias.default].map(|list| list.join(",")),
                )
            })
            .collect();
        assert_eq!(
            aliases,
            [
                (
                    "serif",
                    ["Preferred", "Accepted", "Defaulted"].map(String::from)
                ),
                ("monospace", [const { String::new() }; 3]),
            ]
        );
    }

    #[test]
    fn a_leading_tilde_expands_to_the_home_directory() {
        let home = Some(Path::new("/home/user"));
        assert_eq!(
            fontconfig::expand_home(PathBuf::from("~/fonts"), home),
            Some(PathBuf::from("/home/user/fonts"))
        );
    }

    #[test]
    fn a_tilde_path_is_dropped_when_the_home_directory_is_unknown() {
        assert_eq!(
            fontconfig::expand_home(PathBuf::from("~/fonts"), None),
            None
        );
    }

    #[test]
    fn a_path_without_a_leading_tilde_is_kept() {
        let path = PathBuf::from("/usr/share/fonts/~");
        assert_eq!(fontconfig::expand_home(path.clone(), None), Some(path));
    }

    #[test]
    fn a_configuration_file_merges() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("fonts.conf");
        std::fs::write(&path, "<fontconfig><dir>/srv/fonts</dir></fontconfig>").unwrap();
        let mut fontconfig = fontconfig_parser::FontConfig::default();
        assert!(fontconfig::merge(&mut fontconfig, &path));
        assert_eq!(fontconfig.dirs.len(), 1);
    }

    #[test]
    fn a_missing_configuration_file_does_not_merge() {
        let mut fontconfig = fontconfig_parser::FontConfig::default();
        assert!(!fontconfig::merge(
            &mut fontconfig,
            Path::new("/nonexistent/fontconfig/fonts.conf")
        ));
    }

    /// A generic family resolves to the first installed family fontconfig
    /// prefers for it, even when a later alias prefers another family.
    #[test]
    fn generic_family_is_the_first_installed_preference() {
        let fonts = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fonts");
        let config = format!(
            r#"<?xml version="1.0"?>
<fontconfig>
  <dir>{}</dir>
  <alias>
    <family>serif</family>
    <prefer><family>Missing Serif</family><family>Tuffy</family></prefer>
  </alias>
  <alias>
    <family>serif</family>
    <prefer><family>Missing Symbols</family></prefer>
  </alias>
  <alias>
    <family>monospace</family>
    <prefer><family>Missing Mono</family></prefer>
  </alias>
</fontconfig>
"#,
            fonts.display()
        );
        let machine = Machine::new();
        std::fs::write(machine.path("fonts.conf"), config).unwrap();
        let env = MapEnv::new().with("FONTCONFIG_FILE", machine.var("fonts.conf"));
        let found = fontconfig::find(&env, &machine.root(), fontconfig::config_dirs(false));

        let mut database = Database::new();
        let report = database.load_found_system_fonts(&found);

        assert_eq!(report.added().len(), 1);
        assert_eq!(database.len(), 1);
        let family_name = |generic| database.family_name(&Family::Generic(generic)).to_owned();
        assert_eq!(family_name(GenericFamily::Serif), "Tuffy");
        // Without an installed preference, the family is the first one fontconfig tries.
        assert_eq!(family_name(GenericFamily::Monospace), "Missing Mono");
        // A generic family without an alias keeps its default.
        assert_eq!(family_name(GenericFamily::Cursive), "Comic Sans MS");
    }

    #[test]
    fn the_directories_found_load_in_their_order_and_a_missing_one_adds_nothing() {
        let tuffy = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fonts/Tuffy.ttf");
        let machine = Machine::new();
        let dirs = machine.font_dirs(&["later", "missing", "earlier"]);
        let mut files = Vec::new();
        for (dir, name) in [(&dirs[0], "a.ttf"), (&dirs[2], "b.ttf")] {
            std::fs::create_dir_all(dir).unwrap();
            let file = std::fs::canonicalize(dir).unwrap().join(name);
            std::fs::copy(&tuffy, &file).unwrap();
            files.push(file);
        }

        let mut database = Database::new();
        let report = database.load_found_system_fonts(&SystemFonts::from(dirs));

        assert!(report.failures().is_empty());
        let loaded: Vec<_> = database
            .faces()
            .iter()
            .map(|face| match &face.source {
                Source::File(path) => std::fs::canonicalize(path).unwrap(),
                Source::Binary(_) => PathBuf::new(),
            })
            .collect();
        assert_eq!(loaded, files);
    }
}
