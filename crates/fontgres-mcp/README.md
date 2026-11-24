# fontgres-mcp

MCP server that answers questions about the fonts on a machine, built on
[rmcp](https://github.com/modelcontextprotocol/rust-sdk) 3.x and
[fontgres-core](../fontgres-core/), a fork of
[fontdb](https://github.com/RazrFalcon/fontdb).

Implements the MCP [2026-07-28](https://modelcontextprotocol.io/specification/2026-07-28)
specification, and negotiates down to every earlier version `rmcp` supports.

The server loads its fonts once at startup (the system's installed fonts plus
any configured directories and files) and every tool reads that snapshot.
Restart it to pick up newly installed fonts.

## Install

`brew install seferino-fernandez/tools/fontgres-mcp` (Homebrew, with shell completions
and man pages), `cargo binstall fontgres-mcp` (prebuilt binary) or
`cargo install fontgres-mcp` (from source). Archives for Linux, macOS and Windows
are on [GitHub Releases](https://github.com/seferino-fernandez/fontgres/releases).

## Transports

- `stdio` for Claude Desktop, Claude Code, and other local MCP clients

## Quick start

```bash
fontgres-mcp --transport stdio
fontgres-mcp --no-system-fonts --font-dir ~/projects/site/fonts
```

## Logging verbosity

`-v`/`--verbose` and `-q`/`--quiet` adjust the log level (`-q` silent; `-v` warn,
`-vv` info, `-vvv` debug, `-vvvv` trace). Default level precedence, highest first:

`RUST_LOG` -> `-v`/`-q` (when passed) -> `FONTGRES_LOG_LEVEL` / config `log.level` -> default (`info`)

So `fontgres-mcp -q --transport stdio` quiets a run without touching config, while
config / `FONTGRES_LOG_LEVEL` still apply when no flag is given. An invalid
configured level keeps the server from starting. Logs go to stderr,
including the warning for a font file or face that cannot be loaded.

## Tools

Every tool name carries the `fontgres_` prefix, so a client running several MCP
servers can tell this one's tools apart.

| Tool                     | Description                                                        | Read-only |
| ------------------------ | ------------------------------------------------------------------ | --------- |
| `fontgres_list_faces`    | List the loaded faces, a page at a time; `family` narrows to one   | Yes |
| `fontgres_list_families` | List the distinct family names, a page at a time                   | Yes |
| `fontgres_query_font`    | Best face for `families`, `weight`, `style`, `stretch` (CSS-like) | Yes |

Faces carry `family`, `families`, `post_script_name`, `weight` (CSS 1-1000),
`style` (`normal`, `italic`, `oblique`), `stretch` (`ultra-condensed` ...
`ultra-expanded`), `monospaced`, `path` and `index`. Family names in a query
match ignoring ASCII case; the generic names `serif`, `sans-serif`,
`cursive`, `fantasy` and `monospace` resolve through `[fonts.generic]`.

### Paged listings

Both list tools take `offset` and `limit` and return one page:

```json
{ "items": ["Arial", "Avenir"], "total": 418, "next_offset": 2 }
```

- `total` counts every match, on this page and every other.
- `next_offset` is the `offset` that gets the next page, and `null` on the
  last one. An `offset` past the end gives an empty last page.
- `limit` is the most items in a page: 100 faces or 1,000 family names unless
  the call gives another. A page holds fewer when more would exceed the
  response budget, so follow `next_offset` instead of counting items.
- Each call sorts the fonts loaded at startup, so pages do not shift between
  calls.

### Tool calls

Every call goes through the same checks, in order:

1. **Arguments.** An argument the tool's input schema does not declare is
   refused. Every params struct also denies unknown fields.
1. **Response budget.** The whole serialized result, `content` and
   `structuredContent` together, is capped at `max_response_bytes` (default
   1 MiB). A page of a listing is cut to fit. Any other result over the cap,
   which takes one item larger than the cap itself, is replaced by a small
   `response_too_large` error; if even that cannot fit, the call fails with a
   protocol error. Raise the cap then.

## Shell completions

`fontgres-mcp` supports static completions for bash, elvish, fish, nushell,
powershell, and zsh, plus dynamic completions for every shell except nushell.

Static: generate a script and install it where your shell looks for it

```shell
fontgres-mcp completions zsh > ~/.zsh/completions/_fontgres-mcp
```

```shell
fontgres-mcp completions nushell > ~/.config/nushell/completions/fontgres-mcp.nu
```

Dynamic: let the binary drive completions at runtime (re-source after upgrades)

```shell
echo 'source <(COMPLETE=zsh fontgres-mcp)' >> ~/.zshrc
```

Running `fontgres-mcp` with no subcommand still starts the server as usual.

## Man pages

Generate ROFF man pages for the server and every subcommand into a directory
(created if missing):

```shell
fontgres-mcp man ./man
man -l ./man/fontgres-mcp.1
```

## Configuration

Config file default: `~/.config/fontgres/config.toml` (or `--config` / `FONTGRES_CONFIG`).
A file named by `--config` or `FONTGRES_CONFIG` must exist; the default may be
absent.

Each setting comes from the highest layer that sets it: CLI flag, then
`FONTGRES_*` env, then `config.toml`. A list (font directories or files) is
taken whole from that layer. A value that does not parse fails at startup
naming its source: an env var by name and raw value, a TOML key by file, line
and column. A key or table the file does not expect fails the same way,
`[mcp]` keys included.

| Flag / Variable                                      | Description                                        |
| ---------------------------------------------------- | -------------------------------------------------- |
| `--no-system-fonts` / `FONTGRES_FONTS_SYSTEM`        | Load the system's installed fonts (default: `true`) |
| `--font-dir` / `FONTGRES_FONTS_DIRS`                 | Extra directories, scanned recursively             |
| `--font-file` / `FONTGRES_FONTS_FILES`               | Extra font files                                   |
| `FONTGRES_FONTS_GENERIC_SERIF` (and `_SANS_SERIF`, ...) | Family a CSS generic name resolves to              |
| `FONTGRES_MCP_MAX_RESPONSE_BYTES`                    | Tool result size cap in bytes (default: 1 MiB)     |

See [config.toml.example](../../config.toml.example) for all available fields.
