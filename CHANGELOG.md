# Changelog

All notable changes to this project are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and published plugin
versions are immutable.

## [0.1.0] - 2026-10-28

### Added

- `deepseek_search` tool plugin for the DeepSeek Search API
  (`POST https://api.deepseek.com/v1/tools/search`).
- Model-readable Markdown output with summary, site, publication date,
  relevance, authority, and optional capped page text.
- Query-operator passthrough: `site:`, `-site:`, exact phrases, exclusions,
  `lang:`, `after:`, and `before:`.
- Typed operator config: required secret `api_key`, plus `count` (1–50) and
  `need_content` defaults.
- 18 host-run tests over request validation, response parsing, rendering, and
  HTTP status mapping.
- Manifest declares `http_client` and `api.deepseek.com`; egress remains the
  operator's grant.

[0.1.0]: https://github.com/nickludlam/zeroclaw-deepseek-search/releases/tag/v0.1.0
