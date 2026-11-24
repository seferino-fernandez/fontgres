# fontgres-mcp

MCP server that answers questions about the fonts on a machine, built on
[rmcp](https://github.com/modelcontextprotocol/rust-sdk) 3.x and
[fontgres-core](../fontgres-core/).

Implements the MCP [2026-07-28](https://modelcontextprotocol.io/specification/2026-07-28)
specification, and negotiates down to every earlier version `rmcp` supports.

The server loads its fonts once at startup (the system's installed fonts plus
any configured directories and files) and every tool reads that snapshot.
Restart it to pick up newly installed fonts.

## Install

```shell
brew install seferino-fernandez/tools/fontgres-mcp   # macOS and Linux
cargo binstall fontgres-mcp                          # a prebuilt binary
cargo install fontgres-mcp                           # from source, with Rust 1.100 or newer
```

[Installing fontgres](../../docs/installation.md) has the release archives,
shell completions and man pages.

## Quick start

The server talks to its client over standard input and output, as Claude
Desktop, Claude Code and other local MCP clients start it.

```bash
fontgres-mcp
fontgres-mcp --no-system-fonts --font-dir ~/projects/site/fonts
```

## Tools

Every tool name carries the `fontgres_` prefix, so a client running several MCP
servers can tell this one's tools apart.

| Tool                     | Description                                                        | Read-only |
| ------------------------ | ------------------------------------------------------------------ | --------- |
| `fontgres_list_faces`    | List the loaded faces, a page at a time; `family` narrows to one   | Yes |
| `fontgres_list_families` | List the distinct family names, a page at a time                   | Yes |
| `fontgres_query_font`    | Best face for `families`, `weight`, `style`, `stretch` (CSS-like) | Yes |

Faces carry `family`, `families`, `subfamily` (such as `Bold Italic`),
`post_script_name`, `weight` (CSS 1-1000), `style` (`normal`, `italic`,
`oblique`), `stretch` (`ultra-condensed` ... `ultra-expanded`), `monospaced`,
`path` and `index`. Family names in a query
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

1. **Arguments.** The arguments are read into the tool's parameters. An
   argument the tool does not declare, which its closed input schema rules
   out, is refused with an error that names it and lists the ones the tool
   takes.
1. **Response budget.** The whole serialized result, `content` and
   `structuredContent` together, is capped at `max_response_bytes` (default
   1 MiB). A page of a listing is cut to fit. Any other result over the cap,
   which takes one item larger than the cap itself, is replaced by a small
   `response_too_large` error; if even that cannot fit, the call fails with a
   protocol error. Raise the cap then.

## Configuration

The server reads the same flags, `FONTGRES_*` environment variables and
`~/.config/fontgres/config.toml` as the CLI, and a setting it cannot use keeps
it from starting. [Configuration](../../docs/configuration.md) lists every
setting, and [config.toml.example](../../config.toml.example) is a file to
start from.

- **Tool result cap:** `[mcp] max_response_bytes` or
  `FONTGRES_MCP_MAX_RESPONSE_BYTES`, 1 MiB by default.
- **Logging:** logs go to stderr, at `info` unless a flag or a setting names
  another level.
