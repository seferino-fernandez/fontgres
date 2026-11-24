//! Loads the system fonts and times a query.

#[cfg(feature = "fs")]
use fontgres_core::{Database, Family, GenericFamily, Query, Source, Weight};

#[cfg(not(feature = "fs"))]
fn main() {}

#[cfg(feature = "fs")]
#[expect(
    clippy::print_stdout,
    reason = "the example reports what it found and how long it took on stdout"
)]
fn main() {
    const FAMILY_NAME: &str = "Times New Roman";

    tracing_subscriber::fmt()
        .with_env_filter("fontgres_core=trace")
        .init();

    let mut db = Database::new();
    let load_start = std::time::Instant::now();
    let report = db.load_system_fonts();
    db.set_generic_family(GenericFamily::SansSerif, "Arial");
    println!(
        "Loaded {} font faces in {}ms.",
        db.len(),
        load_start.elapsed().as_millis()
    );
    for failure in report.failures() {
        println!("Warning: {failure}.");
    }

    let query = Query {
        families: &[
            Family::Name(FAMILY_NAME),
            Family::Generic(GenericFamily::SansSerif),
        ],
        weight: Weight::BOLD,
        ..Query::default()
    };

    let query_start = std::time::Instant::now();
    let Some(face) = db.query(&query) else {
        println!("Error: '{FAMILY_NAME}' not found.");
        return;
    };
    if let Source::File(path) = &face.source {
        println!(
            "Font '{}':{} found in {}ms.",
            path.display(),
            face.index,
            query_start.elapsed().as_secs_f64() * 1000.0
        );
    }
}
