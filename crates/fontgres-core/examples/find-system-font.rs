//! Loads the system fonts and times a query.

use fontgres_core as fontdb;

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

    let mut db = fontdb::Database::new();
    let load_start = std::time::Instant::now();
    db.load_system_fonts();
    db.set_serif_family("Times New Roman");
    db.set_sans_serif_family("Arial");
    db.set_cursive_family("Comic Sans MS");
    db.set_fantasy_family("Impact");
    db.set_monospace_family("Courier New");
    println!(
        "Loaded {} font faces in {}ms.",
        db.len(),
        load_start.elapsed().as_millis()
    );

    let query = fontdb::Query {
        families: &[fontdb::Family::Name(FAMILY_NAME), fontdb::Family::SansSerif],
        weight: fontdb::Weight::BOLD,
        ..fontdb::Query::default()
    };

    let query_start = std::time::Instant::now();
    let Some(id) = db.query(&query) else {
        println!("Error: '{FAMILY_NAME}' not found.");
        return;
    };
    if let Some((fontdb::Source::File(path), index)) = db.face_source(id) {
        println!(
            "Font '{}':{} found in {}ms.",
            path.display(),
            index,
            query_start.elapsed().as_secs_f64() * 1000.0
        );
    }
}
