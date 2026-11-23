# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.1.0]

First release. fontgres-core is forked from fontdb 0.24.0 (commit `ecb707e`);
fontdb's own history up to the fork is in
[FONTDB-CHANGELOG.md](https://github.com/seferino-fernandez/fontgres/blob/main/crates/fontgres-core/FONTDB-CHANGELOG.md).

### Changed

- Renamed to `fontgres-core`, built as edition 2021.
- `log` records reach the binaries through `tracing`.
- Optional `cli` and `schema` features add the configuration, font views and
  shell helpers the `fontgres` CLI and `fontgres-mcp` server share.
- Documented the whole public API, with runnable examples.

### Removed

- `Database::make_shared_face_data`, the public `unsafe` function.
