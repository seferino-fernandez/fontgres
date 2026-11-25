# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.4.0](https://github.com/seferino-fernandez/fontgres/compare/fontgres-mcp-v0.3.0...fontgres-mcp-v0.4.0) - 2026-11-25

### Added

- *(core)* [**breaking**] serializable views and query behind a serde feature ([#22](https://github.com/seferino-fernandez/fontgres/pull/22))

## [0.3.0](https://github.com/seferino-fernandez/fontgres/compare/fontgres-mcp-v0.2.0...fontgres-mcp-v0.3.0) - 2026-11-24

### Added

- *(core)* [**breaking**] give each face its subfamily name and let it settle a tie
- *(mcp)* [**breaking**] one query type, paged listings, a two-module server ([#16](https://github.com/seferino-fernandez/fontgres/pull/16))
- *(core)* [**breaking**] load the config once, strictly, through one startup module
- *(core)* [**breaking**] return a FontLoadReport from every load

### Other

- make the README a landing page and drop the fork wording ([#21](https://github.com/seferino-fernandez/fontgres/pull/21))
- *(mcp)* assert on the schemas the server lists

## [0.2.0](https://github.com/seferino-fernandez/fontgres/compare/fontgres-mcp-v0.1.0...fontgres-mcp-v0.2.0) - 2026-11-23

### Added

- *(core)* [**breaking**] match family names ignoring case

### Other

- *(mcp)* share the oversized-result checks in the budget tests

## [0.1.0]

First release of `fontgres-mcp`, an MCP server that answers questions about the
fonts on a machine over stdio.

### Added

- Read-only tools `fontgres_list_faces`, `fontgres_list_families` and
  `fontgres_query_font`, served from fonts loaded once at startup.
- `--font-dir`, `--font-file` and `--no-system-fonts`, also settable in
  `config.toml` and `FONTGRES_FONTS_*` variables.
- `completions` and `man` subcommands.
- Prebuilt binaries for Linux, macOS and Windows on x86_64 and aarch64, a
  Homebrew formula (`brew install seferino-fernandez/tools/fontgres-mcp`) and
  cargo-binstall support.
