//! Serializable views of the font database, and the loading and querying the
//! `fontgres` binaries build on them.
//!
//! [`FontsConfig::build_database`](crate::FontsConfig::build_database) loads
//! the fonts a configuration names, [`FontQuery`] is the query both binaries
//! read and finds the best face for a list of families, and [`faces`] and
//! [`family_names`] list what the database holds. A face comes back as a
//! [`Face`], a copy of a [`FaceInfo`] that serializes with CSS keywords, so
//! the serialized form does not change when `FaceInfo` does.

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
    /// The name that tells the face from the others of its family, such as
    /// `Bold Italic`; empty when the font has none.
    pub subfamily: String,
    /// The face's `PostScript` name.
    pub post_script_name: String,
    /// Normal, italic or oblique.
    pub style: Style,
    /// CSS weight, from 1 to 1000; 400 is normal and 700 bold.
    pub weight: u16,
    /// CSS stretch, from `ultra-condensed` to `ultra-expanded`.
    pub stretch: Stretch,
    /// Whether every glyph has the same advance width.
    pub monospaced: bool,
    /// The file the face was loaded from; `None` for in-memory data.
    pub path: Option<PathBuf>,
    /// The face's index within its file (non-zero only in font collections).
    pub index: u32,
}

impl From<&FaceInfo> for Face {
    fn from(info: &FaceInfo) -> Self {
        Self {
            family: primary_family(info).to_owned(),
            families: info.families.iter().map(|(name, _)| name.clone()).collect(),
            subfamily: info.subfamily.clone(),
            post_script_name: info.post_script_name.clone(),
            style: info.style,
            weight: info.weight.0,
            stretch: info.stretch,
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

/// The smallest weight a query may ask for.
const MIN_WEIGHT: u16 = 1;
/// The largest weight a query may ask for.
const MAX_WEIGHT: u16 = 1000;

/// The CSS `normal` weight, which a query asks for unless it names another.
fn normal_weight() -> u16 {
    Weight::NORMAL.0
}

/// A CSS-like font query: the best face for the first family that has one.
///
/// Both binaries read their query as this type: `fontgres query` from the
/// command line, and the MCP server from a tool call's arguments, where an
/// argument it does not declare is an error. [`run`](Self::run) checks what
/// those readers check, so a query built in code is held to the same rules.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, clap::Args)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct FontQuery {
    /// Families in priority order. Names match ignoring ASCII case;
    /// `serif`, `sans-serif`, `cursive`, `fantasy` and `monospace` resolve
    /// through the configured generic families.
    #[arg(required = true, value_name = "FAMILY")]
    #[cfg_attr(feature = "schema", schemars(length(min = 1)))]
    pub families: Vec<String>,
    /// Wanted weight, from 1 to 1000; 400 is normal, 700 bold.
    #[arg(
        long,
        default_value_t = normal_weight(),
        value_parser = clap::value_parser!(u16).range(i64::from(MIN_WEIGHT)..=i64::from(MAX_WEIGHT))
    )]
    #[serde(default = "normal_weight")]
    #[cfg_attr(
        feature = "schema",
        schemars(range(min = MIN_WEIGHT, max = MAX_WEIGHT))
    )]
    pub weight: u16,
    /// Wanted style.
    #[arg(long, value_enum, default_value_t = Style::Normal)]
    #[serde(default)]
    pub style: Style,
    /// Wanted stretch.
    #[arg(long, value_enum, default_value_t = Stretch::Normal)]
    #[serde(default)]
    pub stretch: Stretch,
}

/// Why a [`FontQuery`] gave no face.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum QueryError {
    /// The query lists no family.
    #[error("families must name at least one family")]
    NoFamilies,
    /// The wanted weight, held here, is outside the CSS range of 1 to 1000.
    #[error("weight must be between {min} and {max}, got {0}", min = MIN_WEIGHT, max = MAX_WEIGHT)]
    WeightOutOfRange(u16),
    /// The database has no face of any family the query lists; they are
    /// held here in the query's order.
    #[error("no face matches {}", .0.join(", "))]
    NoMatch(Vec<String>),
}

impl FontQuery {
    /// Creates a query for `families` at normal weight, style and stretch.
    pub fn new(families: Vec<String>) -> Self {
        Self {
            families,
            weight: normal_weight(),
            style: Style::Normal,
            stretch: Stretch::Normal,
        }
    }

