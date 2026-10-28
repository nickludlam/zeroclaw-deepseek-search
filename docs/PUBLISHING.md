# Publishing deepseek-search

The plugin ships as a **package directory** (`manifest.toml` +
`deepseek_search.wasm`) and, for registries, as a zip whose root is that
directory. Binaries are never committed to git; they are release assets.

## Before the first publish

- Confirm every repository URL points at `nickludlam/zeroclaw-deepseek-search`:
  `Cargo.toml` (`repository`, `homepage`), the README badges and install
  command, `CHANGELOG.md`, `docs/INSTALL.md`, and `registry.example.json`.
- The copyright line in `LICENSE` is set to Nick Ludlam; adjust it only if
  you publish under a different name.
- Optionally add `author = "…"` to `manifest.toml`; registries display it.
- `registry.example.json` keeps a placeholder zip digest. The release workflow
  writes the real `registry-entry.json`, or substitute the released zip's
  `sha256`.

Then run the full check:

```bash
scripts/check-version.sh
cargo test
cargo fmt --all -- --check
cargo clippy --all-targets -- -D warnings
cargo clippy --target wasm32-wasip2 -- -D warnings
scripts/package.sh
```

## Versioning

The version appears in three places:

1. `manifest.toml` — the release identity the registry and installer read.
2. `Cargo.toml` — what `plugin-info` announces, via `env!("CARGO_PKG_VERSION")`.
3. `Cargo.lock` — CI and the release build use `--locked`.

`scripts/check-version.sh` fails when (1) and (2) disagree, and
`cargo test --locked` fails when (3) is stale. Bump all three together.

Published `<name>@<version>` identities are immutable. Any change that alters
package bytes — source, manifest, compiler, or WIT — requires a new version, and
pinned installs must keep resolving to their original artifact. Do not rewrite a
release asset: upload a new version.

## Updating the WIT contract

`wit/v0/` is vendored unmodified from ZeroClaw and pinned by
[`wit/UPSTREAM_REF`](../wit/UPSTREAM_REF). When the host contract changes:

```bash
# in a ZeroClaw checkout at the release you target
cp -R wit/v0 /path/to/deepseek-search/wit/v0
git -C /path/to/zeroclaw rev-parse HEAD   # record this in wit/UPSTREAM_REF
```

Update the pin and the WIT in one commit, rebuild, and verify against that host:

```bash
scripts/package.sh
zeroclaw plugin install ./dist/deepseek-search-<version>
```

Then release a new plugin version. The WIT is experimental; a mismatch surfaces
as an instantiation failure at install (or `Loads: no` in `plugin info`).

## Build and package

```bash
scripts/package.sh
```

Produces:

```text
dist/deepseek-search-0.1.0/
├── manifest.toml
└── deepseek_search.wasm
dist/deepseek-search-0.1.0.zip
dist/deepseek-search-0.1.0.zip.sha256
```

The zip contains the package at its root (no enclosing directory), which is the
layout registries and `plugin install <zip>` expect. The script prints the
component's SHA-256 for release notes.

## Signing (optional, required for `strict` hosts)

Unsigned packages install under the default `signature_mode = "disabled"` and
load with a warning under `permissive`. Operators running `strict` accept only a
signature they trust, so sign if you want those deployments.

Use the signer from a ZeroClaw checkout. Generate a key once and keep it outside
both trees:

```bash
cargo run -p zeroclaw-plugins --example sign_manifest -- \
  keygen ~/.keys/deepseek-search.pk8
# prints the hex public key; back up the private key
scripts/sign.sh /path/to/zeroclaw ~/.keys/deepseek-search.pk8
```

`scripts/sign.sh` re-signs `dist/deepseek-search-<version>/manifest.toml` with a
`wasm_sha256` bound to the exact component, prints the publisher key, and
re-zips the package in place. Sign last: any edit to the manifest after signing
invalidates it. Operators trust it with:

```bash
zeroclaw config set plugins.security.signature_mode strict
zeroclaw config set plugins.security.trusted_publisher_keys '["<hex-public-key>"]'
```

The official ZeroClaw registry currently publishes unsigned packages; signing is
how you support strict hosts and how you let an operator pin your exact key.

## Release

1. Bump the version in `Cargo.toml` and `manifest.toml`, then update
   `Cargo.lock` (`cargo update -p deepseek-search` or a build).
2. Add the entry to `CHANGELOG.md`.
3. Commit and tag `v<version>`:

   ```bash
   git tag -s v0.1.0 -m "deepseek-search 0.1.0"
   git push origin v0.1.0
   ```

4. The [release workflow](../.github/workflows/release.yml) verifies the tag
   matches the package version, runs the tests, packages the plugin, and uploads
   `deepseek-search-<version>.zip`, its `.sha256`, and a ready-to-serve
   `registry-entry.json` to a GitHub Release.

To publish a signed artifact instead, run `scripts/package.sh` and
`scripts/sign.sh` locally and upload the signed zip to the release (replace the
unsigned asset, or attach it as a separate file and point the registry entry at
it).

## Registry listing

A registry is a JSON index of release assets. Two options:

**Official registry** — open a pull request against
[`zeroclaw-labs/zeroclaw-plugins`](https://github.com/zeroclaw-labs/zeroclaw-plugins)
adding `plugins/deepseek-search/` in the reference format. Their CI builds every
plugin, packages new immutable versions, and regenerates `registry.json`; do not
hand-edit the index.

That repository keeps one shared `wit/` at its root, while this standalone
project vendors `wit/` next to the crate. Either submit the directory as-is
(self-contained, and the `generate!` path resolves to its own `wit/v0`), or move
`wit/` to the registry repo root and change `path: "wit/v0"` in `src/lib.rs` to
`path: "../../wit/v0"` to match the reference layout. Keep `wit/UPSTREAM_REF`
with the plugin either way.

**Your own registry** — host a `registry.json` next to the release assets:

```json
{
  "plugins": [
    {
      "name": "deepseek-search",
      "version": "0.1.0",
      "description": "Web search through the DeepSeek Search API",
      "author": "nickludlam",
      "capabilities": ["tool"],
      "url": "https://github.com/nickludlam/zeroclaw-deepseek-search/releases/download/v0.1.0/deepseek-search-0.1.0.zip",
      "sha256": "<sha256 of the zip>"
    }
  ]
}
```

Operators then install with:

```bash
zeroclaw plugin install deepseek-search --registry https://example.com/registry.json
export ZEROCLAW_PLUGIN_REGISTRY_URL=https://example.com/registry.json
```

`sha256` covers the zip (transport integrity); the host separately enforces its
signature policy (authenticity). See
[`registry.example.json`](../registry.example.json).

## Post-release verification

Install the published artifact on a clean config dir and confirm the verdict:

```bash
zeroclaw --config-dir /tmp/ds-check plugin install ./deepseek-search-<version>
zeroclaw --config-dir /tmp/ds-check plugin info deepseek-search
```

Set a key, or expect the `missing required property 'api_key'` verdict:

```bash
zeroclaw --config-dir /tmp/ds-check config set \
  plugins.entries.zpi1_WyJkZWVwc2Vlay1zZWFyY2giLCJ0b29sIiwiZGVlcHNlZWstc2VhcmNoIl0.config.api_key \
  sk-check-only --no-interactive
zeroclaw --config-dir /tmp/ds-check plugin info deepseek-search   # Loads: yes
```
