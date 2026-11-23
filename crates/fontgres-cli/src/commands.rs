//! The font commands: `faces`, `families` and `query`.

use anyhow::bail;
use clap::Args;
use fontgres_core::Database;
use fontgres_core::models::{self, Face, FontQuery, FontStretch, FontStyle};

use crate::output::{self, OutputFormat};

/// Arguments of `fontgres faces`.
#[derive(Args)]
pub(crate) struct FacesArgs {
    /// Only list faces of this family (any of its names, case-insensitive).
    #[arg(long)]
    family: Option<String>,
}

/// Arguments of `fontgres query`.
#[derive(Args)]
pub(crate) struct QueryArgs {
    /// Families in priority order. Names match ignoring ASCII case;
    /// `serif`, `sans-serif`, `cursive`, `fantasy` and `monospace` resolve
    /// through the configured generic families.
    #[arg(required = true, value_name = "FAMILY")]
    families: Vec<String>,
    /// Wanted weight: 100 (thin) to 900 (black); 400 is normal, 700 bold.
    #[arg(long, default_value_t = 400, value_parser = clap::value_parser!(u16).range(1..=1000))]
    weight: u16,
    /// Wanted style.
    #[arg(long, value_enum, default_value_t = FontStyle::Normal)]
    style: FontStyle,
    /// Wanted stretch.
    #[arg(long, value_enum, default_value_t = FontStretch::Normal)]
    stretch: FontStretch,
}

/// Table and CSV columns for a face.
const FACE_HEADERS: [&str; 8] = [
    "Family",
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
    let faces = models::faces(database, args.family.as_deref());
    match format {
        OutputFormat::Json => output::print_json(&faces),
        OutputFormat::Table => Ok(output::print_table(&FACE_HEADERS, &face_rows(&faces))?),
        OutputFormat::Csv => Ok(output::print_csv(&FACE_HEADERS, &face_rows(&faces))?),
    }
}

/// Lists the distinct family names in `database`.
pub(crate) fn families(database: &Database, format: OutputFormat) -> anyhow::Result<()> {
    let names = models::family_names(database);
    let rows: Vec<Vec<String>> = names.iter().map(|name| vec![name.clone()]).collect();
    match format {
        OutputFormat::Json => output::print_json(&names),
        OutputFormat::Table => Ok(output::print_table(&["Family"], &rows)?),
        OutputFormat::Csv => Ok(output::print_csv(&["Family"], &rows)?),
    }
}

/// Prints the face that best matches the query, failing when none does.
pub(crate) fn query(
    database: &Database,
    args: &QueryArgs,
    format: OutputFormat,
) -> anyhow::Result<()> {
    let query = FontQuery {
        families: args.families.clone(),
        weight: args.weight,
        style: args.style,
        stretch: args.stretch,
    };
    let Some(face) = query.run(database) else {
        bail!("no face matches {}", args.families.join(", "));
    };
    match format {
        OutputFormat::Json => output::print_json(&face),
        OutputFormat::Table => Ok(output::print_table(&FACE_HEADERS, &face_rows(&[face]))?),
        OutputFormat::Csv => Ok(output::print_csv(&FACE_HEADERS, &face_rows(&[face]))?),
    }
}

/// One table or CSV row per face, in [`FACE_HEADERS`] order.
fn face_rows(faces: &[Face]) -> Vec<Vec<String>> {
    faces
        .iter()
        .map(|face| {
            vec![
                face.family.clone(),
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

    #[test]
    fn keyword_is_the_css_name() {
        assert_eq!(keyword(&FontStretch::ExtraCondensed), "extra-condensed");
        assert_eq!(keyword(&FontStyle::Italic), "italic");
    }

    #[test]
    fn face_rows_follow_the_headers() {
        let face = Face {
            family: "Tuffy".into(),
            families: vec!["Tuffy".into()],
            post_script_name: "Tuffy-Bold".into(),
            style: FontStyle::Oblique,
            weight: 700,
            stretch: FontStretch::Condensed,
            monospaced: true,
            path: None,
            index: 2,
        };
        let rows = face_rows(&[face]);
        assert_eq!(
            rows,
            [[
                "Tuffy",
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