    /// Returns the face in `database` that best matches the query.
    ///
    /// Matching follows [`Database::query`], so a family name other than a
    /// generic one matches the font's family name ignoring ASCII case.
    ///
    /// # Errors
    ///
    /// - [`QueryError::NoFamilies`] when the query lists no family.
    /// - [`QueryError::WeightOutOfRange`] when the weight is not from 1 to
    ///   1000.
    /// - [`QueryError::NoMatch`] when no listed family is in the database.
    ///
    /// # Examples
    ///
    /// ```
    /// use fontgres_core::Database;
    /// use fontgres_core::models::{FontQuery, QueryError};
    ///
    /// let mut database = Database::new();
    /// # let path = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fonts/Tuffy.ttf");
    /// assert!(database.load_font_data(std::fs::read(path)?).failures().is_empty());
    /// let mut query = FontQuery::new(vec!["Missing".into(), "Tuffy".into()]);
    /// query.weight = 700;
    /// let face = query.run(&database).expect("Tuffy is loaded");
    /// assert_eq!(face.family, "Tuffy");
    ///
    /// let missing = FontQuery::new(vec!["Missing".into()]).run(&database);
    /// assert_eq!(missing, Err(QueryError::NoMatch(vec!["Missing".into()])));
    /// # Ok::<(), std::io::Error>(())
    /// ```
    pub fn run(&self, database: &Database) -> Result<Face, QueryError> {
        if self.families.is_empty() {
            return Err(QueryError::NoFamilies);
        }
        if !(MIN_WEIGHT..=MAX_WEIGHT).contains(&self.weight) {
            return Err(QueryError::WeightOutOfRange(self.weight));
        }
        let families: Vec<Family<'_>> = self.families.iter().map(|name| family(name)).collect();
        let query = Query {
            families: &families,
            weight: Weight(self.weight),
            stretch: self.stretch,
            style: self.style,
        };
        database
            .query(&query)
            .map(Face::from)
            .ok_or_else(|| QueryError::NoMatch(self.families.clone()))
    }
}

/// Reads `name` as a CSS generic family when it is one (in any case), and as
/// a family name otherwise.
fn family(name: &str) -> Family<'_> {
    GenericFamily::from_css_name(name).map_or(Family::Name(name), Family::Generic)
}

/// The name a face is listed under: its primary family name.
fn primary_family(info: &FaceInfo) -> &str {
    info.families.first().map_or("", |(name, _)| name.as_str())
}

/// Returns the faces in `database` in listing order: sorted by primary
/// family name, then weight, style, stretch and `PostScript` name. Faces
/// equal in all five keep their load order.
///
/// With `family`, returns only the faces that carry that family name in any
/// language, compared ignoring ASCII case.
///
/// The listing borrows from the database. [`Face::from`] copies a face out
/// of it, so a caller that shows one page of the listing copies only that
/// page.
pub fn faces<'a>(database: &'a Database, family: Option<&str>) -> Vec<&'a FaceInfo> {
    let mut faces: Vec<&'a FaceInfo> = match family {
        Some(wanted) => database.family_faces(wanted).collect(),
        None => database.faces().iter().collect(),
    };
    faces.sort_by_key(|info| {
        let info: &'a FaceInfo = info;
        (
            primary_family(info),
            info.weight,
            info.style as u8,
            info.stretch,
            info.post_script_name.as_str(),
        )
    });
    faces
}

