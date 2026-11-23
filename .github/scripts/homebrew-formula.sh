#!/usr/bin/env bash
# Renders the Homebrew formula for one fontgres binary to stdout.
#
# Usage: homebrew-formula.sh <fontgres|fontgres-mcp> <version> <checksum-dir>
#
# <checksum-dir> holds the <bin>-<target>.sha256 files that
# taiki-e/upload-rust-binary-action uploads next to each archive. The formula
# is written whole, so the tap never keeps a hand edit CI does not know about.
# Each url line is followed by its sha256 line and the formula names no
# version (brew reads it from the URL), so the tap's apply-update recipe and
# `brew audit --strict` keep working on the result.
set -euo pipefail

if [[ $# -ne 3 ]]; then
    echo "usage: $0 <fontgres|fontgres-mcp> <version> <checksum-dir>" >&2
    exit 2
fi
bin=$1
version=$2
checksum_dir=$3

case "$bin" in
    fontgres)
        class=Fontgres
        desc="Query installed fonts from the terminal"
        ;;
    fontgres-mcp)
        class=FontgresMcp
        desc="MCP server for querying installed fonts"
        ;;
    *)
        echo "unknown binary: $bin" >&2
        exit 2
        ;;
esac

if [[ ! "$version" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]]; then
    echo "version must be X.Y.Z without a leading v: $version" >&2
    exit 2
fi

repo=https://github.com/seferino-fernandez/fontgres
base="$repo/releases/download/v$version"

# Prints the sha256 of <bin>-<target>.tar.gz, failing unless it is 64 hex digits.
sha() {
    local file="$checksum_dir/$bin-$1.sha256" hash
    if [[ ! -f "$file" ]]; then
        echo "missing checksum file: $file" >&2
        exit 1
    fi
    hash=$(awk -v archive="$bin-$1.tar.gz" '$2 == archive || $2 == "*" archive { print $1 }' "$file")
    if [[ ! "$hash" =~ ^[0-9a-f]{64}$ ]]; then
        echo "no sha256 for $bin-$1.tar.gz in $file" >&2
        exit 1
    fi
    echo "$hash"
}

mac_intel=$(sha x86_64-apple-darwin)
mac_arm=$(sha aarch64-apple-darwin)
linux_intel=$(sha x86_64-unknown-linux-gnu)
linux_arm=$(sha aarch64-unknown-linux-gnu)

cat <<FORMULA
class $class < Formula
  desc "$desc"
  homepage "$repo"
  license "MIT"

  on_macos do
    if Hardware::CPU.intel?
      url "$base/$bin-x86_64-apple-darwin.tar.gz"
      sha256 "$mac_intel"
    end
    if Hardware::CPU.arm?
      url "$base/$bin-aarch64-apple-darwin.tar.gz"
      sha256 "$mac_arm"
    end
  end

  on_linux do
    if Hardware::CPU.intel?
      url "$base/$bin-x86_64-unknown-linux-gnu.tar.gz"
      sha256 "$linux_intel"
    end
    if Hardware::CPU.arm?
      url "$base/$bin-aarch64-unknown-linux-gnu.tar.gz"
      sha256 "$linux_arm"
    end
  end

  def install
    bin.install "$bin"
    generate_completions_from_executable(bin/"$bin", "completions")
    system bin/"$bin", "man", buildpath/"man"
    man1.install buildpath.glob("man/*.1")
  end

  test do
    assert_match version.to_s, shell_output("#{bin}/$bin --version")
  end
end
FORMULA
