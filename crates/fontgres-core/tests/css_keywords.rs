//! The CSS keywords of `Style` and `Stretch`: `css_name`, and with the
//! `serde` feature the keyword serde reads and writes, which must agree.

use fontgres_core::{Stretch, Style};

const STYLES: [(Style, &str); 3] = [
    (Style::Normal, "normal"),
    (Style::Italic, "italic"),
    (Style::Oblique, "oblique"),
];

const STRETCHES: [(Stretch, &str); 9] = [
    (Stretch::UltraCondensed, "ultra-condensed"),
    (Stretch::ExtraCondensed, "extra-condensed"),
    (Stretch::Condensed, "condensed"),
    (Stretch::SemiCondensed, "semi-condensed"),
    (Stretch::Normal, "normal"),
    (Stretch::SemiExpanded, "semi-expanded"),
    (Stretch::Expanded, "expanded"),
    (Stretch::ExtraExpanded, "extra-expanded"),
    (Stretch::UltraExpanded, "ultra-expanded"),
];

#[test]
fn css_name_is_the_css_keyword() {
    for (style, keyword) in STYLES {
        assert_eq!(style.css_name(), keyword, "{style:?}");
    }
    for (stretch, keyword) in STRETCHES {
        assert_eq!(stretch.css_name(), keyword, "{stretch:?}");
    }
}

#[cfg(feature = "serde")]
#[test]
fn serde_reads_and_writes_the_css_name() {
    fn round_trip<T>(value: &T, keyword: &str)
    where
        T: serde::Serialize + serde::de::DeserializeOwned + PartialEq + core::fmt::Debug,
    {
        assert_eq!(serde_json::to_value(value).unwrap(), keyword, "{value:?}");
        let read: T = serde_json::from_value(keyword.into()).unwrap();
        assert_eq!(&read, value, "{keyword}");
    }
    for (style, _) in STYLES {
        round_trip(&style, style.css_name());
    }
    for (stretch, _) in STRETCHES {
        round_trip(&stretch, stretch.css_name());
    }
}
