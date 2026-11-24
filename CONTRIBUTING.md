# Contributing to fontgres

## Where a change goes

Three crates, and the two binaries both depend on the library:

```text
            fontgres-core  (font database, configuration, font views, errors)
              /     \
       fontgres-cli   fontgres-mcp
     (the CLI)      (the MCP server)
```

Font loading, queries, configuration and errors live in `fontgres-core`, so a
change to font behaviour lands there and both binaries get it.
[GLOSSARY.md](GLOSSARY.md) defines the project's terms (face, family, font
file, query), and [docs/adr/](docs/adr/) records the design decisions that are
not open again without a new reason.

## Build, run and test

`rust-toolchain.toml` pins the Rust version, and rustup installs it on the
first build.

```shell
cargo build
cargo run -p fontgres-cli -- families
cargo run -p fontgres-cli -- query "Helvetica Neue" sans-serif --weight 700
cargo run -p fontgres-mcp
cargo test
```

`just install` installs both binaries from the checkout.

## Checks

`just verify` runs what CI runs: formatting, the Clippy lints, the tests with
all features and with the defaults, the docs build, the dependency policy,
spelling, TOML style and the ast-grep rules. A change is ready when it passes.
[docs/quality-guardrails.md](docs/quality-guardrails.md) lists each tool and
what it catches.

Install the tools once:

```shell
cargo install --locked just cargo-nextest cargo-deny cargo-shear typos-cli ast-grep prek
cargo install --locked cargo-mutants   # optional: `just mutants-diff`
prek install                           # run the fast checks on each commit
```

Tombi installs separately (see
[its install guide](https://tombi-toml.github.io/tombi/docs/installation)).

Two checks sit outside `just verify`:

- `just dupes` fails on code a branch duplicates. CI runs it on every pull
  request, and it needs Node.js for `npx`.
- `just mutants-diff` breaks the changed code on purpose and reports each
  break the tests do not catch. It runs locally only.

## Pull requests

- **Commits:** write [Conventional Commits](https://www.conventionalcommits.org)
  (`feat(cli): …`, `fix(core): …`). The release tooling reads them to choose
  the next version and to write each crate's changelog.
- **Lints:** fix the code a lint flags. When the code is right as written, put
  `#[expect(lint, reason = "…")]` on the item; `#[allow]` fails the checks.
- **Tests:** when a test looks wrong, say so in the pull request instead of
  deleting or weakening it.
- **Dependencies:** a new one needs a sentence in the pull request on why std
  or an existing dependency cannot do the job.
- **Configuration:** list any change to the lint, formatter or CI
  configuration in the pull request, with the reason.

[AGENTS.md](AGENTS.md) states the same rules for coding agents, and
[docs/releasing.md](docs/releasing.md) describes how a release is cut.
