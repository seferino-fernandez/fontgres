//! Serializable views of the font database, and the loading and querying the
//! `fontgres` binaries build on them.
//!
//! [`FontsConfig::build_database`](crate::FontsConfig::build_database) loads
//! the fonts a configuration names, [`FontQuery`] finds the best face for a
//! list of families, and [`faces`] and [`family_names`] list what the
//! database holds. A face comes back as a [`Face`], a copy of a
//! [`FaceInfo`] that serializes with CSS keywords, so the
//! serialized form does not change when `FaceInfo` does.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::config::FontsConfig;
use crate::{
    Database, FaceInfo, Family, FontLoadReport, GenericFamily, Query, Source, Stretch, Style,
    Weight,
};

/// One font face, as the `fontgres` binaries print and serialize it.
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
    /// CSS weight as the font declares it, normally 1 to 1000; 400 is normal
    /// and 700 bold.
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
    /// Creates a query for `families` at normal weight, style and stretch.
    pub fn new(families: Vec<String>) -> Self {
        Self {
            families,
            weight: Weight::NORMAL.0,
            style: FontStyle::Normal,
            stretch: FontStretch::Normal,
        }
    }

    /// Returns the face in `database` that best matches the query, or `None`
    /// when no listed family is in the database.
    ///
    /// Matching follows [`Database::query`], so a family name other than a
    /// generic one matches the font's family name ignoring ASCII case.
    ///
    /// # Examples
    ///
    /// ```
    /// use fontgres_core::Database;
    /// use fontgres_core::models::FontQuery;
    ///
    /// let mut database = Database::new();
    /// # let path = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fonts/Tuffy.ttf");
    /// assert!(database.load_font_data(std::fs::read(path)?).failures().is_empty());
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
        database.query(&query).map(Face::from)
    }
}

/// Reads `name` as a CSS generic family when it is one (in any case), and as
/// a family name otherwise.
fn family(name: &str) -> Family<'_> {
    GenericFamily::from_css_name(name).map_or(Family::Name(name), Family::Generic)
}

