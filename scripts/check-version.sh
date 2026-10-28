#!/usr/bin/env bash
# Fail when the crate version and the release manifest disagree.
set -euo pipefail
cd "$(dirname "$0")/.."

fail() { printf 'error: %s\n' "$*" >&2; exit 1; }

cargo_version=$(sed -n 's/^version = "\([^"]*\)"$/\1/p' Cargo.toml | head -n1)
manifest_version=$(sed -n 's/^version = "\([^"]*\)"$/\1/p' manifest.toml | head -n1)

[[ -n "$cargo_version" ]] || fail "could not read version from Cargo.toml"
[[ -n "$manifest_version" ]] || fail "could not read version from manifest.toml"
[[ "$cargo_version" == "$manifest_version" ]] || fail \
  "Cargo.toml version '$cargo_version' does not match manifest.toml version '$manifest_version'"

grep -q 'env!("CARGO_PKG_NAME")' src/lib.rs || fail \
  'src/lib.rs must report env!("CARGO_PKG_NAME")'
grep -q 'env!("CARGO_PKG_VERSION")' src/lib.rs || fail \
  'src/lib.rs must report env!("CARGO_PKG_VERSION")'

printf 'version ok: %s (Cargo.toml = manifest.toml)\n' "$cargo_version"
