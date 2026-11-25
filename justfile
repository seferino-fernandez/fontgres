default:
    @just --list

# Full gate: everything CI checks except the PR-only duplicate check (`just dupes`)
verify:
	just hygiene
	cargo fmt --all --check
	cargo check --locked --workspace
	cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
	just lint-features
	just test
	just doc
	cargo deny check
	cargo shear

# Spelling, TOML formatting and lint, and the ast-grep rules with their tests
hygiene:
	typos
	tombi format --check
	tombi lint --error-on-warnings
	ast-grep scan
	ast-grep test

# Mutation-test only the code changed since main; any surviving mutant fails
mutants-diff:
	mkdir -p target
	git diff --no-ext-diff --no-color --src-prefix=a/ --dst-prefix=b/ "$(git merge-base main HEAD)" > target/mutants.diff
	cargo mutants --in-diff target/mutants.diff

# Fail on code clones introduced since origin/main (needs Node.js)
dupes:
	npx --yes jscpd@5.4.3 --baseline-from-ref origin/main --fail-on-new-clones=0 crates

# Run all tests: full feature matrix, default features, fontgres-core's serde feature without cli, and doctests
test:
	cargo nextest run --all-targets --all-features
	cargo nextest run
	cargo nextest run -p fontgres-core --all-targets --no-default-features --features serde
	cargo test --doc
	cargo test --doc -p fontgres-core --no-default-features --features serde

# Build API docs, failing on rustdoc warnings
doc:
	RUSTDOCFLAGS="-D warnings" cargo doc --no-deps --all-features

# Lint the code with Clippy, failing on any warning
lint:
    cargo clippy --locked --workspace --all-targets --all-features -- -D warnings

# Lint fontgres-core with its optional features off: no_std, std only, std with fs but no memmap, serde and schema without cli, and the defaults with serde
lint-features:
	cargo clippy --locked -p fontgres-core --all-targets --no-default-features -- -D warnings
	cargo clippy --locked -p fontgres-core --all-targets --no-default-features --features std -- -D warnings
	cargo clippy --locked -p fontgres-core --all-targets --no-default-features --features std,fs -- -D warnings
	cargo clippy --locked -p fontgres-core --all-targets --no-default-features --features serde -- -D warnings
	cargo clippy --locked -p fontgres-core --all-targets --no-default-features --features schema -- -D warnings
	cargo clippy --locked -p fontgres-core --all-targets --features serde -- -D warnings

# Format code with rustfmt and apply Clippy's auto-fixable suggestions
format:
    cargo fmt --all
    cargo clippy --all-targets --all-features --fix --allow-dirty --allow-staged

# Build the project in debug mode
build:
    cargo build

# Print a static CLI completion script (bash|elvish|fish|nushell|powershell|zsh); redirect to install
completions shell="zsh":
    cargo run --quiet -p fontgres-cli -- completions {{shell}}

# Print a static MCP-server completion script (bash|elvish|fish|nushell|powershell|zsh); redirect to install
completions-mcp shell="zsh":
    cargo run --quiet -p fontgres-mcp -- completions {{shell}}

# Generate man pages for the CLI into <dir> (created if missing; default ./man)
man dir="man":
    cargo run --quiet -p fontgres-cli -- man {{dir}}

# Generate man pages for the MCP server into <dir> (created if missing; default ./man)
man-mcp dir="man":
    cargo run --quiet -p fontgres-mcp -- man {{dir}}

# Install both the CLI and MCP server in parallel
[parallel]
install: install-cli install-mcp

# Install the fontgres CLI tool
install-cli:
    cargo install --path crates/fontgres-cli

# Install the fontgres MCP server
install-mcp:
    cargo install --path crates/fontgres-mcp

# Build the project in release mode for production
release:
    cargo build --release

# Build with cargo-audit (dependency metadata embedded), then scan the binaries for known vulnerabilities
audit:
    cargo auditable build --release
    cargo audit bin target/release/fontgres
    cargo audit bin target/release/fontgres-mcp

# Remove the target directory and all build artifacts
clean:
    cargo clean

# Generate and open the project's API documentation in a browser
docs:
    cargo doc --open

# Generate an HTML coverage report from nextest runs and open it in a browser
coverage:
    cargo llvm-cov nextest --open --html