/// Returns the distinct primary family names in `database`, sorted.
pub fn family_names(database: &Database) -> Vec<&str> {
    let mut names: Vec<&str> = database
        .faces()
        .iter()
        .filter_map(|info| info.families.first().map(|(name, _)| name.as_str()))
        .collect();
    names.sort_unstable();
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
    use serde_json::json;

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
        let face = Face::from(faces[0]);
        assert_eq!(face.family, "Tuffy");
        assert_eq!(face.families.first().map(String::as_str), Some("Tuffy"));
        assert_eq!(face.subfamily, "Regular");
        assert_eq!(face.post_script_name, "Tuffy");
        assert_eq!(face.style, Style::Normal);
        // Tuffy.ttf declares usWeightClass 500 (medium).
        assert_eq!(face.weight, 500);
        assert_eq!(face.stretch, Stretch::Normal);
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
        assert_eq!(Face::from(faces(&database, None)[0]).path, None);
    }

    #[test]
    fn face_serializes_with_css_keywords() {
        let database = tuffy_only().build_database().unwrap();
        let value = serde_json::to_value(Face::from(faces(&database, None)[0])).unwrap();
        assert_eq!(value["style"], "normal");
        assert_eq!(value["stretch"], "normal");
        assert_eq!(
            serde_json::to_value(Stretch::SemiCondensed).unwrap(),
            "semi-condensed"
        );
    }

    #[test]
    fn family_filter_ignores_case_and_drops_other_families() {
        let database = tuffy_only().build_database().unwrap();
        assert_eq!(faces(&database, Some("tUFFY")).len(), 1);
        assert!(faces(&database, Some("Nope")).is_empty());
    }

    /// A face that was never in a font file, for listings that need faces
    /// Tuffy.ttf does not hold.
    fn face(family: &str, weight: u16, style: Style, stretch: Stretch, name: &str) -> FaceInfo {
        FaceInfo {
            id: crate::ID::dummy(),
            source: Source::Binary(std::sync::Arc::new(Vec::<u8>::new())),
            index: 0,
            families: vec![(family.to_owned(), crate::Language::English_UnitedStates)],
            subfamily: String::new(),
            post_script_name: name.to_owned(),
            style,
            weight: Weight(weight),
            stretch,
            monospaced: false,
        }
    }

    fn database_of(faces: impl IntoIterator<Item = FaceInfo>) -> Database {
        let mut database = Database::new();
        for face in faces {
            database.push_face_info(face);
        }
        database
    }

    fn post_script_names<'a>(faces: &[&'a FaceInfo]) -> Vec<&'a str> {
        faces
            .iter()
            .map(|face| face.post_script_name.as_str())
            .collect()
    }

    /// Each face follows the one before it by one key, and in every key
    /// after that one the earlier face is the larger. A listing that
    /// compared the keys in another order would not give this order.
    #[test]
    fn faces_are_listed_by_family_then_weight_style_stretch_and_postscript_name() {
        let listed = [
            face("Alpha", 700, Style::Oblique, Stretch::Expanded, "z"),
            face("Beta", 400, Style::Oblique, Stretch::Expanded, "z"),
            face("Beta", 700, Style::Normal, Stretch::Expanded, "z"),
            face("Beta", 700, Style::Italic, Stretch::Condensed, "z"),
            face("Beta", 700, Style::Italic, Stretch::Normal, "a"),
            face("Beta", 700, Style::Italic, Stretch::Normal, "b"),
            face("Beta", 700, Style::Oblique, Stretch::UltraCondensed, "a"),
        ];
        // Each face's index is its place in the listing. They load last to
        // first, so load order alone gives the reverse.
        let mut numbered: Vec<FaceInfo> = listed
            .into_iter()
            .zip(0_u32..)
            .map(|(mut info, place)| {
                info.index = place;
                info
            })
            .collect();
        numbered.reverse();
        let database = database_of(numbered);
        let places: Vec<u32> = faces(&database, None)
            .iter()
            .map(|face| face.index)
            .collect();
        assert_eq!(places, [0, 1, 2, 3, 4, 5, 6]);
    }

    #[test]
    fn faces_equal_in_every_listing_key_keep_their_load_order() {
        let mut first = face("Alpha", 400, Style::Normal, Stretch::Normal, "same");
        first.index = 7;
        let second = face("Alpha", 400, Style::Normal, Stretch::Normal, "same");
        let earlier = face("Alpha", 100, Style::Normal, Stretch::Normal, "thin");
        let database = database_of([first, second, earlier]);
        let indexes: Vec<u32> = faces(&database, None)
            .iter()
            .map(|face| face.index)
            .collect();
        assert_eq!(indexes, [0, 7, 0]);
        assert_eq!(
            post_script_names(&faces(&database, None)),
            ["thin", "same", "same"]
        );
    }

    #[test]
    fn a_family_listing_is_sorted_too() {
        let database = database_of([
            face("Beta", 700, Style::Normal, Stretch::Normal, "beta-bold"),
            face("Alpha", 400, Style::Normal, Stretch::Normal, "alpha"),
            face("Beta", 400, Style::Normal, Stretch::Normal, "beta"),
        ]);
        assert_eq!(
            post_script_names(&faces(&database, Some("beta"))),
            ["beta", "beta-bold"]
        );
    }

    #[test]
    fn family_names_are_sorted_and_distinct() {
        let mut nameless = face("", 400, Style::Normal, Stretch::Normal, "nameless");
        nameless.families.clear();
        let database = database_of([
            face("Gamma", 400, Style::Normal, Stretch::Normal, "g"),
            face("Alpha", 400, Style::Normal, Stretch::Normal, "a"),
            nameless,
            face("Gamma", 700, Style::Normal, Stretch::Normal, "g-bold"),
            face("Beta", 400, Style::Normal, Stretch::Normal, "b"),
        ]);
        assert_eq!(family_names(&database), ["Alpha", "Beta", "Gamma"]);
        // A face without a family name is listed, under the empty name.
        assert_eq!(
            Face::from(faces(&database, None)[0]).post_script_name,
            "nameless"
        );
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
        assert_eq!(
            FontQuery::new(vec!["Nope".into(), "Absent".into()]).run(&database),
            Err(QueryError::NoMatch(vec!["Nope".into(), "Absent".into()]))
        );
    }

    /// The checks the command line and the tool schema make are made again
    /// by `run`, with the messages both binaries print.
    #[test]
    fn query_refuses_no_family_and_a_weight_outside_the_css_range() {
        let database = tuffy_only().build_database().unwrap();
        let none = FontQuery::new(Vec::new()).run(&database).unwrap_err();
        assert_eq!(none, QueryError::NoFamilies);
        assert_eq!(none.to_string(), "families must name at least one family");

        let with_weight = |weight| FontQuery {
            weight,
            ..FontQuery::new(vec!["Tuffy".into()])
        };
        for (weight, message) in [
            (0, "weight must be between 1 and 1000, got 0"),
            (1001, "weight must be between 1 and 1000, got 1001"),
        ] {
            let error = with_weight(weight).run(&database).unwrap_err();
            assert_eq!(error, QueryError::WeightOutOfRange(weight));
            assert_eq!(error.to_string(), message);
        }
        for weight in [1, 1000] {
            let face = with_weight(weight).run(&database).unwrap();
            assert_eq!(face.family, "Tuffy", "{weight}");
        }
    }

    #[test]
    fn a_query_without_a_match_names_the_families_it_tried() {
        let database = tuffy_only().build_database().unwrap();
        let error = FontQuery::new(vec!["Missing".into(), "Absent".into()])
            .run(&database)
            .unwrap_err();
        assert_eq!(error.to_string(), "no face matches Missing, Absent");
    }

    #[test]
    fn query_denies_unknown_fields() {
        let message = serde_json::from_value::<FontQuery>(json!({ "families": ["A"], "extra": 1 }))
            .expect_err("an unknown field is refused")
            .to_string();
        assert!(message.contains("unknown field `extra`"), "{message}");
    }

    #[test]
    fn query_defaults_to_a_normal_face() {
        let query: FontQuery = serde_json::from_value(json!({ "families": ["Tuffy"] })).unwrap();
        assert_eq!(query.weight, 400);
        assert_eq!(query.style, Style::Normal);
        assert_eq!(query.stretch, Stretch::Normal);
        assert_eq!(query, FontQuery::new(vec!["Tuffy".into()]));
    }

    #[test]
    fn query_reads_css_keywords() {
        let query: FontQuery = serde_json::from_value(json!({
            "families": ["Tuffy"], "style": "oblique", "stretch": "semi-expanded"
        }))
        .unwrap();
        assert_eq!(query.style, Style::Oblique);
        assert_eq!(query.stretch, Stretch::SemiExpanded);
    }

    #[cfg(feature = "schema")]
    #[test]
    fn query_schema_bounds_weight_and_requires_a_family() {
        let schema = serde_json::to_value(schemars::schema_for!(FontQuery)).unwrap();
        let properties = &schema["properties"];
        assert_eq!(properties["weight"]["minimum"], json!(1));
        assert_eq!(properties["weight"]["maximum"], json!(1000));
        assert_eq!(properties["families"]["minItems"], json!(1));
        assert_eq!(schema["required"], json!(["families"]));
    }

    #[test]
    fn generic_names_resolve_through_the_configured_families() {
        let mut config = tuffy_only();
        assert_eq!(
            FontQuery::new(vec!["serif".into()]).run(&config.build_database().unwrap()),
            Err(QueryError::NoMatch(vec!["serif".into()]))
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
                Ok("Tuffy"),
                "{generic}"
            );
        }
    }
}
