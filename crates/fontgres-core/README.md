# fontgres-core

An in-memory font database with CSS-like queries: a fork of
[fontdb](https://github.com/RazrFalcon/fontdb) 0.24.0, renamed, and the engine
behind the `fontgres` CLI and the `fontgres-mcp` server.

## The font database

The crate root is fontdb's API, unchanged apart from the removal of
`Database::make_shared_face_data`: load fonts from files, directories, raw data
or the system into a `Database`, then list them or match one with a `Query`.

```rust,no_run
use fontgres_core::{Database, Family, Query, Weight};

let mut db = Database::new();
db.load_system_fonts();
let query = Query {
    families: &[Family::Name("Noto Sans"), Family::SansSerif],
    weight: Weight::BOLD,
    ..Query::default()
};
if let Some(id) = db.query(&query) {
    let face = db.face(id).expect("a matched id is in the database");
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
  a missing `--config`/`FONTGRES_CONFIG` file or invalid TOML is an error naming
  its source.
- `models`: `FontsConfig::build_database`, the serializable `Face` view with
  CSS keywords for style and stretch, `FontQuery` (CSS generic names resolve
  through the configured families), and the face and family listings.
- `cli`: the `completions` and `man` subcommand helpers.

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
