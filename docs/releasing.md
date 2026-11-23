# Releasing

Releases run in GitHub Actions with [release-plz](https://release-plz.dev). All
three crates share the workspace version and are published together: one
`v{version}` tag and one GitHub Release, named `fontgres v{version}`, cover the
workspace.

## How a release happens

1. A push to `main` runs Pull Request Validation.
2. When validation passes, `release-plz.yml` runs two jobs on that commit:
   - **Release PR** opens or updates a `release-plz-*` pull request. It bumps
     the version from the conventional commits since the last release and adds
     their entries to each crate's `CHANGELOG.md`.
   - **Release** publishes every crate whose version is not on crates.io yet,
     then creates the `v{version}` tag and the GitHub Release.
3. Merging the release PR therefore releases: its version bump reaches `main`,
   validation passes, and the Release job publishes it.
4. The GitHub Release starts `release-binaries.yml`. It builds `fontgres` and
   `fontgres-mcp` for six targets, attaches an archive and a `.sha256` file
   per binary and target, and rewrites `Formula/fontgres.rb` and
   `Formula/fontgres-mcp.rb` in
   [seferino-fernandez/homebrew-tools](https://github.com/seferino-fernandez/homebrew-tools).

| Target | Runner | Archive |
| --- | --- | --- |
| `x86_64-unknown-linux-gnu` | `ubuntu-latest` | `.tar.gz` |
| `aarch64-unknown-linux-gnu` | `ubuntu-24.04-arm` | `.tar.gz` |
| `x86_64-apple-darwin` | `macos-latest` | `.tar.gz` |
| `aarch64-apple-darwin` | `macos-latest` | `.tar.gz` |
| `x86_64-pc-windows-msvc` | `windows-latest` | `.zip` |
| `aarch64-pc-windows-msvc` | `windows-latest` | `.zip` |

The archives are named `<bin>-<target>.<ext>`; the cargo-binstall metadata in
the CLI and MCP manifests and the Homebrew formulas depend on that name.

A push that changes only `README.md` skips validation, so it starts no release
run; the next push picks the change up. To run release-plz without a push,
start the workflow from the Actions tab (`workflow_dispatch`).

The first release (0.1.0) publishes as soon as the workflows reach `main` and
validation passes, because no fontgres crate is on crates.io yet. The release-plz
docs do not say whether a release PR is opened for crates that have never been
published, so the workflow does not wait for one.

## Secrets

| Secret | What it is | Used by |
| --- | --- | --- |
| `RELEASE_PLZ_TOKEN` | Fine-grained PAT on this repository with Contents and Pull requests read and write | Both release-plz jobs. Events from the default `GITHUB_TOKEN` start no workflows, so without a PAT neither validation on the release PR nor `release-binaries.yml` would run. |
| `CARGO_REGISTRY_TOKEN` | crates.io API token with the `publish-new` and `publish-update` scopes | Release job |
| `HOMEBREW_TAP_TOKEN` | Fine-grained PAT on `seferino-fernandez/homebrew-tools` with Contents read and write | Tap update in `release-binaries.yml` |

## crates.io trusted publishing

Publishing uses the `CARGO_REGISTRY_TOKEN` secret. crates.io trusted publishing
cannot replace it while the Release job runs on `workflow_run`: crates.io
refuses trusted-publishing tokens for runs started by `workflow_run` or
`pull_request_target` (see `src/controllers/trustpub/tokens/exchange/mod.rs`
in rust-lang/crates.io). Moving to trusted publishing needs a release job
started by `push` that still waits for validation, plus a GitHub publisher
(owner `seferino-fernandez`, repository `fontgres`, that workflow's file name)
on each crate's crates.io settings.

## When something fails

- **A crate failed to publish.** Fix the cause on `main`. The next run, or a
  manual run, publishes the crates still missing from crates.io; crates.io never
  accepts the same version twice, so published ones are skipped.
- **A binary build failed.** Re-run the failed jobs of `release-binaries.yml`
  from the Actions tab. The tap job waits for every build.
- **The tap was not updated.** In `homebrew-tools`, `just update-fontgres`
  and `just apply-fontgres v{version}` (and the `-mcp` pair) update a formula
  by hand.
