# fontgres-cli

Query the fonts on this machine from the terminal, built on
[fontgres-core](../fontgres-core/), a fork of [fontdb](https://github.com/RazrFalcon/fontdb).

## Build

```shell
cargo build --release -p fontgres-cli
```

The binary is written to `target/release/fontgres`. Add it to your `PATH` or invoke it directly.

## Commands

| Command    | Description                                                              |
| ---------- | ------------------------------------------------------------------------ |
| `faces`    | List font faces (`--family` narrows to one family, case-insensitive).    |
| `families` | List the distinct family names.                                          |
| `query`    | Print the face that best matches a CSS-like query; exit 1 if none does. |

Each command loads the system's fonts, plus any `--font-dir` (scanned
recursively) and `--font-file`; `--no-system-fonts` skips the system fonts.

```shell
fontgres families
fontgres faces --family "Noto Sans"
fontgres query "Helvetica Neue" Arial sans-serif --weight 700 --style italic
fontgres --no-system-fonts --font-dir ./fonts faces
```

`query` takes families in priority order, as CSS `font-family` does. Names
match exactly, case included, as in fontdb; the generic names `serif`,
`sans-serif`, `cursive`, `fantasy` and `monospace` resolve through
`[fonts.generic]`. `--weight` is 1–1000 (400 normal, 700 bold), `--style` is
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

## Configuration

Config file default location: `~/.config/fontgres/config.toml`
(override with `--config <PATH>` or `FONTGRES_CONFIG`).

Resolution precedence (highest first): CLI flag, then `FONTGRES_*` env, then
`config.toml`, then built-in defaults. A list (font directories or files) is
taken whole from the highest layer that sets it.

| Flag / Env                                       | Description                                              |
| ------------------------------------------------ | -------------------------------------------------------- |
| `--no-system-fonts` / `FONTGRES_FONTS_SYSTEM`    | Whether to load the system's fonts (default: `true`).    |
| `--font-dir` / `FONTGRES_FONTS_DIRS`             | Extra font directories (env: a `:`-separated path list). |
| `--font-file` / `FONTGRES_FONTS_FILES`           | Extra font files (env: a `:`-separated path list).       |
| `FONTGRES_FONTS_GENERIC_SERIF` (and `_SANS_SERIF`, `_CURSIVE`, `_FANTASY`, `_MONOSPACE`) | Family a CSS generic name resolves to. |
| `--config` / `FONTGRES_CONFIG`                   | Path to `config.toml`.                                   |
| `FONTGRES_LOG_LEVEL`                             | Log level: `trace`, `debug`, `info`, `warn`, `error`.    |

See [config.toml.example](../../config.toml.example) for all fields and defaults.
