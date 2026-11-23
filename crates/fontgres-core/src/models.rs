//! Serializable views of the font database for the workspace binaries, and
//! the font loading and querying they share.
//!
//! fontdb's own types are left untouched: [`Face`](crate::models::Face) is
//! built from a [`FaceInfo`](crate::FaceInfo), so the CLI and MCP output stays stable if `FaceInfo`
//! changes.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::config::FontsConfig;
use crate::{Database, FaceInfo, Family, Query, Source, Stretch, Style, Weight};

/// One font face, as the binaries present it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct Face {
    /// The face's primary family name (English US when the font has one).
    pub family: String,
    /// Every family name the face carries, in every language, primary first.
    pub families: Vec<String>,
    /// The face's `PostScript` name.
    pub post_script_name: String,
    /// Normal, italic or oblique.
    pub style: FontStyle,
    /// CSS weight: 100 (thin) to 900 (black); 400 is normal and 700 bold.
    pub weight: u16,
    /// CSS stretch, from `ultra-condensed` to `ultra-expanded`.
    pub stretch: FontStretch,
    /// Whether every glyph has the same advance width.
    pub monospaced: bool,
    /// The file the face was loaded from; `None` for in-memory data.
    pub path: Option<PathBuf>,
    /// The face's index within its file (non-zero only in font collections).
    pub index: u32,
}

impl From<&FaceInfo> for Face {
    fn from(info: &FaceInfo) -> Self {
        let families: Vec<String> = info.families.iter().map(|(name, _)| name.clone()).collect();
        Self {
            family: families.first().cloned().unwrap_or_default(),
            families,
            post_script_name: info.post_script_name.clone(),
            style: info.style.into(),
            weight: info.weight.0,
            stretch: info.stretch.into(),
            monospaced: info.monospaced,
            path: source_path(&info.source).map(Path::to_path_buf),
            index: info.index,
        }
    }
}

/// The file a source was read from, if any.
fn source_path(source: &Source) -> Option<&Path> {
    match source {
        Source::Binary(_) => None,
        Source::File(path) => Some(path),
        #[cfg(feature = "memmap")]
        Source::SharedFile(path, _) => Some(path),
    }
}

/// A face's style, the CSS `font-style`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, clap::ValueEnum)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "kebab-case")]
pub enum FontStyle {
    /// Neither italic nor oblique.
    #[default]
    Normal,
    /// A form that is generally cursive in nature.
    Italic,
    /// A typically sloped version of the regular face.
    Oblique,
}

impl From<Style> for FontStyle {
    fn from(style: Style) -> Self {
        match style {
            Style::Normal => Self::Normal,
            Style::Italic => Self::Italic,
            Style::Oblique => Self::Oblique,
        }
    }
}

impl From<FontStyle> for Style {
    fn from(style: FontStyle) -> Self {
        match style {
            FontStyle::Normal => Self::Normal,
            FontStyle::Italic => Self::Italic,
            FontStyle::Oblique => Self::Oblique,
        }
    }
}

/// A face's width, the CSS `font-stretch`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, clap::ValueEnum)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "kebab-case")]
pub enum FontStretch {
    /// 50% of normal width.
    UltraCondensed,
    /// 62.5% of normal width.
    ExtraCondensed,
    /// 75% of normal width.
    Condensed,
    /// 87.5% of normal width.
    SemiCondensed,
    /// Normal width.
    #[default]
    Normal,
    /// 112.5% of normal width.
    SemiExpanded,
    /// 125% of normal width.
    Expanded,
    /// 150% of normal width.
    ExtraExpanded,
    /// 200% of normal width.
    UltraExpanded,
}

impl From<Stretch> for FontStretch {
    fn from(stretch: Stretch) -> Self {
        match stretch {
            Stretch::UltraCondensed => Self::UltraCondensed,
            Stretch::ExtraCondensed => Self::ExtraCondensed,
            Stretch::Condensed => Self::Condensed,
            Stretch::SemiCondensed => Self::SemiCondensed,
            Stretch::Normal => Self::Normal,
            Stretch::SemiExpanded => Self::SemiExpanded,
            Stretch::Expanded => Self::Expanded,
            Stretch::ExtraExpanded => Self::ExtraExpanded,
            Stretch::UltraExpanded => Self::UltraExpanded,
        }
    }
}