/// Returns the faces in `database`, sorted by family, then weight, style,
/// stretch and `PostScript` name.
///
/// With `family`, returns only the faces that carry that family name in any
/// language, compared ignoring ASCII case.
pub fn faces(database: &Database, family: Option<&str>) -> Vec<Face> {
    let mut faces: Vec<Face> = match family {
        Some(wanted) => database.family_faces(wanted).map(Face::from).collect(),
        None => database.faces().iter().map(Face::from).collect(),
    };
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

/// Returns the distinct primary family names in `database`, sorted.
pub fn family_names(database: &Database) -> Vec<String> {
    let mut names: Vec<String> = database
        .faces()
        .iter()
        .filter_map(|info| info.families.first().map(|(name, _)| name.clone()))
        .collect();
    names.sort();
    names.dedup();
    names
}

/// Logs each font file and face that a load could not add.
fn warn_of_failures(report: &FontLoadReport) {
    for failure in report.failures() {
        tracing::warn!("{failure}");
    }
}

impl FontsConfig {
    /// Builds a database holding the fonts this configuration names: the
    /// system fonts when [`system`](Self::system) is set, then each directory
    /// and file, with the generic families applied.
    ///
    /// A font file or face that a load could not add is skipped and logged
    /// as a warning, except in a file named in [`files`](Self::files) that
    /// gives no face, which is an error. A named file that an earlier
    /// directory, or an earlier entry of `files`, already loaded is not
    /// loaded again and is not an error.
    ///
    /// # Errors
    ///
    /// - [`Error::FontDir`](crate::Error::FontDir) when a directory cannot be
    ///   read: it does not exist, is not a directory, or may not be listed.
    /// - [`Error::FontFile`](crate::Error::FontFile) when a file cannot be
    ///   read.
    /// - [`Error::NoFaces`](crate::Error::NoFaces) when a file is read and
    ///   gives no face, as a file that is not a font does.
    pub fn build_database(&self) -> crate::Result<Database> {
        let mut database = Database::new();
        if self.system {
            warn_of_failures(&database.load_system_fonts());
        }
        for dir in &self.dirs {
            let report = database
                .load_fonts_dir(dir)
                .map_err(|source| crate::Error::FontDir {
                    path: dir.clone(),
                    source,
                })?;
            warn_of_failures(&report);
        }
        for path in &self.files {
            let report =
                database
                    .load_font_file(path)
                    .map_err(|source| crate::Error::FontFile {
                        path: path.clone(),
                        source,
                    })?;
            if report.added().len() == 0 && report.skipped() == 0 {
                let first = report.into_failures().into_iter().next();
                return Err(crate::Error::NoFaces {
                    path: path.clone(),
                    reason: first.map(|failure| failure.reason),
                });
            }
            warn_of_failures(&report);
        }
        for generic in GenericFamily::ALL {
            if let Some(family) = self.generic.get(generic) {
                database.set_generic_family(generic, family.as_str());
            }
        }
        Ok(database)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::GenericFamilies;

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
        let report = database.load_font_data(std::fs::read(fonts_dir().join("Tuffy.ttf")).unwrap());
        assert_eq!(report.added().len(), 1);
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
    fn the_same_dir_given_twice_loads_its_fonts_once() {
        let once = FontsConfig {
            system: false,
            dirs: vec![fonts_dir()],
            ..FontsConfig::default()
        };
        let twice = FontsConfig {
            dirs: vec![fonts_dir(), fonts_dir()],
            ..once.clone()
        };
        assert_eq!(
            twice.build_database().unwrap().len(),
            once.build_database().unwrap().len()
        );
    }

    #[test]
    fn duplicate_families_are_listed_once() {
        let copies = tempfile::tempdir().unwrap();
        let copy = copies.path().join("Tuffy.ttf");
        std::fs::copy(fonts_dir().join("Tuffy.ttf"), &copy).unwrap();
        let mut config = tuffy_only();
        config.files.push(copy);
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
            matches!(
                &err,
                crate::Error::FontDir { path, source }
                    if *path == missing && source.kind() == std::io::ErrorKind::NotFound
            ),
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

    /// A font file given where a directory belongs is not a directory that
    /// can be read.
    #[test]
    fn a_file_given_as_a_dir_is_an_error() {
        let file = fonts_dir().join("Tuffy.ttf");
        let config = FontsConfig {
            system: false,
            dirs: vec![file.clone()],
            ..FontsConfig::default()
        };
        let err = config.build_database().unwrap_err();
        assert!(
            matches!(&err, crate::Error::FontDir { path, .. } if *path == file),
            "{err:?}"
        );
    }

    /// A directory holding a font and `not-a-font.ttf`, which is not one.
    fn dir_with_a_file_that_is_not_a_font() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        std::fs::copy(fonts_dir().join("Tuffy.ttf"), dir.path().join("Tuffy.ttf")).unwrap();
        std::fs::write(dir.path().join("not-a-font.ttf"), "plain text").unwrap();
        dir
    }

    /// A directory may hold files that are not fonts; a file named on its own
    /// may not be one.
    #[test]
    fn a_file_that_is_not_a_font_is_an_error_only_when_named() {
        let dir = dir_with_a_file_that_is_not_a_font();
        let not_a_font = dir.path().join("not-a-font.ttf");
        let in_a_dir = FontsConfig {
            system: false,
            dirs: vec![dir.path().to_path_buf()],
            ..FontsConfig::default()
        };
        assert_eq!(in_a_dir.build_database().unwrap().len(), 1);

        let named = FontsConfig {
            system: false,
            files: vec![not_a_font.clone()],
            ..FontsConfig::default()
        };
        let err = named.build_database().unwrap_err();
        assert!(
            matches!(
                &err,
                crate::Error::NoFaces {
                    path,
                    reason: Some(crate::FontLoadFailureReason::MalformedFont),
                } if *path == not_a_font
            ),
            "{err:?}"
        );
        assert_eq!(
            err.to_string(),
            format!("no font faces in {}: malformed font", not_a_font.display())
        );
    }

    /// Runs `build` and returns what it logs, as the binaries' subscriber
    /// formats it.
    fn logged_by(build: impl FnOnce()) -> String {
        use std::sync::{Arc, Mutex};

        struct Log(Arc<Mutex<Vec<u8>>>);

        impl std::io::Write for Log {
            fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
                let mut log = self
                    .0
                    .lock()
                    .map_err(|_poisoned| std::io::ErrorKind::Other)?;
                log.extend_from_slice(bytes);
                Ok(bytes.len())
            }

            fn flush(&mut self) -> std::io::Result<()> {
                Ok(())
            }
        }

        let log = Arc::new(Mutex::new(Vec::new()));
        let writer = Arc::clone(&log);
        let subscriber = tracing_subscriber::fmt()
            .with_ansi(false)
            .with_writer(move || Log(Arc::clone(&writer)))
            .finish();
        tracing::subscriber::with_default(subscriber, build);
        let bytes = log.lock().unwrap().clone();
        String::from_utf8(bytes).unwrap()
    }

    /// A file a directory load cannot load is logged as a warning that names
    /// it. A named file that gives no face is the error, and is not logged as
    /// well.
    #[test]
    fn a_failure_is_logged_as_a_warning_unless_it_is_the_error() {
        let dir = dir_with_a_file_that_is_not_a_font();
        let not_a_font = dir.path().join("not-a-font.ttf");
        let warning = format!(
            "cannot load face 0 of {}: malformed font",
            not_a_font.display()
        );

        let in_a_dir = FontsConfig {
            system: false,
            dirs: vec![dir.path().to_path_buf()],
            ..FontsConfig::default()
        };
        let dir_log = logged_by(|| {
            in_a_dir.build_database().unwrap();
        });
        assert!(
            dir_log.contains("WARN") && dir_log.contains(&warning),
            "{dir_log}"
        );

        let named = FontsConfig {
            system: false,
            files: vec![not_a_font],
            ..FontsConfig::default()
        };
        let named_log = logged_by(|| {
            named.build_database().unwrap_err();
        });
        assert_eq!(named_log, "");
    }

    /// The directory load meets the file first and skips it with a warning;
    /// naming the file is still an error, with the reason.
    #[test]
    fn a_named_file_that_its_dir_could_not_load_is_still_an_error() {
        let dir = dir_with_a_file_that_is_not_a_font();
        let not_a_font = dir.path().join("not-a-font.ttf");
        let config = FontsConfig {
            system: false,
            dirs: vec![dir.path().to_path_buf()],
            files: vec![not_a_font.clone()],
            ..FontsConfig::default()
        };
        let err = config.build_database().unwrap_err();
        assert_eq!(
            err.to_string(),
            format!("no font faces in {}: malformed font", not_a_font.display())
        );
    }

    /// A named file that adds no face because it is already loaded is not an
    /// error: its directory loaded it, or it is named twice.
    #[test]
    fn a_named_file_that_is_already_loaded_is_not_an_error() {
        let dir = dir_with_a_file_that_is_not_a_font();
        let font = dir.path().join("Tuffy.ttf");
        let config = FontsConfig {
            system: false,
            dirs: vec![dir.path().to_path_buf()],
            files: vec![font.clone(), font],
            ..FontsConfig::default()
        };
        assert_eq!(config.build_database().unwrap().len(), 1);

        let mut twice = tuffy_only();
        twice.files.extend(tuffy_only().files);
        assert_eq!(twice.build_database().unwrap().len(), 1);
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
