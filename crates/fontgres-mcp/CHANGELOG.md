# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

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
