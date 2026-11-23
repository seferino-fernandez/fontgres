//! Resolves a CSS generic family through fontconfig's `<alias>` rules.

use alloc::{string::String, vec::Vec};

/// One fontconfig `<alias>`: the family it is for and its three family lists.
pub(crate) struct Alias<'a> {
    /// The aliased family, such as `serif`.
    pub(crate) family: &'a str,
    /// The `<prefer>` families.
    pub(crate) prefer: &'a [String],
    /// The `<accept>` families.
    pub(crate) accept: &'a [String],
    /// The `<default>` families.
    pub(crate) default: &'a [String],
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
    aliases: &[Alias<'a>],
    generic: &[&str],
    is_installed: impl Fn(&str) -> bool,
) -> Option<&'a str> {
    let aliases: Vec<&Alias<'a>> = aliases
        .iter()
        .filter(|alias| {
            generic
                .iter()
                .any(|name| alias.family.eq_ignore_ascii_case(name))
        })
        .collect();
    let mut candidates = aliases
        .iter()
        .flat_map(|alias| alias.prefer)
        .chain(aliases.iter().rev().flat_map(|alias| alias.accept))
        .chain(aliases.iter().flat_map(|alias| alias.default))
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

    struct Lists {
        family: &'static str,
        prefer: Vec<String>,
        accept: Vec<String>,
        default: Vec<String>,
    }

    fn lists(family: &'static str, prefer: &[&str], accept: &[&str], default: &[&str]) -> Lists {
        Lists {
            family,
            prefer: names(prefer),
            accept: names(accept),
            default: names(default),
        }
    }

    fn aliases(lists: &[Lists]) -> Vec<Alias<'_>> {
        lists
            .iter()
            .map(|lists| Alias {
                family: lists.family,
                prefer: &lists.prefer,
                accept: &lists.accept,
                default: &lists.default,
            })
            .collect()
    }

    fn installed(families: &'static [&'static str]) -> impl Fn(&str) -> bool {
        move |name| families.contains(&name)
    }

    #[test]
    fn a_later_alias_does_not_override_an_installed_earlier_preference() {
        // URW base35 adds a serif alias preferring a symbol font; when it comes
        // last it must not win over the installed family preferred earlier.
        let config = [
            lists("serif", &["Missing Serif", "DejaVu Serif"], &[], &[]),
            lists("serif", &["Standard Symbols PS"], &[], &[]),
        ];
        let found = resolve(
            &aliases(&config),
            &["serif"],
            installed(&["DejaVu Serif", "Standard Symbols PS"]),
        );
        assert_eq!(found, Some("DejaVu Serif"));
    }

    #[test]
    fn uninstalled_preferences_are_skipped() {
        let config = [lists(
            "serif",
            &["Missing Serif"],
            &["Accepted Serif"],
            &["Default Serif"],
        )];
        let found = resolve(&aliases(&config), &["serif"], installed(&["Default Serif"]));
        assert_eq!(found, Some("Default Serif"));
    }

    #[test]
    fn without_an_installed_family_the_first_one_tried_is_kept() {
        let config = [
            lists("monospace", &[], &["Missing Accept"], &[]),
            lists("monospace", &["Missing Mono"], &[], &[]),
        ];
        let found = resolve(&aliases(&config), &["monospace"], installed(&[]));
        assert_eq!(found, Some("Missing Mono"));
    }

    #[test]
    fn prefer_comes_before_accept_and_accept_before_default() {
        let config = [
            lists("cursive", &[], &["Accepted"], &["Defaulted"]),
            lists("cursive", &["Preferred"], &[], &[]),
        ];
        let all = installed(&["Preferred", "Accepted", "Defaulted"]);
        assert_eq!(
            resolve(&aliases(&config), &["cursive"], all),
            Some("Preferred")
        );
        let no_prefer = installed(&["Accepted", "Defaulted"]);
        assert_eq!(
            resolve(&aliases(&config), &["cursive"], no_prefer),
            Some("Accepted")
        );
    }

    #[test]
    fn accept_families_of_later_aliases_come_first() {
        // fontconfig inserts each alias's accept list right after the generic
        // family, so the last alias's list ends up first.
        let config = [
            lists("fantasy", &[], &["First Alias"], &[]),
            lists("fantasy", &[], &["Second Alias"], &[]),
        ];
        let both = installed(&["First Alias", "Second Alias"]);
        assert_eq!(
            resolve(&aliases(&config), &["fantasy"], both),
            Some("Second Alias")
        );
    }

    #[test]
    fn default_families_keep_their_config_order() {
        let config = [
            lists("serif", &[], &[], &["First Default"]),
            lists("serif", &[], &[], &["Second Default"]),
        ];
        let both = installed(&["First Default", "Second Default"]);
        assert_eq!(
            resolve(&aliases(&config), &["serif"], both),
            Some("First Default")
        );
    }

    #[test]
    fn alias_names_match_any_listed_spelling_ignoring_case() {
        let config = [
            lists("Sans Serif", &["Spaced"], &[], &[]),
            lists("SANS-SERIF", &["Hyphenated"], &[], &[]),
            lists("serif", &["Other Generic"], &[], &[]),
        ];
        let found = resolve(
            &aliases(&config),
            &["sans-serif", "sans serif"],
            installed(&["Hyphenated"]),
        );
        assert_eq!(found, Some("Hyphenated"));
    }

    #[test]
    fn a_generic_without_aliases_resolves_to_nothing() {
        let config = [lists("serif", &["DejaVu Serif"], &[], &[])];
        assert_eq!(
            resolve(
                &aliases(&config),
                &["monospace"],
                installed(&["DejaVu Serif"])
            ),
            None
        );
    }
}
