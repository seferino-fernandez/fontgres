# Configuration

`fontgres` and `fontgres-mcp` read the same settings: which fonts to load,
the family each CSS generic name stands for, and the log level. The server has
one setting of its own.

## Where a setting comes from

Each setting comes from the highest of these that sets it:

1. a command-line flag
2. a `FONTGRES_*` environment variable
3. the config file
4. the built-in default

A list (font directories or font files) is taken whole from the highest one
that sets it; lists are not joined.

## The config file

The file is the first of these:

1. the path `--config <PATH>` gives
2. the path in `FONTGRES_CONFIG`
3. `fontgres/config.toml` in the platform's configuration directory, when
   that file exists
4. `~/.config/fontgres/config.toml`

A file named by `--config` or `FONTGRES_CONFIG` must exist. The default may
be absent, and then the built-in defaults apply.

[`config.toml.example`](../config.toml.example) lists every setting with its
default. One file serves both binaries: `fontgres` accepts the server's
`[mcp]` table without reading it.

## Settings

| Config key | Flag | Environment variable | Default |
| --- | --- | --- | --- |
| `[fonts] system` | `--no-system-fonts` | `FONTGRES_FONTS_SYSTEM` | `true` |
| `[fonts] dirs` | `--font-dir <DIR>` (repeatable) | `FONTGRES_FONTS_DIRS` | none |
| `[fonts] files` | `--font-file <FILE>` (repeatable) | `FONTGRES_FONTS_FILES` | none |
| `[fonts.generic] serif`, `sans_serif`, `cursive`, `fantasy`, `monospace` | | `FONTGRES_FONTS_GENERIC_SERIF`, `_SANS_SERIF`, `_CURSIVE`, `_FANTASY`, `_MONOSPACE` | see [Generic families](#generic-families) |
| `[log] level` | `-v`, `-q` | `FONTGRES_LOG_LEVEL` | see [Logging](#logging) |
| `[mcp] max_response_bytes` | | `FONTGRES_MCP_MAX_RESPONSE_BYTES` | `1048576` (1 MiB) |

- **`system`** loads the system fonts: the ones the platform installs or lets
  its users install.
- **`dirs`** are extra directories, scanned recursively. A file in one that
  cannot be loaded is skipped with a warning, which `-v` shows.
- **`files`** are named font files. A named font file that holds no font is
  an error.
- **Path lists:** `FONTGRES_FONTS_DIRS` and `FONTGRES_FONTS_FILES` hold
  paths separated by `:`, or by `;` on Windows.
- **`max_response_bytes`** caps one MCP tool result; the
  [`fontgres-mcp` README](../crates/fontgres-mcp/README.md#tool-calls) says
  what happens at the cap.

## Generic families

Each of the CSS generic names `serif`, `sans-serif`, `cursive`, `fantasy`
and `monospace` stands for one family. A query that names `sans-serif` asks
for that family, not for a list of fallbacks.

The family is the first of these that names one:

1. `[fonts.generic]` or its environment variable
2. when the system fonts load on a platform that reads fontconfig's
   configuration, the first installed family that fontconfig's `<alias>`
   rules give for the generic name; `--no-system-fonts` skips this step
3. the built-in default: Times New Roman, Arial, Comic Sans MS, Impact
   (Papyrus on macOS) and Courier New

## Mistakes fail at startup

A setting that cannot be used stops the binary with exit 1, and the message
names where the setting came from:

```console
$ FONTGRES_FONTS_SYSTEM=flase fontgres families
fontgres: loading config: configuration error: invalid FONTGRES_FONTS_SYSTEM value "flase": expected one of 1, true, yes, on, 0, false, no, off
```

- An environment value that does not parse names the variable and the value.
- A TOML error names the file, the line and the column.
- A key or table the file does not expect is such an error, so a misspelt
  setting never silently does nothing.
- A config file that cannot be read is named.
- A font directory that cannot be read, or a named font file that holds no
  font, stops the run when the fonts load and is named too.

## Logging

Logs go to stderr. Both binaries take `-v`/`--verbose` and `-q`/`--quiet`,
before or after the subcommand: `-q` is silent, `-v` shows warnings, `-vv`
info, `-vvv` debug and `-vvvv` trace.

The level is the first of these that sets one:

1. `RUST_LOG` (for example `RUST_LOG=fontgres_core=trace`)
2. `-v` or `-q`
3. `FONTGRES_LOG_LEVEL` or `[log] level`: `trace`, `debug`, `info`,
   `warn`, `error` or `off`
4. the built-in default: `error` for `fontgres`, `info` for `fontgres-mcp`

An invalid level in `FONTGRES_LOG_LEVEL` or `[log] level` fails at startup
even when a flag would override it. The warning for a font file or face that
cannot be loaded follows the same level.
