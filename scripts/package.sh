#!/usr/bin/env bash
# Build the wasm component and assemble the installable package and zip.
set -euo pipefail
cd "$(dirname "$0")/.."

crate=deepseek-search
component=deepseek_search
target=wasm32-wasip2

command -v zip >/dev/null 2>&1 || { echo "error: zip is required" >&2; exit 1; }

sha256() {
  if command -v sha256sum >/dev/null 2>&1; then
    sha256sum "$1" | cut -d' ' -f1
  else
    shasum -a 256 "$1" | cut -d' ' -f1
  fi
}

./scripts/check-version.sh >/dev/null
version=$(sed -n 's/^version = "\([^"]*\)"$/\1/p' Cargo.toml | head -n1)

cargo build --locked --release --target "$target"

artifact="target/$target/release/$component.wasm"
[[ -f "$artifact" ]] || { echo "error: expected $artifact" >&2; exit 1; }

out="dist/$crate-$version"
archive="$crate-$version.zip"
rm -rf "$out" "dist/$archive" "dist/$archive.sha256"
mkdir -p "$out"
cp "$artifact" "$out/"
cp manifest.toml "$out/"

# Registries and `plugin install <zip>` expect the package at the archive root.
(cd "$out" && zip -qr "../$archive" .)

digest=$(sha256 "dist/$archive")
printf '%s  %s\n' "$digest" "$archive" > "dist/$archive.sha256"

printf 'component: %s (%s bytes)\n' "$out/$component.wasm" "$(wc -c < "$out/$component.wasm" | tr -d ' ')"
printf 'wasm sha256: %s\n' "$(sha256 "$out/$component.wasm")"
printf 'package: %s\n' "$out"
printf 'archive: dist/%s (sha256 %s)\n' "$archive" "$digest"
