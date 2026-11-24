# Installing fontgres

fontgres is two binaries, and each installs by itself:

| Binary | What it is | Homebrew formula | Crate |
| --- | --- | --- | --- |
| `fontgres` | The command-line tool | `seferino-fernandez/tools/fontgres` | `fontgres-cli` |
| `fontgres-mcp` | The MCP server | `seferino-fernandez/tools/fontgres-mcp` | `fontgres-mcp` |

The commands below install `fontgres`. Put the other formula, crate or binary
name in its place to install the server.

## Homebrew

On macOS and Linux:

```shell
brew install seferino-fernandez/tools/fontgres
```

The formula installs the prebuilt binary with its shell completions and man
pages, so the two sections on those below do not apply.

## cargo-binstall

[cargo-binstall](https://github.com/cargo-bins/cargo-binstall) downloads the
prebuilt binary for your platform:

```shell
cargo binstall fontgres-cli
```

## cargo install

Building from source needs Rust 1.100 or newer:

```shell
cargo install fontgres-cli
```

## Release archives

Each [GitHub Release](https://github.com/seferino-fernandez/fontgres/releases)
carries one archive per binary and target, named `<binary>-<target>`, and a
`.sha256` file beside it. An archive holds the binary, the licence and a README.

| Target | Archive |
| --- | --- |
| `x86_64-unknown-linux-gnu` | `.tar.gz` |
| `aarch64-unknown-linux-gnu` | `.tar.gz` |
| `x86_64-apple-darwin` | `.tar.gz` |
| `aarch64-apple-darwin` | `.tar.gz` |
| `x86_64-pc-windows-msvc` | `.zip` |
| `aarch64-pc-windows-msvc` | `.zip` |

To download the latest `fontgres` for an Apple silicon Mac, check it and unpack it:

```shell
base=https://github.com/seferino-fernandez/fontgres/releases/latest/download
curl -fsSLO "$base/fontgres-aarch64-apple-darwin.tar.gz"
curl -fsSLO "$base/fontgres-aarch64-apple-darwin.sha256"
shasum -a 256 -c fontgres-aarch64-apple-darwin.sha256
tar -xzf fontgres-aarch64-apple-darwin.tar.gz
```

Then move `fontgres` to a directory on your `PATH`. On Linux,
`sha256sum -c` does the check.

## From a checkout

```shell
cargo install --path crates/fontgres-cli
```

## Check the install

```shell
fontgres --version
fontgres families
```

## Shell completions

Both binaries have a `completions` subcommand. It prints a static script for
bash, elvish, fish, nushell, powershell and zsh, and every one of those except
nushell can also complete dynamically.

**Static:** generate a script once and put it where your shell looks for it.

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

**Dynamic:** the binary answers each completion as you type, through the
`COMPLETE` environment variable. Add one line to your shell's startup file:

```shell
echo 'source <(COMPLETE=zsh fontgres)'  >> ~/.zshrc
```

```shell
echo 'source <(COMPLETE=bash fontgres)' >> ~/.bashrc
```

```shell
echo 'COMPLETE=fish fontgres | source'  >> ~/.config/fish/completions/fontgres.fish
```

`fontgres-mcp` works the same way: `fontgres-mcp completions zsh`, or
`COMPLETE=zsh fontgres-mcp`. Run with no subcommand, it still starts the
server.

## Man pages

The `man` subcommand writes a ROFF man page for the binary and for each of
its subcommands into a directory, which it creates when missing:

```shell
fontgres man ~/.local/share/man/man1
fontgres-mcp man ~/.local/share/man/man1
```

To read one without installing it, give `man` its path:

```shell
fontgres man ./man
man ./man/fontgres-query.1
```