impl From<FontStretch> for Stretch {
    fn from(stretch: FontStretch) -> Self {
        match stretch {
            FontStretch::UltraCondensed => Self::UltraCondensed,
            FontStretch::ExtraCondensed => Self::ExtraCondensed,
            FontStretch::Condensed => Self::Condensed,
            FontStretch::SemiCondensed => Self::SemiCondensed,
            FontStretch::Normal => Self::Normal,
            FontStretch::SemiExpanded => Self::SemiExpanded,
            FontStretch::Expanded => Self::Expanded,
            FontStretch::ExtraExpanded => Self::ExtraExpanded,
            FontStretch::UltraExpanded => Self::UltraExpanded,
        }
    }
}

/// A CSS-like font query: the best face for the first family that has one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FontQuery {
    /// Families in priority order. The CSS generic names `serif`,
    /// `sans-serif`, `cursive`, `fantasy` and `monospace` (any case) resolve
    /// through the database's generic families; any other name is a family name.
    pub families: Vec<String>,
    /// Wanted CSS weight (400 is normal, 700 bold).
    pub weight: u16,
    /// Wanted style.
    pub style: FontStyle,
    /// Wanted stretch.
    pub stretch: FontStretch,
}

impl FontQuery {
    /// A query for `families` at normal weight, style and stretch.
    pub fn new(families: Vec<String>) -> Self {
        Self {
            families,
            weight: Weight::NORMAL.0,
            style: FontStyle::Normal,
            stretch: FontStretch::Normal,
        }
    }

    /// Runs the query against `database`, returning the matching face, or
    /// `None` when no listed family is in the database.
    ///
    /// # Examples
    ///
    /// ```
    /// use fontgres_core::Database;
    /// use fontgres_core::models::FontQuery;
    ///
    /// let mut database = Database::new();
    /// database.load_font_data(std::fs::read("tests/fonts/Tuffy.ttf")?);
    /// let mut query = FontQuery::new(vec!["Missing".into(), "Tuffy".into()]);
    /// query.weight = 700;
    /// let face = query.run(&database).expect("Tuffy is loaded");
    /// assert_eq!(face.family, "Tuffy");
    /// assert!(FontQuery::new(vec!["Missing".into()]).run(&database).is_none());
    /// # Ok::<(), std::io::Error>(())
    /// ```
    pub fn run(&self, database: &Database) -> Option<Face> {
        let families: Vec<Family<'_>> = self.families.iter().map(|name| family(name)).collect();
        let query = Query {
            families: &families,
            weight: Weight(self.weight),
            stretch: self.stretch.into(),
            style: self.style.into(),
        };
        let id = database.query(&query)?;
        database.face(id).map(Face::from)
    }
}

/// Reads `name` as a CSS generic family when it is one (in any case), and as
/// a family name otherwise.
fn family(name: &str) -> Family<'_> {
    match name.to_ascii_lowercase().as_str() {
        "serif" => Family::Serif,
        "sans-serif" => Family::SansSerif,
        "cursive" => Family::Cursive,
        "fantasy" => Family::Fantasy,
        "monospace" => Family::Monospace,
        _ => Family::Name(name),
    }
}

/// The faces in `database`, sorted by family, then weight, style, stretch and
/// `PostScript` name. With `family`, only the faces that carry that family name
/// in any language, compared case-insensitively.
pub fn faces(database: &Database, family: Option<&str>) -> Vec<Face> {
    let mut faces: Vec<Face> = database
        .faces()
        .filter(|info| {
            family.is_none_or(|wanted| {
                info.families
                    .iter()
                    .any(|(name, _)| name.eq_ignore_ascii_case(wanted))
            })
        })
        .map(Face::from)
        .collect();
    faces.sort_by(|a, b| {
        a.family
            .cmp(&b.family)
            .then(a.weight.cmp(&b.weight))
            .then((a.style as u8).cmp(&(b.style as u8)))
            .then((a.stretch as u8).cmp(&(b.stretch as u8)))
            .then(a.post_script_name.cmp(&b.post_script_name))
    });
    faces
}

