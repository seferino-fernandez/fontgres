# fontgres-mcp

MCP server exposing the FONTGRES API as tools for LLM clients.

Built on [rmcp](https://github.com/modelcontextprotocol/rust-sdk) 3.x and [fontgres-core](../fontgres-core/).

Implements the MCP [2026-07-28](https://modelcontextprotocol.io/specification/2026-07-28)
specification, and negotiates down to every earlier version `rmcp` supports.

## Transports

- `stdio` for Claude Desktop, Claude Code, and other local MCP clients
## Quick start

stdio transport:

```bash
fontgres-mcp --transport stdio
```

## Logging verbosity

`-v`/`--verbose` and `-q`/`--quiet` adjust the log level (`-q` silent; `-v` warn,
`-vv` info, `-vvv` debug, `-vvvv` trace). Default level precedence, highest first:

`RUST_LOG` → `-v`/`-q` (when passed) → `FONTGRES_LOG_LEVEL` / config `log.level` → default (`info`)

So `fontgres-mcp -q --transport stdio` quiets a run without touching config, while
config / `FONTGRES_LOG_LEVEL` still apply when no flag is given. An invalid
configured level prints a warning and falls back to `info`.

## Example tools

Every tool name carries the `fontgres_` prefix, so a client running several MCP
servers can tell this one's tools apart.

| Tool                      | Description                                  | Read-only |
| ------------------------- | -------------------------------------------- | --------- |
| `fontgres_get_system_status` | Server health/status                         | Yes |
| `fontgres_list_items`        | List items (paginated: `page`, `page_size`)  | Yes |
| `fontgres_get_item`          | Fetch one item by `id`                       | Yes |
| `fontgres_create_item`       | Create an item (`name`, `enabled`)           | No |
| `fontgres_delete_item`       | Delete an item by `id` (destructive)         | No |

### Tool calls

Every call goes through the same checks, in order:

1. **Arguments.** An argument the tool's input schema does not declare is
   refused, without contacting the upstream API. Every params struct also
   denies unknown fields.
1. **Confirmation.** A tool annotated `destructiveHint: true`, called by a
   client that negotiated 2026-07-28 and supports form elicitation, first
   answers an input-required result asking the user to confirm. Its
   arguments are decoded first, so a mistyped one fails before the user is
   asked. The pending call is sealed into its `requestState` (valid for 5
   minutes, bound to the tool name, keyed per process). The retry must carry
   the same arguments and an accepted `confirm: true`; changed arguments, a
   decline or a cancel are refused. An accepted state is single-use: a
   replay is refused. The server remembers up to 4096 spent states; past
   that it forgets the oldest early, so under that much confirmation traffic
   a spent state could be replayed, but only within its 5 minutes. Clients without form elicitation, or on an older protocol, run the
   tool directly. Turn the prompt off with `[mcp] confirm_destructive = false`
   or `FONTGRES_MCP_CONFIRM_DESTRUCTIVE=false`. This is a courtesy against a
   model acting alone, not access control.
1. **Response budget.** The whole serialized result, `content` and
   `structuredContent` together, is capped at `max_response_bytes` (default
   1 MiB). A larger result is replaced by a small `response_too_large` error;
   if even that cannot fit, the call fails with a protocol error.

Every result is `Json<Redacted<T>>`, built by `ApiClient::redact`, and the
output schema stays `T`'s. It masks the credential forms listed in the
[`fontgres-core` README](../fontgres-core/README.md#redaction) (string values
under sensitive keys, secret pairs, query values and path segments, URL user
information, `Authorization` and `Cookie` header values, JSON embedded in
strings, object keys) and the configured API key itself. Redaction is
pattern-based and never changes a value's type, so a secret in a shape it
does not recognize, or stored as a number, reaches the model. A failing call answers a tool error holding only the core error's
public message (a 404 adds a hint to use the list tool), never the upstream
body, the request URL, or decoder detail.

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
`FONTGRES_*` env, then `FONTGRES_*_FILE` env, then `config.toml`. A secret's lower
layers are never read. A value that does not parse fails at startup naming its
source: an env var by name and raw value, a TOML key by file, line and
column (the offending line is never echoed, since it may hold a secret).

| Variable | Description |
| --- | --- |
| `FONTGRES_BASE_URL`                    | Upstream API base URL                                |
| `FONTGRES_API_KEY`                     | API key sent as `X-Api-Key`                          |
| `FONTGRES_MCP_CONFIRM_DESTRUCTIVE`     | Confirm destructive tools (default: `true`)          |
| `FONTGRES_MCP_MAX_RESPONSE_BYTES`      | Tool result size cap in bytes (default: 1 MiB)       |


See [config.toml.example](../../config.toml.example) for all available fields.
