# fontgres

Find the fonts installed on a machine and answer CSS-style font queries about
them, from a terminal, an [MCP](https://modelcontextprotocol.io) client or Rust code.

```console
$ fontgres query "Helvetica Neue" sans-serif --weight 700
Family          Subfamily  PostScript name     Weight  Style   Stretch  Monospaced  Path                                     Index
Helvetica Neue  Bold       HelveticaNeue-Bold  700     normal  normal   false       /System/Library/Fonts/HelveticaNeue.ttc  1
```

## What it does, and what it does not

- **One face per query.** `query` takes families in priority order, as CSS
  `font-family` does, with a weight, a style and a stretch. It prints the one
  face that matches best by the CSS font matching rules, and exits 1 when no
  face matches.
- **Listings for people and for programs.** `faces` and `families` print an
  aligned table, JSON or CSV. Logs go to stderr, so stdout is safe to pipe.
- **The same answers for an LLM client.** `fontgres-mcp` serves three
  read-only MCP tools over standard input and output.
- **Nothing else to install.** The binary reads the font files itself and, on
  Linux, fontconfig's configuration for the font directories. It links to no
  font library.
- **No database server, despite the name.** Nothing is stored: each run reads
  the font files again, and the MCP server reads them once when it starts.
- **Not a renderer.** It does not draw, shape or install fonts, and it has no
  font fallback. Only TrueType and OpenType files (`.ttf`, `.otf`, `.ttc`,
  `.otc`) load.
- **A generic name is one family.** `sans-serif` stands for the single family
  [the configuration](docs/configuration.md#generic-families) names, not for
  the list of fonts a browser would try.

## Install

```shell
brew install seferino-fernandez/tools/fontgres   # macOS and Linux
cargo binstall fontgres-cli                      # a prebuilt binary
cargo install fontgres-cli                       # from source, with Rust 1.100 or newer
```

The MCP server is a second binary, `fontgres-mcp`, and installs the same three
ways under that name. [docs/installation.md](docs/installation.md) has the
release archives, shell completions and man pages.

## Usage

```shell
# Every family name
fontgres families

# The faces of one family
fontgres faces --family "Noto Sans"

# The best face for a query: families in priority order, then what is wanted
fontgres query "Helvetica Neue" Arial sans-serif --weight 700 --style italic

# The monospaced families, as JSON for another program
fontgres --output json faces | jq '.[] | select(.monospaced) | .family'

# Only the fonts in a directory
fontgres --no-system-fonts --font-dir ./fonts faces
```

The [`fontgres-cli` README](crates/fontgres-cli/README.md) describes each
command and output format, and [docs/configuration.md](docs/configuration.md)
the config file, the environment variables and logging.

## Crates

| Crate | What it is |
| --- | --- |
| [`fontgres-cli`](crates/fontgres-cli/README.md) | The `fontgres` command |
| [`fontgres-mcp`](crates/fontgres-mcp/README.md) | The `fontgres-mcp` server, for MCP clients such as Claude Desktop |
| [`fontgres-core`](crates/fontgres-core/README.md) | The font database as a Rust library ([API docs](https://docs.rs/fontgres-core)) |

## Status

Early and in active development: 0.x, first released on 2026-11-23. Any 0.x
release can change the CLI's output, the MCP tools' results and the library's
public items; each crate's `CHANGELOG.md` lists what changed. CI runs the
tests on Linux. Binaries are built for Linux, macOS and Windows, on `x86_64`
and `aarch64`.

## Contributing

[CONTRIBUTING.md](CONTRIBUTING.md) covers building, testing and the checks a
change must pass.

## License

[MIT](LICENSE.md). The font database began as a fork of
[fontdb](https://github.com/RazrFalcon/fontdb) by Yevhenii Reizner and its
contributors.
