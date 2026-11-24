# fontgres-cli

List the fonts installed on this machine and find the face that best matches a
CSS-style query, from the terminal. Built on [fontgres-core](../fontgres-core/).

## Install

```shell
brew install seferino-fernandez/tools/fontgres   # macOS and Linux
cargo binstall fontgres-cli                      # a prebuilt binary
cargo install fontgres-cli                       # from source, with Rust 1.100 or newer
```

[Installing fontgres](../../docs/installation.md) has the release archives,
shell completions and man pages.

## Commands

| Command    | Description                                                              |
| ---------- | ------------------------------------------------------------------------ |
| `faces`    | List font faces (`--family` narrows to one family, case-insensitive).    |
| `families` | List the distinct family names.                                          |
| `query`    | Print the face that best matches a CSS-like query; exit 1 if none does. |

Each command loads the system's fonts, plus any `--font-dir` (scanned
recursively) and `--font-file`; `--no-system-fonts` skips the system fonts.
A `--font-file` must hold a font, and it is an error when it does not. A file
in a directory that cannot be loaded is skipped with a warning, which `-v`
shows.

```shell
fontgres families
fontgres faces --family "Noto Sans"
fontgres query "Helvetica Neue" Arial sans-serif --weight 700 --style italic
fontgres --no-system-fonts --font-dir ./fonts faces
```

`query` takes families in priority order, as CSS `font-family` does. Names
match ignoring ASCII case; the generic names `serif`,
`sans-serif`, `cursive`, `fantasy` and `monospace` resolve through
`[fonts.generic]`. `--weight` is 1-1000 (400 normal, 700 bold), `--style` is
`normal`, `italic` or `oblique`, and `--stretch` runs from `ultra-condensed` to
`ultra-expanded`.

## Output formats

`--output table|json|csv` (default: `table`) is a global flag accepted by every command.
Logs go to stderr, so stdout carries only structured data when `--output json` or
`--output csv` is active, which makes it safe to pipe.

```shell
fontgres --output json faces | jq '.[] | select(.monospaced) | .family'
fontgres --output csv faces > faces.csv
```

The table is a header line and then one line per row, with the columns aligned
and no cell wrapped or cut:

```text
$ fontgres query Helvetica
Family     Subfamily  PostScript name  Weight  Style   Stretch  Monospaced  Path                                 Index
Helvetica  Regular    Helvetica        400     normal  normal   false       /System/Library/Fonts/Helvetica.ttc  0
```

A listing of faces is wider than most terminals, and `fontgres faces | less -S`
scrolls it sideways. A control character in a name, such as a line break,
shows as its escape (`\n`) in the table; JSON and CSV carry the name as it is.

## Configuration

Each setting comes from a flag, then a `FONTGRES_*` environment variable, then
`~/.config/fontgres/config.toml`, then a built-in default.
[Configuration](../../docs/configuration.md) lists every setting, the generic
families and the log levels, and
[config.toml.example](../../config.toml.example) is a file to start from.
