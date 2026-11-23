//! Makes a PostScript name for a face whose font has none.

use alloc::string::String;

/// The characters a PostScript literal name cannot hold, as fontconfig lists
/// them.
const FORBIDDEN: &[char] = &[
    '\u{4}', '(', ')', '/', '<', '>', '[', ']', '{', '}', '\t', '\u{c}', '\r', '\n', ' ',
];

/// The most characters fontconfig keeps.
const MAX_CHARS: usize = 255;

/// Makes a PostScript name from `family` the way fontconfig does when a font
/// has none: each character PostScript forbids in a literal name, spaces
/// included, becomes `-`, and the name stops at 255 characters. The style is
/// not added.
///
/// fontconfig counts the 255 in bytes; this counts characters, so a name with
/// non-ASCII characters may run longer than fontconfig's.
pub(crate) fn from_family(family: &str) -> String {
    family
        .chars()
        .take(MAX_CHARS)
        .map(|c| if FORBIDDEN.contains(&c) { '-' } else { c })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::from_family;

    #[test]
    fn spaces_become_hyphens() {
        assert_eq!(from_family("DEC Terminal Modern"), "DEC-Terminal-Modern");
    }

    #[test]
    fn a_name_without_forbidden_characters_is_kept() {
        assert_eq!(from_family("Tuffy"), "Tuffy");
    }

    #[test]
    fn every_character_postscript_forbids_becomes_a_hyphen() {
        assert_eq!(
            from_family("a\u{4}b(c)d/e<f>g[h]i{j}k\tl\u{c}m\rn\no p"),
            "a-b-c-d-e-f-g-h-i-j-k-l-m-n-o-p"
        );
    }

    #[test]
    fn other_characters_are_kept() {
        assert_eq!(
            from_family("Noto Sans CJK 日本語 %&"),
            "Noto-Sans-CJK-日本語-%&"
        );
    }

    #[test]
    fn the_name_stops_at_255_characters() {
        let long = "x".repeat(300);
        assert_eq!(from_family(&long).chars().count(), 255);
        assert_eq!(from_family(&"é".repeat(300)).chars().count(), 255);
    }
}
