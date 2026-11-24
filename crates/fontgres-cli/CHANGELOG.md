# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.3.0](https://github.com/seferino-fernandez/fontgres/compare/v0.2.0...v0.3.0) - 2026-11-24

### Added

- *(cli)* [**breaking**] print the table as one aligned line per row ([#20](https://github.com/seferino-fernandez/fontgres/pull/20))
- *(core)* [**breaking**] give each face its subfamily name and let it settle a tie
- *(mcp)* [**breaking**] one query type, paged listings, a two-module server ([#16](https://github.com/seferino-fernandez/fontgres/pull/16))
- *(core)* [**breaking**] load the config once, strictly, through one startup module
- *(core)* [**breaking**] return a FontLoadReport from every load

### Fixed

- *(cli)* fail the run when stdout does not take the output

### Other

- make the README a landing page and drop the fork wording ([#21](https://github.com/seferino-fernandez/fontgres/pull/21))
- *(cli)* print a command's result through one buffered stdout writer

## [0.2.0](https://github.com/seferino-fernandez/fontgres/compare/v0.1.0...v0.2.0) - 2026-11-23

### Added

- *(core)* [**breaking**] match family names ignoring case

## [0.1.0]

First release of `fontgres`, a CLI that queries the fonts on a machine.

### Added

- `faces`, `families` and `query` commands, with table, JSON and CSV output.
- `--font-dir`, `--font-file` and `--no-system-fonts` to choose which fonts
  load, also settable in `config.toml` and `FONTGRES_FONTS_*` variables.
- `completions` and `man` subcommands.
- Prebuilt binaries for Linux, macOS and Windows on x86_64 and aarch64, a
  Homebrew formula (`brew install seferino-fernandez/tools/fontgres`) and
  cargo-binstall support.
