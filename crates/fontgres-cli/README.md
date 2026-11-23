# fontgres-cli

Terminal CLI for the FONTGRES API, built on [fontgres-core](../fontgres-core/).

## Build

```shell
cargo build --release -p fontgres-cli
```

The binary is written to `target/release/fontgres`. Add it to your `PATH` or invoke it directly.

## Commands

| Command       | Description                                     |
| ------------- | ----------------------------------------------- |
| `status`      | Show upstream server health.                    |
| `item list`   | List items (paginated, `--page`/`--page-size`). |
| `item get`    | Fetch a single item by id.                      |
| `item create` | Create an item (`<name>`, `--enabled`).         |
| `item delete` | Delete an item by id.                           |

Server health:

```shell
fontgres status
```

List the first page of items:

```shell
fontgres item list --page 1 --page-size 20
```

Fetch / create / delete:

```shell
fontgres item get 42
fontgres item create "my item" --enabled
fontgres item delete 42
```

## Output formats

`--output table|json|csv` (default: `table`) is a global flag accepted by every command.
Logs go to stderr, so stdout carries only structured data when `--output json` or
`--output csv` is active, which makes it safe to pipe.

```shell
fontgres --output json item list | jq '.[] | {id, name}'
fontgres --output csv item list > items.csv
```

## Configuration

Config file default location: `~/.config/fontgres/config.toml`
(override with `--config <PATH>` or `FONTGRES_CONFIG`).

Resolution precedence (highest first): CLI flag, then `FONTGRES_*` env, then `FONTGRES_*_FILE` env, then `config.toml`, then built-in defaults.

| Flag / Env                      | Description                                           |
| ------------------------------- | ----------------------------------------------------- |
| `--base-url` / `FONTGRES_BASE_URL` | Upstream API base URL.                                |
| `--api-key` / `FONTGRES_API_KEY`   | API key sent as the `X-Api-Key` header.               |
| `FONTGRES_API_KEY_FILE`            | Path to a file containing the API key.                |
| `--config` / `FONTGRES_CONFIG`     | Path to `config.toml`.                                |
| `FONTGRES_LOG_LEVEL`               | Log level: `trace`, `debug`, `info`, `warn`, `error`. |

Prefer `FONTGRES_API_KEY` or config over `--api-key` to avoid exposing the key in
process listings (e.g. `ps`).

See [config.toml.example](../../config.toml.example) for all fields and defaults.
