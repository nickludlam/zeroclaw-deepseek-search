#!/usr/bin/env bash
# Sign the packaged manifest with a ZeroClaw checkout's sign_manifest example.
#
#   scripts/sign.sh <zeroclaw-checkout> <private-key.pk8>
#
# Run scripts/package.sh first. The package under dist/ is re-signed and
# re-zipped in place; the printed public key is what operators trust.
set -euo pipefail
cd "$(dirname "$0")/.."

usage="usage: scripts/sign.sh <zeroclaw-checkout> <private-key.pk8>"
zeroclaw=${1:?$usage}
key=${2:?$usage}

[[ -f "$zeroclaw/Cargo.toml" ]] || { echo "error: not a ZeroClaw checkout: $zeroclaw" >&2; exit 1; }
[[ -f "$key" ]] || { echo "error: private key not found: $key" >&2; exit 1; }

sha256() {
  if command -v sha256sum >/dev/null 2>&1; then
    sha256sum "$1" | cut -d' ' -f1
  else
    shasum -a 256 "$1" | cut -d' ' -f1
  fi
}

version=$(sed -n 's/^version = "\([^"]*\)"$/\1/p' Cargo.toml | head -n1)
package="dist/deepseek-search-$version"
archive="deepseek-search-$version.zip"

[[ -f "$package/manifest.toml" && -f "$package/deepseek_search.wasm" ]] || {
  echo "error: run scripts/package.sh first" >&2
  exit 1
}

cargo run --quiet --manifest-path "$zeroclaw/Cargo.toml" -p zeroclaw-plugins \
  --example sign_manifest -- \
  sign manifest.toml "$package/manifest.toml" \
  --key "$key" --payload "$package/deepseek_search.wasm"

grep -q '^signature = ' "$package/manifest.toml" || {
  echo "error: signing did not write a signature" >&2
  exit 1
}

rm -f "dist/$archive"
(cd "$package" && zip -qr "../$archive" .)
printf '%s  %s\n' "$(sha256 "dist/$archive")" "$archive" > "dist/$archive.sha256"

printf 'signed package: %s\n' "$package"
printf 'archive: dist/%s\n' "$archive"
