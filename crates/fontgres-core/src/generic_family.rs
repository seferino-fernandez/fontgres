//! Resolves a CSS generic family through fontconfig's `<alias>` rules.

use alloc::{string::String, vec::Vec};

/// One fontconfig `<alias>`: the family it is for and its three family lists.
pub(crate) struct Alias {
    /// The aliased family, such as `serif`.
    pub(crate) family: String,
    /// The `<prefer>` families.
    pub(crate) prefer: Vec<String>,
    /// The `<accept>` families.
    pub(crate) accept: Vec<String>,
    /// The `<default>` families.
    pub(crate) default: Vec<String>,
}

/// Returns the family that `generic` resolves to, given the aliases in config
/// order and which families are installed.
///
/// `generic` lists the alias names to read, compared ignoring ASCII case
/// (`sans-serif` and `sans serif` are one generic family). The families are
/// tried in fontconfig's order: the `prefer` families of every alias in
/// config order, then their `accept` families from the last alias to the
/// first (each alias inserts its list right after the generic family), then
/// their `default` families in config order. The first installed one wins.
/// With none installed, the first family tried is kept, as fontconfig would
/// name it; with no alias for `generic`, the result is `None`.
pub(crate) fn resolve<'a>(
    aliases: &'a [Alias],
    generic: &[&str],
    is_installed: impl Fn(&str) -> bool,
) -> Option<&'a str> {
    let aliases: Vec<&Alias> = aliases
        .iter()
        .filter(|alias| {
            generic
                .iter()
                .any(|name| alias.family.eq_ignore_ascii_case(name))
        })
        .collect();
    let mut candidates = aliases
        .iter()
        .flat_map(|alias| &alias.prefer)
        .chain(aliases.iter().rev().flat_map(|alias| &alias.accept))
        .chain(aliases.iter().flat_map(|alias| &alias.default))
        .map(String::as_str);
    let first = candidates.clone().next()?;
    Some(candidates.find(|name| is_installed(name)).unwrap_or(first))
}

#[cfg(test)]
mod tests {
    use super::{Alias, resolve};
    use alloc::{borrow::ToOwned, string::String, vec::Vec};

    fn names(list: &[&str]) -> Vec<String> {
        list.iter().map(|name| (*name).to_owned()).collect()
    }

    fn alias(family: &str, prefer: &[&str], accept: &[&str], default: &[&str]) -> Alias {
        Alias {
            family: family.to_owned(),
            prefer: names(prefer),
            accept: names(accept),
            default: names(default),
        }
    }

    fn installed(families: &'static [&'static str]) -> impl Fn(&str) -> bool {
        move |name| families.contains(&name)
    }

    #[test]
    fn a_later_alias_does_not_override_an_installed_earlier_preference() {
        // URW base35 adds a serif alias preferring a symbol font; when it comes
        // last it must not win over the installed family preferred earlier.
        let config = [
            alias("serif", &["Missing Serif", "DejaVu Serif"], &[], &[]),
            alias("serif", &["Standard Symbols PS"], &[], &[]),
        ];
        let found = resolve(
            &config,
            &["serif"],
            installed(&["DejaVu Serif", "Standard Symbols PS"]),
        );
        assert_eq!(found, Some("DejaVu Serif"));
    }

    #[test]
    fn uninstalled_preferences_are_skipped() {
        let config = [alias(
            "serif",
            &["Missing Serif"],
            &["Accepted Serif"],
            &["Default Serif"],
        )];
        let found = resolve(&config, &["serif"], installed(&["Default Serif"]));
        assert_eq!(found, Some("Default Serif"));
    }

    #[test]
    fn without_an_installed_family_the_first_one_tried_is_kept() {
        let config = [
            alias("monospace", &[], &["Missing Accept"], &[]),
            alias("monospace", &["Missing Mono"], &[], &[]),
        ];
        let found = resolve(&config, &["monospace"], installed(&[]));
        assert_eq!(found, Some("Missing Mono"));
    }

    #[test]
    fn prefer_comes_before_accept_and_accept_before_default() {
        let config = [
            alias("cursive", &[], &["Accepted"], &["Defaulted"]),
            alias("cursive", &["Preferred"], &[], &[]),
        ];
        let all = installed(&["Preferred", "Accepted", "Defaulted"]);
        assert_eq!(resolve(&config, &["cursive"], all), Some("Preferred"));
        let no_prefer = installed(&["Accepted", "Defaulted"]);
        assert_eq!(resolve(&config, &["cursive"], no_prefer), Some("Accepted"));
    }

    #[test]
    fn accept_families_of_later_aliases_come_first() {
        // fontconfig inserts each alias's accept list right after the generic
        // family, so the last alias's list ends up first.
        let config = [
            alias("fantasy", &[], &["First Alias"], &[]),
            alias("fantasy", &[], &["Second Alias"], &[]),
        ];
        let both = installed(&["First Alias", "Second Alias"]);
        assert_eq!(resolve(&config, &["fantasy"], both), Some("Second Alias"));
    }

    #[test]
    fn default_families_keep_their_config_order() {
        let config = [
            alias("serif", &[], &[], &["First Default"]),
            alias("serif", &[], &[], &["Second Default"]),
        ];
        let both = installed(&["First Default", "Second Default"]);
        assert_eq!(resolve(&config, &["serif"], both), Some("First Default"));
    }

    #[test]
    fn alias_names_match_any_listed_spelling_ignoring_case() {
        let config = [
            alias("Sans Serif", &["Spaced"], &[], &[]),
            alias("SANS-SERIF", &["Hyphenated"], &[], &[]),
            alias("serif", &["Other Generic"], &[], &[]),
        ];
        let found = resolve(
            &config,
            &["sans-serif", "sans serif"],
            installed(&["Hyphenated"]),
        );
        assert_eq!(found, Some("Hyphenated"));
    }

    #[test]
    fn a_generic_without_aliases_resolves_to_nothing() {
        let config = [alias("serif", &["DejaVu Serif"], &[], &[])];
        assert_eq!(
            resolve(&config, &["monospace"], installed(&["DejaVu Serif"])),
            None
        );
    }
}