/// The distinct primary family names in `database`, sorted.
pub fn family_names(database: &Database) -> Vec<String> {
    let mut names: Vec<String> = database
        .faces()
        .filter_map(|info| info.families.first().map(|(name, _)| name.clone()))
        .collect();
    names.sort();
    names.dedup();
    names
}

impl FontsConfig {
    /// Builds a database holding the fonts this configuration names: the
    /// system fonts when [`system`](Self::system) is set, then each directory
    /// and file, with the generic families applied.
    ///
    /// A file that fails to parse as a font is skipped with a logged warning,
    /// as fontdb does.
    ///
    /// # Errors
    ///
    /// Returns [`Error::FontDirNotFound`](crate::Error::FontDirNotFound) when a
    /// directory does not exist, and
    /// [`Error::FontFile`](crate::Error::FontFile) when a file cannot be read.
    pub fn build_database(&self) -> crate::Result<Database> {
        let mut database = Database::new();
        if self.system {
            database.load_system_fonts();
        }
        for dir in &self.dirs {
            if !dir.is_dir() {
                return Err(crate::Error::FontDirNotFound { path: dir.clone() });
            }
            database.load_fonts_dir(dir);
        }
        for path in &self.files {
            database
                .load_font_file(path)
                .map_err(|source| crate::Error::FontFile {
                    path: path.clone(),
                    source,
                })?;
        }
        let generic = &self.generic;
        if let Some(family) = &generic.serif {
            database.set_serif_family(family.as_str());
        }
        if let Some(family) = &generic.sans_serif {
            database.set_sans_serif_family(family.as_str());
        }
        if let Some(family) = &generic.cursive {
            database.set_cursive_family(family.as_str());
        }
        if let Some(family) = &generic.fantasy {
            database.set_fantasy_family(family.as_str());
        }
        if let Some(family) = &generic.monospace {
            database.set_monospace_family(family.as_str());
        }
        Ok(database)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::GenericFamilies;

    fn fonts_dir() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests")
            .join("fonts")
    }

    fn tuffy_only() -> FontsConfig {
        FontsConfig {
            system: false,
            files: vec![fonts_dir().join("Tuffy.ttf")],
            ..FontsConfig::default()
        }
    }

    #[test]
    fn face_view_reads_the_face_info() {
        let database = tuffy_only().build_database().unwrap();
        let faces = faces(&database, None);
        assert_eq!(faces.len(), 1);
        let face = &faces[0];
        assert_eq!(face.family, "Tuffy");
        assert_eq!(face.families.first().map(String::as_str), Some("Tuffy"));
        assert_eq!(face.post_script_name, "Tuffy");
        assert_eq!(face.style, FontStyle::Normal);
        // Tuffy.ttf declares usWeightClass 500 (medium).
        assert_eq!(face.weight, 500);
        assert_eq!(face.stretch, FontStretch::Normal);
        assert!(!face.monospaced);
        assert_eq!(
            face.path.as_deref(),
            Some(fonts_dir().join("Tuffy.ttf").as_path())
        );
        assert_eq!(face.index, 0);
    }

    #[test]
    fn in_memory_faces_have_no_path() {
        let mut database = Database::new();
        database.load_font_data(std::fs::read(fonts_dir().join("Tuffy.ttf")).unwrap());
        assert_eq!(faces(&database, None)[0].path, None);
    }

    #[test]
    fn face_serializes_with_css_keywords() {
        let database = tuffy_only().build_database().unwrap();
        let value = serde_json::to_value(&faces(&database, None)[0]).unwrap();
        assert_eq!(value["style"], "normal");
        assert_eq!(value["stretch"], "normal");
        assert_eq!(
            serde_json::to_value(FontStretch::SemiCondensed).unwrap(),
            "semi-condensed"
        );
    }

