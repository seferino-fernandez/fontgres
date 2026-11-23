# fontgres

A fork of [fontdb](https://github.com/RazrFalcon/fontdb), the in-memory font
database with CSS-like queries, with a CLI and an MCP server on top: list the
fonts on a machine and find the face that best matches a CSS-style query, from
a terminal or from an LLM client. (The name is a nod to fontdb, nothing more.)

## Crates

| Crate | Description |
| --- | --- |
| `fontgres-core` | Font database library (forked from fontdb 0.24.0) |
| `fontgres-cli` | Terminal CLI (`fontgres`) |
| `fontgres-mcp` | MCP server (`fontgres-mcp`) for LLM clients |

The CLI and the MCP server both depend on the core library, so font loading,
queries, configuration and errors live in one place:

```text
            fontgres-core  (Database, Config, models, errors)
              /     \
       fontgres-cli   fontgres-mcp
     (clap CLI)   (MCP tools)
```

## Installation

Prebuilt binaries for Linux, macOS and Windows (`x86_64` and `aarch64`) are attached
to each [GitHub Release](https://github.com/seferino-fernandez/fontgres/releases).

With Homebrew (macOS and Linux), which also installs shell completions and man pages:

```shell
brew tap seferino-fernandez/tools
brew install fontgres
brew install fontgres-mcp
```

With cargo-binstall, which downloads the prebuilt binary:

```shell
cargo binstall fontgres-cli
cargo binstall fontgres-mcp
```

From source with cargo:

```shell
cargo install fontgres-cli
cargo install fontgres-mcp
```

## Quick start

Build the project:

```shell
cargo build
```

Run the project's tests:

```shell
cargo test
```

Run a command from the CLI:

```shell
cargo run -p fontgres-cli -- families
cargo run -p fontgres-cli -- query "Helvetica Neue" sans-serif --weight 700
```

## Configuration

Configuration is layered, highest precedence first:
CLI flag, then `FONTGRES_*` env, then `config.toml`, then built-in defaults.
It chooses which fonts load (`[fonts]`: the system's fonts, extra directories
and files, and the families the CSS generic names resolve to) and the log level.
See [`config.toml.example`](config.toml.example) for every setting. Mistakes
fail at startup: an env value that does not parse is an error naming the
variable, a config file named by `--config` or `FONTGRES_CONFIG` must exist, a
font directory or file that is missing is named, and a TOML error reports the
line and column.

## Logging and verbosity flag

Both binaries take `-v`/`--verbose` and `-q`/`--quiet` flags: `-q` silent;
`-v` warn, `-vv` info, `-vvv` debug, `-vvvv` trace. Default level precedence,
highest first:

`RUST_LOG` → `-v`/`-q` (when passed) → `FONTGRES_LOG_LEVEL` / `[log] level` → built-in default

The built-in default is `error` for the `fontgres` CLI and `info` for the
`fontgres-mcp` server. An invalid configured level prints a warning and falls back
to that default. Place the flag before the subcommand: `fontgres -v faces`.
`RUST_LOG` overrides everything (e.g. `RUST_LOG=fontgres_core=trace`). fontdb's
own `log` warnings, such as a file it cannot parse, follow the same level.

## Shell completions

The `fontgres` CLI supports static completions for bash, elvish, fish, nushell,
powershell, and zsh, plus dynamic completions for every shell except nushell.

**Static** - generate a script once and install it where your shell looks for it:

```shell
fontgres completions zsh     > ~/.zsh/completions/_fontgres
```

```shell
fontgres completions bash    | sudo tee /usr/share/bash-completion/completions/fontgres
```

```shell
fontgres completions fish    > ~/.config/fish/completions/fontgres.fish
```

```shell
fontgres completions nushell > ~/.config/nushell/completions/fontgres.nu
```

**Dynamic** - let the binary drive completions at runtime via the `COMPLETE`
environment variable. Add one line to your shell startup file:

```shell
echo 'source <(COMPLETE=zsh fontgres)'  >> ~/.zshrc
```

```shell
echo 'source <(COMPLETE=bash fontgres)' >> ~/.bashrc
```

```shell
echo 'COMPLETE=fish fontgres | source'  >> ~/.config/fish/completions/fontgres.fish
```

## Man pages

The `fontgres` CLI generates ROFF man pages for itself and every subcommand into a
directory (created if missing):

```bash
fontgres man ~/.local/share/man/man1
```

## MCP server

`fontgres-mcp` implements MCP **2026-07-28** (`rmcp 3.x`) and exposes the font
database to LLM clients over `stdio`, for local clients such as Claude Desktop.
It loads its fonts once at startup, from the same configuration and flags as
the CLI, and logs to stderr.

Three read-only tools, `fontgres_list_faces`, `fontgres_list_families` and
`fontgres_query_font`, answer from that snapshot. Tool input schemas are closed
(`additionalProperties: false`), unknown arguments are refused, and tool
results are capped to a configurable byte budget. See the
[`fontgres-mcp` README](crates/fontgres-mcp/README.md) for details.

## Quality guardrails

`just verify` runs every check CI runs: formatting, strict Clippy lints, tests,
docs, dependency policy, spelling, TOML style and the ast-grep rules.
[`docs/quality-guardrails.md`](docs/quality-guardrails.md) lists each tool and
what it catches; [`AGENTS.md`](AGENTS.md) holds the rules for coding agents.

Install the tools once:

```shell
cargo install --locked just cargo-nextest cargo-deny cargo-shear typos-cli ast-grep prek
cargo install --locked cargo-mutants   # optional: `just mutants-diff`
prek install                           # run the fast checks on each commit
```

Tombi installs separately (see [its install guide](https://tombi-toml.github.io/tombi/docs/installation)),
and `just dupes` needs Node.js for `npx`.

## Releasing

Releases are cut by release-plz from `main`; see [docs/releasing.md](docs/releasing.md).

## License

[MIT](LICENSE.md)
