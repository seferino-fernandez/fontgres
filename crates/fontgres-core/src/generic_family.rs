//! The CSS generic families: their names, the family each stands for in a
//! new database, a value kept for each of them, and how fontconfig's
//! `<alias>` rules resolve one.

use alloc::string::String;
#[cfg(any(test, feature = "fs"))]
use alloc::vec::Vec;

// Variant descriptions are from the CSS spec.
/// One of the CSS
/// [generic font families](https://www.w3.org/TR/2018/REC-css-fonts-3-20180920/#generic-font-families).
/// Each stands for one family, chosen per database.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum GenericFamily {
    /// Serif fonts represent the formal text style for a script.
    Serif,

    /// Glyphs in sans-serif fonts, as the term is used in CSS, are generally low contrast
    /// and have stroke endings that are plain, without any flaring, cross stroke,
    /// or other ornamentation.
    SansSerif,

    /// Glyphs in cursive fonts generally use a more informal script style,
    /// and the result looks more like handwritten pen or brush writing than printed letterwork.
    Cursive,

    /// Fantasy fonts are primarily decorative or expressive fonts that
    /// contain decorative or expressive representations of characters.
    Fantasy,

    /// The sole criterion of a monospace font is that all glyphs have the same fixed width.
    Monospace,
}

impl GenericFamily {
    /// Every generic family.
    pub const ALL: [Self; 5] = [
        Self::Serif,
        Self::SansSerif,
        Self::Cursive,
        Self::Fantasy,
        Self::Monospace,
    ];

    /// Returns the name CSS gives the generic family, such as `sans-serif`.
    pub const fn css_name(self) -> &'static str {
        match self {
            Self::Serif => "serif",
            Self::SansSerif => "sans-serif",
            Self::Cursive => "cursive",
            Self::Fantasy => "fantasy",
            Self::Monospace => "monospace",
        }
    }

    /// Returns the generic family CSS names `name`, compared ignoring ASCII
    /// case, or `None` when `name` is not a generic family's name.
    ///
    /// # Examples
    ///
    /// ```
    /// use fontgres_core::GenericFamily;
    ///
    /// assert_eq!(
    ///     GenericFamily::from_css_name("Sans-Serif"),
    ///     Some(GenericFamily::SansSerif)
    /// );
    /// assert_eq!(GenericFamily::from_css_name("Helvetica"), None);
    /// ```
    pub fn from_css_name(name: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|generic| generic.css_name().eq_ignore_ascii_case(name))
    }

    /// Whether a fontconfig `<alias>` for `family` is one for this generic
    /// family. fontconfig's own configuration spells `sans-serif` with a
    /// space too.
    #[cfg(any(test, feature = "fs"))]
    fn is_aliased_as(self, family: &str) -> bool {
        self.css_name().eq_ignore_ascii_case(family)
            || (self == Self::SansSerif && family.eq_ignore_ascii_case("sans serif"))
    }
}

/// One value for each generic family. The configuration keeps the family
/// the user chose for each, where `None` keeps the database's own.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[cfg_attr(
    feature = "cli",
    derive(serde::Deserialize),
    serde(deny_unknown_fields)
)]
pub struct GenericFamilies<T = Option<String>> {
    /// The value for `serif`.
    pub serif: T,
    /// The value for `sans-serif`.
    pub sans_serif: T,
    /// The value for `cursive`.
    pub cursive: T,
    /// The value for `fantasy`.
    pub fantasy: T,
    /// The value for `monospace`.
    pub monospace: T,
}

impl<T> GenericFamilies<T> {
    /// Returns the value for `generic`.
    pub fn get(&self, generic: GenericFamily) -> &T {
        match generic {
            GenericFamily::Serif => &self.serif,
            GenericFamily::SansSerif => &self.sans_serif,
            GenericFamily::Cursive => &self.cursive,
            GenericFamily::Fantasy => &self.fantasy,
            GenericFamily::Monospace => &self.monospace,
        }
    }

    /// Returns the value for `generic`, to change it.
    pub fn get_mut(&mut self, generic: GenericFamily) -> &mut T {
        match generic {
            GenericFamily::Serif => &mut self.serif,
            GenericFamily::SansSerif => &mut self.sans_serif,
            GenericFamily::Cursive => &mut self.cursive,
            GenericFamily::Fantasy => &mut self.fantasy,
            GenericFamily::Monospace => &mut self.monospace,
        }
    }
}

impl GenericFamilies<String> {
    /// The family each generic family stands for in a new database. `apple`
    /// is whether the platform is macOS or iOS, which ship no Impact.
    pub(crate) fn defaults(apple: bool) -> Self {
        Self {
            serif: "Times New Roman".into(),
            sans_serif: "Arial".into(),
            cursive: "Comic Sans MS".into(),
            fantasy: if apple { "Papyrus" } else { "Impact" }.into(),
            monospace: "Courier New".into(),
        }
    }
}