    #[test]
    fn style_and_stretch_round_trip_through_fontdb() {
        for style in [FontStyle::Normal, FontStyle::Italic, FontStyle::Oblique] {
            assert_eq!(FontStyle::from(Style::from(style)), style);
        }
        for stretch in [
            FontStretch::UltraCondensed,
            FontStretch::ExtraCondensed,
            FontStretch::Condensed,
            FontStretch::SemiCondensed,
            FontStretch::Normal,
            FontStretch::SemiExpanded,
            FontStretch::Expanded,
            FontStretch::ExtraExpanded,
            FontStretch::UltraExpanded,
        ] {
            assert_eq!(FontStretch::from(Stretch::from(stretch)), stretch);
        }
    }

    #[test]
    fn family_filter_ignores_case_and_drops_other_families() {
        let database = tuffy_only().build_database().unwrap();
        assert_eq!(faces(&database, Some("tUFFY")).len(), 1);
        assert_eq!(faces(&database, Some("Nope")), Vec::<Face>::new());
    }

    #[test]
    fn dirs_load_every_font_inside() {
        let config = FontsConfig {
            system: false,
            dirs: vec![fonts_dir()],
            ..FontsConfig::default()
        };
        let database = config.build_database().unwrap();
        assert_eq!(family_names(&database), ["Tuffy"]);
    }

    #[test]
    fn duplicate_families_are_listed_once() {
        let mut config = tuffy_only();
        config.files.push(fonts_dir().join("Tuffy.ttf"));
        let database = config.build_database().unwrap();
        assert_eq!(database.len(), 2);
        assert_eq!(family_names(&database), ["Tuffy"]);
    }

    #[test]
    fn missing_dir_and_file_are_errors_naming_the_path() {
        let missing = fonts_dir().join("missing");
        let config = FontsConfig {
            system: false,
            dirs: vec![missing.clone()],
            ..FontsConfig::default()
        };
        let err = config.build_database().unwrap_err();
        assert!(
            matches!(&err, crate::Error::FontDirNotFound { path } if *path == missing),
            "{err:?}"
        );
        assert!(
            err.to_string().contains(&missing.display().to_string()),
            "{err}"
        );

        let file_config = FontsConfig {
            system: false,
            files: vec![missing.join("x.ttf")],
            ..FontsConfig::default()
        };
        let file_err = file_config.build_database().unwrap_err();
        assert!(
            matches!(file_err, crate::Error::FontFile { .. }),
            "{file_err:?}"
        );
        assert!(file_err.to_string().contains("x.ttf"), "{file_err}");
    }

    #[test]
    fn query_matches_a_listed_family_and_misses_otherwise() {
        let database = tuffy_only().build_database().unwrap();
        let face = FontQuery::new(vec!["Nope".into(), "Tuffy".into()])
            .run(&database)
            .unwrap();
        assert_eq!(face.family, "Tuffy");
        assert_eq!(FontQuery::new(vec!["Nope".into()]).run(&database), None);
        assert_eq!(FontQuery::new(Vec::new()).run(&database), None);
    }

    #[test]
    fn generic_names_resolve_through_the_configured_families() {
        let mut config = tuffy_only();
        assert_eq!(
            FontQuery::new(vec!["serif".into()]).run(&config.build_database().unwrap()),
            None
        );
        config.generic = GenericFamilies {
            serif: Some("Tuffy".into()),
            sans_serif: Some("Tuffy".into()),
            cursive: Some("Tuffy".into()),
            fantasy: Some("Tuffy".into()),
            monospace: Some("Tuffy".into()),
        };
        let database = config.build_database().unwrap();
        for generic in ["serif", "SANS-SERIF", "Cursive", "fantasy", "monospace"] {
            let face = FontQuery::new(vec![generic.into()]).run(&database);
            assert_eq!(
                face.map(|face| face.family).as_deref(),
                Some("Tuffy"),
                "{generic}"
            );
        }
    }
}
