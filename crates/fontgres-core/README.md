# fontgres-core

An in-memory font database with CSS-like queries: a fork of
[fontdb](https://github.com/RazrFalcon/fontdb) 0.24.0, renamed, and the engine
behind the `fontgres` CLI and the `fontgres-mcp` server.

## The font database

The crate root began as fontdb's API: load fonts from files, directories, raw
data or the system into a `Database`, then list them or match one with a
`Query`. It has since diverged:

- A database only gains faces. `remove_face` is gone; to drop faces, build a
  new database.
- Faces are kept in load order. `faces()` returns them as a slice, an `ID` is a
  face's position in it, and a query that finds several equally good faces
  returns the one loaded first. A directory loads in the order of its file
  names, so that order is the same on every machine.
- `query` returns the face, not its ID, and `family_faces` returns every face
  of a family.
- `Source::File` holds a shared path. `Source::SharedFile`,
  `make_shared_face_data`, `make_face_data_unshared` and `face_source` are
  gone.
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

Cargo features, as in fontdb: `std`, `fs`, `memmap` (memory-maps font files
while parsing) and `fontconfig` (reads fontconfig's directory list on Linux),
all on by default.

## What the binaries share

The `cli` feature adds what the CLI and MCP server have in common:

- `config`: layered configuration (CLI flag -> `FONTGRES_*` env -> `config.toml`
  -> defaults) for which fonts to load (`[fonts] system/dirs/files`), the CSS
  generic families (`[fonts.generic]`) and logging (`[log]`). A bad env value,
  a missing or unreadable `--config`/`FONTGRES_CONFIG` file, invalid TOML and a
  key or table the file does not expect are each an error naming its source.
- `models`: `FontsConfig::build_database`, which fails on a font directory it
  cannot read and on a named font file that gives no face, and logs a warning
  for any other file or face it cannot load; the serializable `Face` view with
  CSS keywords for style and stretch, `FontQuery` (CSS generic names resolve
  through the configured families), and the face and family listings.
- `cli`: what the binaries do the same way at startup: the flags they share,
  the `completions` and `man` subcommands, the one configuration load, and
  the logging setup.

The `schema` feature derives `schemars::JsonSchema` on the views for MCP
output schemas.

## Logging

fontdb logs through the `log` crate (for example, a file it cannot parse).
The binaries install a `tracing` subscriber, whose default `tracing-log`
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
