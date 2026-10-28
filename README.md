# zeroclaw-deepseek-search

Live web search for [ZeroClaw](https://github.com/zeroclaw-labs/zeroclaw)
through the [DeepSeek Search API](https://api-docs.deepseek.com/guides/search) —
one sandboxed WebAssembly component, one tool.

[![CI](https://github.com/nickludlam/zeroclaw-deepseek-search/actions/workflows/ci.yml/badge.svg)](https://github.com/nickludlam/zeroclaw-deepseek-search/actions/workflows/ci.yml)
[![Release](https://img.shields.io/github/v/release/nickludlam/zeroclaw-deepseek-search)](https://github.com/nickludlam/zeroclaw-deepseek-search/releases)
[![License](https://img.shields.io/badge/license-MIT-blue)](#license)

`deepseek_search` returns ranked results with a model-readable summary,
publication date, relevance score, authority tier, and — on request — the
extracted page text. Queries pass DeepSeek's operators through untouched:
`site:` / `-site:`, `"exact phrase"`, `-word`, `lang:zh,en`,
`after:2025-01-01`, `before:2026-06-01`.

## Why this plugin

- **Written for a model.** DeepSeek returns a long `summary` per result;
  `snippet` is only the display fallback. Page text is opt-in and capped.
- **Key stays out of the model's view.** `api_key` is a schema-marked secret:
  the host withholds it from `__config` and serves it through the scoped
  `secrets.get` import during `execute`.
- **Deny by default.** The component can reach only the egress destinations the
  operator grants; this package declares `api.deepseek.com` and nothing else.
- **Stateless.** One HTTP call per invocation in a fresh WASM store; no retained
  state, no background work.
- **Portable.** The same `.wasm` runs on amd64 and arm64 hosts, including a
  Raspberry Pi.

## Requirements

- ZeroClaw **0.8.6 or newer with the plugin host**. Check:
  `zeroclaw plugin --help` must list `install`, `info`, and `list`. Release
  archives for 64-bit desktop/server targets carry it; a Cargo-default source
  build does not.
- A DeepSeek API key with Search access: <https://platform.deepseek.com/api_keys>.
  Search is billed per successful request (US$0.75 per 1,000 at the time of
  writing), not per token or per result.

## Install

From a release archive:

```bash
version=0.1.0
curl -LO "https://github.com/nickludlam/zeroclaw-deepseek-search/releases/download/v${version}/deepseek-search-${version}.zip"
unzip "deepseek-search-${version}.zip" -d "deepseek-search-${version}"
zeroclaw plugin install "./deepseek-search-${version}"
```

From a checkout:

```bash
scripts/package.sh
zeroclaw plugin install ./dist/deepseek-search-0.1.0
```

If you publish it to a registry (the official one or your own `registry.json`,
see [docs/PUBLISHING.md](docs/PUBLISHING.md)):

```bash
zeroclaw plugin install deepseek-search
zeroclaw plugin install deepseek-search --registry https://example.com/registry.json
```

Then enable the plugin system, store the key, and (for headless agents) approve
the tool:

```bash
zeroclaw config set plugins.enabled true
zeroclaw config set plugins.auto_discover true
zeroclaw plugin info deepseek-search          # prints the zpi1_… instance key
zeroclaw config set plugins.entries.<zpi1_key>.config.api_key   # masked prompt
```

See [docs/INSTALL.md](docs/INSTALL.md) for approval, container/Raspberry Pi
deployment, the built-in `web_search_tool` coexistence, and troubleshooting.

## Tool surface

| Parameter      | Type    | Required | Meaning                                                         |
| -------------- | ------- | -------- | --------------------------------------------------------------- |
| `query`        | string  | yes      | 1–128 characters; may combine DeepSeek search operators          |
| `count`        | integer | no       | 1–50 results; defaults to the operator's `count` (10 if unset)   |
| `need_content` | boolean | no       | Also return extracted page text, capped at 4,000 chars/result    |
| `ori_question` | string  | no       | 1–128 characters; the user's original question, improves intent  |

A rendered result looks like:

```text
Found 2 results for "deepseek v4 release".

1. DeepSeek V4 released
   URL: https://example.com/deepseek-v4
   Site: Example News · Published: 2026-06-01 · Relevance: 2.4/3 · Authority: 3/4
   DeepSeek has released the V4 model. The release adds improved tool calling…
```

## Configuration

Operator values live on the plugin's `zpi1_…` row, as typed config:

| Property       | Type    | Secret | Default | Meaning                                      |
| -------------- | ------- | ------ | ------- | -------------------------------------------- |
| `api_key`      | string  | yes    | —       | DeepSeek API key (`sk-…`); required           |
| `count`        | integer | no     | 10      | Default result count when the model omits it  |
| `need_content` | boolean | no     | false   | Default for page text                         |

Model arguments win over operator defaults. A failed search is a normal
`success: false` tool result with an actionable message (bad query, rate limit,
balance) so the model can correct itself; only a missing key or an unreadable
response is a plugin fault.

## Compatibility

| | |
| --- | --- |
| Host | ZeroClaw 0.8.6+ with the plugin host (`plugins-wasm-cranelift`); verified against 0.8.6 |
| Contract | `tool-plugin` world from [`wit/v0`](wit/v0), pinned by [wit/UPSTREAM_REF](wit/UPSTREAM_REF) |
| Guest | `wasm32-wasip2`, `wit-bindgen 0.51`, `waki 0.5.1` (blocking `wasi:http`) |
| Identity | `plugin-info` reports `CARGO_PKG_NAME` / `CARGO_PKG_VERSION`; CI keeps `Cargo.toml` and `manifest.toml` in agreement |

The WIT contract is experimental: a host release that changes `wit/v0` requires
re-vendoring it and rebuilding. `zeroclaw plugin info` reports a mismatch
immediately.

## How it works

1. Registration instantiates the component once and reads `name`, `description`,
   and the parameter schema. Nothing dynamic is computed there.
2. Each call gets a fresh store. The host injects the non-secret config
   (`count`, `need_content`) under `__config` and exposes `api_key` only through
   `secrets.get`.
3. The guest validates arguments, performs one `POST` to
   `https://api.deepseek.com/v1/tools/search`, maps non-200 responses to
   operator-actionable messages, and renders the results as Markdown.
4. The plugin logs one structured `log-record` per call with status, result
   count, and the `need_content` flag — never the query, key, or page content.

The wire logic lives in [`src/search.rs`](src/search.rs) and is covered by
[`tests/search.rs`](tests/search.rs) with a plain `cargo test`; the component
shim is [`src/lib.rs`](src/lib.rs).

## Development

```bash
cargo test                                              # host tests, no wasm target
cargo fmt --all -- --check
cargo clippy --all-targets -- -D warnings
cargo clippy --target wasm32-wasip2 -- -D warnings
rustup target add wasm32-wasip2
scripts/package.sh                                      # component + package + zip
scripts/check-version.sh                                # Cargo.toml vs manifest.toml
```

```text
.
├── Cargo.toml                  # standalone crate (cdylib + rlib)
├── manifest.toml               # release identity: capabilities, permissions, schema, egress
├── src/search.rs               # wire logic, host-tested
├── src/lib.rs                  # thin component shim
├── tests/search.rs             # cargo test over the wire logic
├── scripts/                    # package, sign, version check
├── docs/                       # install and publishing guides
└── wit/v0/                     # vendored ZeroClaw plugin contract
```

See [docs/PUBLISHING.md](docs/PUBLISHING.md) for the release and signing process.

## Security

The plugin is sandboxed by the ZeroClaw host, but `http_client` is
all-or-nothing: the egress grant, not the manifest, decides where queries go.
Search results are untrusted web content. See [SECURITY.md](SECURITY.md) for the
threat model and operator hardening steps.

## License

Licensed under the [MIT license](LICENSE).
