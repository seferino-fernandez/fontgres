# fontgres-core

An in-memory font database with CSS-like queries, and the engine behind the
`fontgres` CLI and the `fontgres-mcp` server.

## The font database

Load fonts from files, directories, raw data or the system into a
`Database`, then list its faces or match one with a `Query`:

- A database only gains faces. To drop faces, build a new database.
- Faces are kept in load order. `faces()` returns them as a slice, an `ID` is a
  face's position in it, and a query that finds several equally good faces
  returns the one named Regular or Roman if exactly one is, else the one
  loaded first. A directory loads in the order of its file names, so that
  order is the same on every machine.
- `query` returns the face that matches best, and `family_faces` returns
  every face of a family.
- `FaceInfo` has the face's `subfamily` name, such as Bold Italic. Weight and
  style come from the `OS/2` table, then from `head.macStyle` when a face has
  no `OS/2` table, then from the words Bold, Italic and Oblique in the
  subfamily name when the tables say normal. A declared weight of 0 is read as
  400 and one above 1000 as 1000.
- A face's `Source` is the file it came from or the data it was loaded
  from, and the faces of one font file share it.
- Every load returns a `FontLoadReport`: the faces it added, how many font
  files it skipped as already loaded, and each file or face it could not load,
  with the reason. Nothing is logged about them. `load_fonts_dir` returns an
  error when the directory it is given cannot be read.

```rust,no_run
use fontgres_core::{Database, Family, GenericFamily, Query, Weight};

let mut db = Database::new();
let report = db.load_system_fonts();
for failure in report.failures() {
    eprintln!("{failure}");
}
let query = Query {
    families: &[
        Family::Name("Noto Sans"),
        Family::Generic(GenericFamily::SansSerif),
    ],
    weight: Weight::BOLD,
    ..Query::default()
};
if let Some(face) = db.query(&query) {
    assert!(!face.families.is_empty());
}
```

## Cargo features

The defaults give the font database and nothing else. Turn on `serde` to
serialize what it holds, as an app that shows fonts in its UI does; `cli`
and `schema` are for the `fontgres` binaries.

| Feature | Default | What it adds | Crates it adds |
| --- | --- | --- | --- |
| `std` | yes | Links the standard library. Without it the crate is `no_std` with `alloc` and loads fonts only from memory. | none |
| `fs` | yes | Loading font files, directories and the system's fonts, and the `env` module. Implies `std`. | none |
| `memmap` | yes | Memory-maps font files while reading them instead of copying them into memory. Implies `fs`. | `memmap2` |
| `fontconfig` | yes | On Linux and the other Unix systems except macOS, iOS and Android, reads the font directories and generic families from fontconfig's configuration. Implies `fs`. | `fontconfig-parser`, on those systems |
| `serde` | no | The `models` module: the serializable `Face` view, the face and family listings in the binaries' order, and `FontQuery`. `Style` and `Stretch` serialize as CSS keywords. Implies `fs`. | `serde` |
| `cli` | no | What the binaries share (below), and `Style` and `Stretch` as command-line values. Implies `serde`. | `clap` and its completion and man-page crates, `dirs`, `thiserror`, `toml`, `tracing`, `tracing-subscriber` |
| `schema` | no | `schemars::JsonSchema` on the `models` views and query, `Style` and `Stretch`, for MCP tool schemas. Implies `serde`. | `schemars` |

With `serde`, list the faces as JSON and run a query:

```toml
[dependencies]
fontgres-core = { version = "0.4", features = ["serde"] }
```

```rust
use fontgres_core::Database;
use fontgres_core::models::{Face, FontQuery, faces};

let mut db = Database::new();
let _report = db.load_system_fonts();
let listing: Vec<Face<'_>> = faces(&db, None).into_iter().map(Face::from).collect();
let json = serde_json::to_string(&listing)?;
let best = FontQuery::new(vec!["sans-serif".into()]).run(&db);
```

A `Face` borrows from the database, so building the listing copies no
names. A `Database` is `Send` and `Sync`: load it once and share it
across threads.

## What the binaries share

The `cli` feature adds what the CLI and MCP server have in common:

- `config`: layered configuration (CLI flag -> `FONTGRES_*` env -> `config.toml`
  -> defaults) for which fonts to load (`[fonts] system/dirs/files`), the CSS
  generic families (`[fonts.generic]`) and logging (`[log]`). A bad env value,
  a missing or unreadable `--config`/`FONTGRES_CONFIG` file, invalid TOML and a
  key or table the file does not expect are each an error naming its source.
  `FontsConfig::build_database` loads the fonts a configuration names: it fails
  on a font directory it cannot read and on a named font file that gives no
  face, and logs a warning for any other file or face it cannot load.
- `cli`: what the binaries do the same way at startup: the flags they share,
  the `completions` and `man` subcommands, the one configuration load, and
  the logging setup.

Both binaries read their query as `models::FontQuery`, so CSS generic names
resolve through the configured families in each.

## Logging

The database logs through the `log` crate (for example, a fontconfig file
it skips). The binaries install a `tracing` subscriber, whose default `tracing-log`
feature forwards those records, so they follow the binaries' log level under
the `fontgres_core` target.

## Safety

With the `memmap` feature, on by default, font files are memory-mapped while
they are read. The one call that maps them is the only `unsafe` block in the
workspace. If a mapped file is truncated or rewritten during the call that
maps it, the behavior is undefined, and the crate does not prevent it; the
crate docs list the cases under "Memory-mapped files". Build without `memmap`
to read files into memory instead.

## License

[MIT](../../LICENSE.md). fontdb is Copyright (c) 2020 Yevhenii Reizner.
