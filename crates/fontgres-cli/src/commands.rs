//! The font commands: `faces`, `families` and `query`.

use clap::Args;
use fontgres_core::Database;
use fontgres_core::models::{self, Face, FontQuery};

use crate::output::{self, OutputFormat};

/// Arguments of `fontgres faces`.
#[derive(Args)]
pub(crate) struct FacesArgs {
    /// Only list faces of this family (any of its names, case-insensitive).
    #[arg(long)]
    family: Option<String>,
}

/// Table and CSV columns for a face.
const FACE_HEADERS: [&str; 9] = [
    "Family",
    "Subfamily",
    "PostScript name",
    "Weight",
    "Style",
    "Stretch",
    "Monospaced",
    "Path",
    "Index",
];

/// Lists the faces in `database`.
pub(crate) fn faces(
    database: &Database,
    args: &FacesArgs,
    format: OutputFormat,
) -> anyhow::Result<()> {
    let faces: Vec<Face> = models::faces(database, args.family.as_deref())
        .into_iter()
        .map(Face::from)
        .collect();
    output::print(format, &faces, &FACE_HEADERS, || face_rows(&faces))
}

/// Lists the distinct family names in `database`.
pub(crate) fn families(database: &Database, format: OutputFormat) -> anyhow::Result<()> {
    let names = models::family_names(database);
    output::print(format, &names, &["Family"], || {
        names.iter().map(|name| vec![(*name).to_owned()]).collect()
    })
}

/// Prints the face that best matches `query`, failing when none does.
pub(crate) fn query(
    database: &Database,
    query: &FontQuery,
    format: OutputFormat,
) -> anyhow::Result<()> {
    let face = query.run(database)?;
    output::print(format, &face, &FACE_HEADERS, || {
        face_rows(core::slice::from_ref(&face))
    })
}

/// One table or CSV row per face, in [`FACE_HEADERS`] order.
fn face_rows(faces: &[Face]) -> Vec<Vec<String>> {
    faces
        .iter()
        .map(|face| {
            vec![
                face.family.clone(),
                face.subfamily.clone(),
                face.post_script_name.clone(),
                face.weight.to_string(),
                keyword(&face.style),
                keyword(&face.stretch),
                face.monospaced.to_string(),
                face.path
                    .as_ref()
                    .map_or_else(String::new, |path| path.display().to_string()),
                face.index.to_string(),
            ]
        })
        .collect()
}

/// The CSS keyword `value` serializes as, such as `semi-condensed`.
fn keyword(value: &impl serde::Serialize) -> String {
    match serde_json::to_value(value) {
        Ok(serde_json::Value::String(keyword)) => keyword,
        Ok(_) | Err(_) => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use fontgres_core::{Stretch, Style};

    #[test]
    fn keyword_is_the_css_name() {
        assert_eq!(keyword(&Stretch::ExtraCondensed), "extra-condensed");
        assert_eq!(keyword(&Style::Italic), "italic");
    }

    #[test]
    fn face_rows_follow_the_headers() {
        let face = Face {
            family: "Tuffy".into(),
            families: vec!["Tuffy".into()],
            subfamily: "Bold Oblique".into(),
            post_script_name: "Tuffy-Bold".into(),
            style: Style::Oblique,
            weight: 700,
            stretch: Stretch::Condensed,
            monospaced: true,
            path: None,
            index: 2,
        };
        let rows = face_rows(&[face]);
        assert_eq!(
            rows,
            [[
                "Tuffy",
                "Bold Oblique",
                "Tuffy-Bold",
                "700",
                "oblique",
                "condensed",
                "true",
                "",
                "2"
            ]]
        );
        assert_eq!(rows[0].len(), FACE_HEADERS.len());
    }
}
