# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.4.0](https://github.com/seferino-fernandez/fontgres/compare/fontgres-core-v0.3.0...fontgres-core-v0.4.0) - 2026-11-25

### Added

- *(core)* [**breaking**] serializable views and query behind a serde feature ([#22](https://github.com/seferino-fernandez/fontgres/pull/22))

## [0.3.0](https://github.com/seferino-fernandez/fontgres/compare/fontgres-core-v0.2.0...fontgres-core-v0.3.0) - 2026-11-24

### Added

- *(core)* [**breaking**] give each face its subfamily name and let it settle a tie
- *(core)* take weight and style from head and the subfamily name
- *(mcp)* [**breaking**] one query type, paged listings, a two-module server ([#16](https://github.com/seferino-fernandez/fontgres/pull/16))
- *(core)* [**breaking**] load the config once, strictly, through one startup module
- *(core)* [**breaking**] return a FontLoadReport from every load

### Fixed

- *(core)* read short post tables, bad name records and version 1 name tables
- *(core)* [**breaking**] load each font file once per database
- *(core)* load a font reached by a link and by its path once per walk

### Other

- make the README a landing page and drop the fork wording ([#21](https://github.com/seferino-fernandez/fontgres/pull/21))
- *(core)* read a face's properties through one face_reader module
- *(core)* [**breaking**] name the generic families with one GenericFamily type
- *(core)* find system fonts by per-platform rules behind Env
- *(core)* [**breaking**] keep faces in load order in an append-only database
- *(core)* parse font files on several threads

## [0.2.0](https://github.com/seferino-fernandez/fontgres/compare/fontgres-core-v0.1.0...fontgres-core-v0.2.0) - 2026-11-23

### Added

- *(core)* [**breaking**] match family names ignoring case

### Fixed

- *(core)* read fontconfig's configuration from /usr/local on FreeBSD
- *(core)* stop scanning /ui/fonts on Redox
- *(core)* load font files by link name and in any extension case
- *(core)* load fonts without a PostScript name
- *(core)* resolve generic families to the first installed fontconfig preference

### Other

- *(core)* keep the font-kit note a comment, outside the matcher's docs
- *(core)* cover the face parsing and matching paths the lint fixes touched
- *(core)* lint the core crate as strictly as the workspace
- lint fontgres-core with its optional features off
- *(core)* move to edition 2024
- *(core)* name the invariants behind the language enum's expects
- *(core)* drop the redundant explicit link targets
- *(core)* bound the font parser's arithmetic and indexing
- *(core)* [**breaking**] take the source to load by reference
- *(core)* fix the manual pedantic lints in the imported code
- *(core)* set the example's log filter in code and drop its unwrap and casts
- *(core)* split CSS font matching into one function per step
- *(core)* apply the fixes clippy suggests for twelve relaxed lints
- *(core)* narrow the font parser items to pub(crate)
- *(core)* pin how a query chooses by stretch, style and weight
- *(core)* share the FileNotFound check in the config tests
- *(core)* read font files through one helper, with one mmap site
- *(core)* read a collection's face offset without LazyArray32
- *(core)* cover parsing the faces of a font collection
- *(core)* say that the mmap sites leave memmap2's contract undischarged
- *(core)* drop provenance from the add_fonts test and the find-system-font example
- *(core)* mark memory-mapped fonts as random access

## [0.1.0]

First release. fontgres-core is forked from fontdb 0.24.0 (commit `ecb707e`);
fontdb's own history up to the fork is in
[FONTDB-CHANGELOG.md](https://github.com/seferino-fernandez/fontgres/blob/e9e11ae7899e80db8fa7cee5413c5a87a699bb1a/crates/fontgres-core/FONTDB-CHANGELOG.md).

### Changed

- Renamed to `fontgres-core`, built as edition 2021.
- `log` records reach the binaries through `tracing`.
- Optional `cli` and `schema` features add the configuration, font views and
  shell helpers the `fontgres` CLI and `fontgres-mcp` server share.
- Documented the whole public API, with runnable examples.

### Removed

- `Database::make_shared_face_data`, the public `unsafe` function.