/// One fontconfig `<alias>`: the family it is for and its three family lists.
#[cfg(any(test, feature = "fs"))]
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
/// An alias is read when it is for `generic`'s CSS name, compared ignoring
/// ASCII case (`sans serif` is `sans-serif` too). The families are tried in
/// fontconfig's order: the `prefer` families of every alias in
/// config order, then their `accept` families from the last alias to the
/// first (each alias inserts its list right after the generic family), then
/// their `default` families in config order. The first installed one wins.
/// With none installed, the first family tried is kept, as fontconfig would
/// name it; with no alias for `generic`, the result is `None`.
#[cfg(any(test, feature = "fs"))]
pub(crate) fn resolve(
    aliases: &[Alias],
    generic: GenericFamily,
    is_installed: impl Fn(&str) -> bool,
) -> Option<&str> {
    let aliases: Vec<&Alias> = aliases
        .iter()
        .filter(|alias| generic.is_aliased_as(&alias.family))
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
    use super::{Alias, GenericFamilies, GenericFamily, resolve};
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
            GenericFamily::Serif,
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
        let found = resolve(&config, GenericFamily::Serif, installed(&["Default Serif"]));
        assert_eq!(found, Some("Default Serif"));
    }

    #[test]
    fn without_an_installed_family_the_first_one_tried_is_kept() {
        let config = [
            alias("monospace", &[], &["Missing Accept"], &[]),
            alias("monospace", &["Missing Mono"], &[], &[]),
        ];
        let found = resolve(&config, GenericFamily::Monospace, installed(&[]));
        assert_eq!(found, Some("Missing Mono"));
    }

    #[test]
    fn prefer_comes_before_accept_and_accept_before_default() {
        let config = [
            alias("cursive", &[], &["Accepted"], &["Defaulted"]),
            alias("cursive", &["Preferred"], &[], &[]),
        ];
        let all = installed(&["Preferred", "Accepted", "Defaulted"]);
        assert_eq!(
            resolve(&config, GenericFamily::Cursive, all),
            Some("Preferred")
        );
        let no_prefer = installed(&["Accepted", "Defaulted"]);
        assert_eq!(
            resolve(&config, GenericFamily::Cursive, no_prefer),
            Some("Accepted")
        );
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
        assert_eq!(
            resolve(&config, GenericFamily::Fantasy, both),
            Some("Second Alias")
        );
    }

    #[test]
    fn default_families_keep_their_config_order() {
        let config = [
            alias("serif", &[], &[], &["First Default"]),
            alias("serif", &[], &[], &["Second Default"]),
        ];
        let both = installed(&["First Default", "Second Default"]);
        assert_eq!(
            resolve(&config, GenericFamily::Serif, both),
            Some("First Default")
        );
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
            GenericFamily::SansSerif,
            installed(&["Hyphenated"]),
        );
        assert_eq!(found, Some("Hyphenated"));
    }

    #[test]
    fn the_spaced_spelling_names_only_sans_serif() {
        let config = [
            alias("serif", &["Serif"], &[], &[]),
            alias("sans serif", &["Spaced"], &[], &[]),
            alias("monospace", &["Mono"], &[], &[]),
        ];
        let all = || installed(&["Serif", "Spaced", "Mono"]);
        let resolved = [
            GenericFamily::Serif,
            GenericFamily::SansSerif,
            GenericFamily::Monospace,
        ]
        .map(|generic| resolve(&config, generic, all()));
        assert_eq!(resolved, [Some("Serif"), Some("Spaced"), Some("Mono")]);
    }

    #[test]
    fn a_generic_without_aliases_resolves_to_nothing() {
        let config = [alias("serif", &["DejaVu Serif"], &[], &[])];
        assert_eq!(
            resolve(
                &config,
                GenericFamily::Monospace,
                installed(&["DejaVu Serif"])
            ),
            None
        );
    }

    #[test]
    fn css_names_read_back_in_any_case_and_other_names_are_not_generic() {
        let names = GenericFamily::ALL.map(GenericFamily::css_name);
        assert_eq!(
            names,
            ["serif", "sans-serif", "cursive", "fantasy", "monospace"]
        );
        for generic in GenericFamily::ALL {
            let name = generic.css_name().to_ascii_uppercase();
            assert_eq!(GenericFamily::from_css_name(&name), Some(generic));
        }
        for name in ["", "sans serif", "sans", "Tuffy"] {
            assert_eq!(GenericFamily::from_css_name(name), None, "{name}");
        }
    }

    #[test]
    fn each_generic_family_has_its_own_value() {
        let mut families = GenericFamilies::<&str>::default();
        for generic in GenericFamily::ALL {
            *families.get_mut(generic) = generic.css_name();
        }
        assert_eq!(
            families,
            GenericFamilies {
                serif: "serif",
                sans_serif: "sans-serif",
                cursive: "cursive",
                fantasy: "fantasy",
                monospace: "monospace",
            }
        );
        for generic in GenericFamily::ALL {
            assert_eq!(*families.get(generic), generic.css_name());
        }
    }

    #[test]
    fn only_fantasy_defaults_differently_on_apple_platforms() {
        let other = GenericFamilies::defaults(false);
        assert_eq!(
            GenericFamily::ALL.map(|generic| other.get(generic).as_str()),
            [
                "Times New Roman",
                "Arial",
                "Comic Sans MS",
                "Impact",
                "Courier New"
            ]
        );
        assert_eq!(
            GenericFamilies::defaults(true),
            GenericFamilies {
                fantasy: "Papyrus".to_owned(),
                ..other
            }
        );
    }
}
