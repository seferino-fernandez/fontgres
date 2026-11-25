//! Serializable views of the font database and the query the `fontgres`
//! binaries share.
//!
//! [`FontQuery`] finds the best face for a list of families, and [`faces`]
//! and [`family_names`] list what a database holds. A face comes back as a
//! [`Face`], a view that borrows from the database's [`FaceInfo`] and
//! serializes with CSS keywords, so building one copies nothing and the
//! serialized form does not change when `FaceInfo` does.
//!
//! # Examples
//!
//! List the faces as JSON and find the best face for a query:
//!
//! ```
//! use fontgres_core::Database;
//! use fontgres_core::models::{Face, FontQuery, faces};
//!
//! # let path = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fonts/Tuffy.ttf");
//! let mut db = Database::new();
//! let report = db.load_font_file(path)?;
//! assert!(report.failures().is_empty());
//!
//! let listing: Vec<Face<'_>> = faces(&db, None).into_iter().map(Face::from).collect();
//! let json = serde_json::to_string(&listing)?;
//! assert!(json.contains(r#""post_script_name":"Tuffy""#));
//!
//! let mut query = FontQuery::new(vec!["Missing".into(), "Tuffy".into()]);
//! query.weight = 700;
//! let best = query.run(&db)?;
//! assert_eq!(best.family, "Tuffy");
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```

use core::fmt;
use std::path::Path;

use serde::{Deserialize, Serialize, Serializer};

use crate::{
    Database, FaceInfo, Family, GenericFamily, Language, Query, Source, Stretch, Style, Weight,
};

// The doc comments of `Face` and its fields are the descriptions in the MCP
// tools' output schema, so they describe the JSON. `Face` borrows from the
// `FaceInfo` it was made from, so `Face::from` copies nothing.
/// One font face, as the `fontgres` binaries print and serialize it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct Face<'a> {
    /// The face's primary family name (English US when the font has one).
    pub family: &'a str,
    /// Every family name the face carries, in every language, primary first.
    // Each name carries its language here; the JSON holds the names alone.
    #[serde(serialize_with = "serialize_names")]
    #[cfg_attr(feature = "schema", schemars(with = "Vec<String>"))]
    pub families: &'a [(String, Language)],
    /// The name that tells the face from the others of its family, such as
    /// `Bold Italic`; empty when the font has none.
    pub subfamily: &'a str,
    /// The face's `PostScript` name.
    pub post_script_name: &'a str,
    /// Normal, italic or oblique.
    pub style: Style,
    /// CSS weight, from 1 to 1000; 400 is normal and 700 bold.
    pub weight: u16,
    /// CSS stretch, from `ultra-condensed` to `ultra-expanded`.
    pub stretch: Stretch,
    /// Whether every glyph has the same advance width.
    pub monospaced: bool,
    /// The file the face was loaded from; `None` for in-memory data.
    pub path: Option<&'a Path>,
    /// The face's index within its file (non-zero only in font collections).
    pub index: u32,
}

impl<'a> From<&'a FaceInfo> for Face<'a> {
    /// Returns the view of `info`, which borrows its names and path.
    fn from(info: &'a FaceInfo) -> Self {
        Self {
            family: primary_family(info),
            families: &info.families,
            subfamily: &info.subfamily,
            post_script_name: &info.post_script_name,
            style: info.style,
            weight: info.weight.0,
            stretch: info.stretch,
            monospaced: info.monospaced,
            path: source_path(&info.source),
            index: info.index,
        }
    }
}

/// Serializes a face's family names without their languages.
fn serialize_names<S: Serializer>(
    families: &&[(String, Language)],
    serializer: S,
) -> Result<S::Ok, S::Error> {
    serializer.collect_seq(families.iter().map(|(name, _)| name))
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
/// command line (with the `cli` feature), and the MCP server from a tool
/// call's arguments, where an argument it does not declare is an error.
/// [`run`](Self::run) checks what those readers check, so a query built in
/// code is held to the same rules.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[cfg_attr(feature = "cli", derive(clap::Args))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct FontQuery {
    /// Families in priority order. Names match ignoring ASCII case;
    /// `serif`, `sans-serif`, `cursive`, `fantasy` and `monospace` resolve
    /// through the configured generic families.
    #[cfg_attr(feature = "cli", arg(required = true, value_name = "FAMILY"))]
    #[cfg_attr(feature = "schema", schemars(length(min = 1)))]
    pub families: Vec<String>,
    /// Wanted weight, from 1 to 1000; 400 is normal, 700 bold.
    #[cfg_attr(
        feature = "cli",
        arg(
            long,
            default_value_t = normal_weight(),
            value_parser = clap::value_parser!(u16).range(i64::from(MIN_WEIGHT)..=i64::from(MAX_WEIGHT))
        )
    )]
    #[serde(default = "normal_weight")]
    #[cfg_attr(
        feature = "schema",
        schemars(range(min = MIN_WEIGHT, max = MAX_WEIGHT))
    )]
    pub weight: u16,
    /// Wanted style.
    #[cfg_attr(feature = "cli", arg(long, value_enum, default_value_t = Style::Normal))]
    #[serde(default)]
    pub style: Style,
    /// Wanted stretch.
    #[cfg_attr(feature = "cli", arg(long, value_enum, default_value_t = Stretch::Normal))]
    #[serde(default)]
    pub stretch: Stretch,
}

/// Why a [`FontQuery`] gave no face.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum QueryError {
    /// The query lists no family.
    NoFamilies,
    /// The wanted weight, held here, is outside the CSS range of 1 to 1000.
    WeightOutOfRange(u16),
    /// The database has no face of any family the query lists; they are
    /// held here in the query's order.
    NoMatch(Vec<String>),
}

impl fmt::Display for QueryError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoFamilies => f.write_str("families must name at least one family"),
            Self::WeightOutOfRange(weight) => write!(
                f,
                "weight must be between {MIN_WEIGHT} and {MAX_WEIGHT}, got {weight}"
            ),
            Self::NoMatch(families) => {
                f.write_str("no face matches ")?;
                let mut separator = "";
                for family in families {
                    write!(f, "{separator}{family}")?;
                    separator = ", ";
                }
                Ok(())
            }
        }
    }
}

impl core::error::Error for QueryError {}

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
    /// generic one matches the font's family name ignoring ASCII case. The
    /// face borrows from `database`.
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
    pub fn run<'db>(&self, database: &'db Database) -> Result<Face<'db>, QueryError> {
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

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::path::PathBuf;

    fn fonts_dir() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests")
            .join("fonts")
    }

    /// A database holding only `tests/fonts/Tuffy.ttf`.
    fn tuffy() -> Database {
        let mut database = Database::new();
        let report = database
            .load_font_file(fonts_dir().join("Tuffy.ttf"))
            .unwrap();
        assert_eq!(report.added().len(), 1);
        database
    }

    #[test]
    fn face_view_reads_the_face_info() {
        let database = tuffy();
        let faces = faces(&database, None);
        assert_eq!(faces.len(), 1);
        let face = Face::from(faces[0]);
        assert_eq!(face.family, "Tuffy");
        assert_eq!(
            face.families.first().map(|(name, _)| name.as_str()),
            Some("Tuffy")
        );
        assert_eq!(face.subfamily, "Regular");
        assert_eq!(face.post_script_name, "Tuffy");
        assert_eq!(face.style, Style::Normal);
        // Tuffy.ttf declares usWeightClass 500 (medium).
        assert_eq!(face.weight, 500);
        assert_eq!(face.stretch, Stretch::Normal);
        assert!(!face.monospaced);
        assert_eq!(face.path, Some(fonts_dir().join("Tuffy.ttf").as_path()));
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
        let database = tuffy();
        let value = serde_json::to_value(Face::from(faces(&database, None)[0])).unwrap();
        assert_eq!(value["style"], "normal");
        assert_eq!(value["stretch"], "normal");
        assert_eq!(
            serde_json::to_value(Stretch::SemiCondensed).unwrap(),
            "semi-condensed"
        );
    }

    /// The JSON the CLI's `--output json` and the MCP tools return: snake
    /// case fields, the family names without their languages, and the path
    /// as a string.
    #[test]
    fn face_serializes_to_the_documented_shape() {
        let database = tuffy();
        let path = fonts_dir().join("Tuffy.ttf");
        let value = serde_json::to_value(Face::from(faces(&database, None)[0])).unwrap();
        assert_eq!(
            value,
            json!({
                "family": "Tuffy",
                "families": ["Tuffy"],
                "subfamily": "Regular",
                "post_script_name": "Tuffy",
                "style": "normal",
                "weight": 500,
                "stretch": "normal",
                "monospaced": false,
                "path": path.to_str().unwrap(),
                "index": 0,
            })
        );
        let in_memory = database_of([face("Alpha", 400, Style::Italic, Stretch::Expanded, "a")]);
        let in_memory_value = serde_json::to_value(Face::from(faces(&in_memory, None)[0])).unwrap();
        assert_eq!(in_memory_value["path"], json!(null));
        assert_eq!(in_memory_value["style"], "italic");
        assert_eq!(in_memory_value["stretch"], "expanded");
    }

    #[test]
    fn family_filter_ignores_case_and_drops_other_families() {
        let database = tuffy();
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
            families: vec![(family.to_owned(), Language::English_UnitedStates)],
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
    fn query_matches_a_listed_family_and_misses_otherwise() {
        let database = tuffy();
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
        let database = tuffy();
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
        let database = tuffy();
        let error = FontQuery::new(vec!["Missing".into(), "Absent".into()])
            .run(&database)
            .unwrap_err();
        assert_eq!(error.to_string(), "no face matches Missing, Absent");
        let one = FontQuery::new(vec!["Missing".into()])
            .run(&database)
            .unwrap_err();
        assert_eq!(one.to_string(), "no face matches Missing");
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
    fn generic_names_resolve_through_the_database_families() {
        let mut database = tuffy();
        assert_eq!(
            FontQuery::new(vec!["serif".into()]).run(&database),
            Err(QueryError::NoMatch(vec!["serif".into()]))
        );
        for generic in GenericFamily::ALL {
            database.set_generic_family(generic, "Tuffy");
        }
        for generic in ["serif", "SANS-SERIF", "Cursive", "fantasy", "monospace"] {
            let face = FontQuery::new(vec![generic.into()]).run(&database);
            assert_eq!(face.map(|face| face.family), Ok("Tuffy"), "{generic}");
        }
    }
}
