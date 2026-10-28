# Installing deepseek-search

This guide is for operators. It assumes a ZeroClaw binary with the plugin host
and a DeepSeek API key.

## 1. Requirements

```bash
zeroclaw --version        # 0.8.6 or newer
zeroclaw plugin --help    # must list install / info / list
```

Release archives for the 64-bit desktop and server targets carry the plugin
host. A Cargo-default source build does not; build with
`--features plugins-wasm-cranelift` (or use a release archive). The 32-bit ARM
archives and the experimental Android archive carry no plugin host.

A DeepSeek API key with Search access is required. Create one at
<https://platform.deepseek.com/api_keys>. Search is billed per **successful**
request (US$0.75 per 1,000 at the time of writing); empty results, failed
requests, and timeouts are not billed.

## 2. Install the package

### From a release

```bash
version=0.1.0
curl -LO "https://github.com/nickludlam/zeroclaw-deepseek-search/releases/download/v${version}/deepseek-search-${version}.zip"
unzip "deepseek-search-${version}.zip" -d "deepseek-search-${version}"
zeroclaw plugin install "./deepseek-search-${version}"
```

Install validates the manifest, runs the signature policy, and instantiates the
component against this host's WIT. It also creates the instance's
`[[plugins.entries]]` row and, because the manifest declares `api.deepseek.com`
and requests `http_client`, seeds that row's `egress_hosts` with it.

### From source

```bash
rustup target add wasm32-wasip2
scripts/package.sh
zeroclaw plugin install ./dist/deepseek-search-0.1.0
```

`scripts/package.sh` builds `wasm32-wasip2` and assembles the package
(`manifest.toml` + `deepseek_search.wasm`) under `dist/`.

### From a registry

```bash
zeroclaw plugin install deepseek-search
zeroclaw plugin install deepseek-search --registry https://example.com/registry.json
export ZEROCLAW_PLUGIN_REGISTRY_URL=https://example.com/registry.json
```

See [PUBLISHING.md](PUBLISHING.md) for the index format.

## 3. Enable it

Both switches are off by default, and `auto_discover` is the one people forget:
`enabled` alone loads no tool plugins.

```bash
zeroclaw config set plugins.enabled true
zeroclaw config set plugins.auto_discover true
```

Restart the daemon (or reload) so the tool set is rebuilt.

## 4. Store the API key

`zeroclaw plugin info deepseek-search` prints the instance key. It is derived
from the package and tool name, so it is stable across reinstall:

```bash
zeroclaw plugin info deepseek-search
# Config entry key (Tool): zpi1_WyJkZWVwc2Vlay1zZWFyY2giLCJ0b29sIiwiZGVlcHNlZWstc2VhcmNoIl0

zeroclaw config set \
  plugins.entries.zpi1_WyJkZWVwc2Vlay1zZWFyY2giLCJ0b29sIiwiZGVlcHNlZWstc2VhcmNoIl0.config.api_key
```

Omit the value to get a masked prompt. The key is encrypted at rest
(`enc2:…`); it never enters the model-visible `__config`.

Optional defaults on the same row:

```bash
zeroclaw config set plugins.entries.<zpi1_key>.config.count 10
zeroclaw config set plugins.entries.<zpi1_key>.config.need_content false
```

## 5. Approval

A plugin tool is not in the built-in read-only auto-approve set. At less than
full autonomy each call asks for approval: the CLI prompts on the terminal, and
channels send an approval card. **A headless daemon with no approver denies the
call.**

To let an agent search unattended, extend its risk profile. `config set`
replaces the whole list, so repeat the entries you already have (the shipped
image bakes its own list):

```bash
zeroclaw config set --no-interactive risk_profiles.default.auto_approve \
  '["file_read","file_write","file_edit","memory_recall","memory_store","web_search_tool","web_fetch","calculator","glob_search","content_search","image_info","weather","git_operations","deepseek_search"]'
```

Check the current value first if the deployment has customised it:

```bash
zeroclaw config get risk_profiles.default.auto_approve
```

## 6. Verify

```bash
zeroclaw plugin list --verify
zeroclaw plugin info deepseek-search
```

Expected: `Loads: yes. The component instantiates against this host's WIT
world.` Before the key is set, the verdict is `does not load … missing required
property 'api_key'` — that is the required-config check, not an ABI failure.

Registration check without invoking a model: with the gateway running and
pairing disabled, `GET /api/tools` lists the assembled registry.

```bash
curl -s http://127.0.0.1:42617/api/tools | python3 -c \
  'import json,sys; [print(t["name"]) for t in json.load(sys.stdin)["tools"]]' | sort
```

`deepseek_search` should appear. A `401` means `gateway.require_pairing` is on;
use the UI or send a paired bearer token.

End-to-end, ask the agent:

> Use the deepseek_search tool to search for DeepSeek's latest model release
> and list the top 3 results with URLs.

The plugin's log event is `deepseek search completed` with
`{"status":200,"results":N,"need_content":false}` in its attributes. The trace
lives at `<data_dir>/state/runtime-trace.jsonl`:

```bash
find ~/.zeroclaw -name runtime-trace.jsonl
grep -h 'deepseek search completed' <path>/runtime-trace.jsonl | tail -3
```

## Container deployment (Docker / Raspberry Pi)

The official images are multi-arch (`linux/amd64`, `linux/arm64`) and, from
**v0.8.6**, built with `plugins-wasm-cranelift`. The component is portable
WebAssembly, so build it once and copy it; no ARM rebuild is needed.

Check the running image before installing:

```bash
docker exec zeroclaw zeroclaw --version       # 0.8.6+
docker exec zeroclaw zeroclaw plugin --help   # must list plugin subcommands
docker exec zeroclaw uname -m                 # aarch64, not armv7l
```

The image keeps state under `/zeroclaw-data`; config is
`/zeroclaw-data/.zeroclaw/config.toml` and plugins are
`/zeroclaw-data/.zeroclaw/plugins/`. An installed package survives restarts,
container recreation, and image upgrades as long as that volume is kept.

### Named volume

Stage the package inside the data volume, but **not** in the plugins directory,
then install it:

```bash
PKG=/zeroclaw-data/pkg-deepseek-search-$(date +%s)
docker cp deepseek-search "zeroclaw:$PKG"
docker exec zeroclaw zeroclaw plugin install "$PKG"

docker exec zeroclaw zeroclaw config set plugins.enabled true
docker exec zeroclaw zeroclaw config set plugins.auto_discover true
docker exec -it zeroclaw zeroclaw config set \
  plugins.entries.zpi1_WyJkZWVwc2Vlay1zZWFyY2giLCJ0b29sIiwiZGVlcHNlZWstc2VhcmNoIl0.config.api_key
docker restart zeroclaw
docker exec zeroclaw zeroclaw plugin list --verify
```

Staging under `/zeroclaw-data` avoids the bind-mount ownership question. The
staged copy is not part of the installed package; remove it with a throwaway
container if you care:

```bash
docker run --rm -v zeroclaw-data:/data alpine rm -rf /data/pkg-deepseek-search-*
```

Compose prefixes named volumes with the project name, so the volume may be
`zeroclaw_zeroclaw-data`.

### Bind mount, with or without a custom user

```yaml
services:
  zeroclaw:
    build: .
    user: "1000:1000"
    volumes:
      - ./data:/zeroclaw-data
```

Config is `./data/.zeroclaw/config.toml`; plugins are
`./data/.zeroclaw/plugins/`. Stage the package inside the data dir (not in the
plugins directory), then install:

```bash
mkdir -p ./data/pkg/deepseek-search
tar -xzf ~/deepseek-search-0.1.0.tar.gz -C ./data/pkg/deepseek-search --strip-components=1
docker compose exec zeroclaw zeroclaw plugin install /zeroclaw-data/pkg/deepseek-search
```

Install copies the package to `./data/.zeroclaw/plugins/deepseek-search/`, so
the staged copy can then be deleted from the host. Because the container runs
as the host user, the installed files are owned by that user and no `chown` is
needed — the data directory just has to be writable by that uid. The stock
image instead runs as uid 65534; if you did not override the user, make the
bind mount writable by 65534 or use a named volume.

Do not drop the package straight into `./data/.zeroclaw/plugins/`: discovery
lists it, but no `[[plugins.entries]]` row exists, so it has no egress grant and
no secret storage, and `plugin install` then refuses to replace it.

The distroless image has no shell, so use `docker exec zeroclaw zeroclaw …`
rather than `sh`, `ls`, or `rm`. The `:debian` tag includes a shell.

### Memory and limits

The sample Compose file caps the container at 512M while the plugin guest may
grow to `plugins.limits.max_memory_mb` (default 256). On a 2 GB Pi that is
workable; 1 GB is a safer ceiling, or lower the guest limit:

```bash
zeroclaw config set plugins.limits.max_memory_mb 64
```

The default wall-clock deadline is 30 s and the plugin connects with a 5 s
timeout, so `need_content` with `count: 50` is the only setting likely to
approach the limit.

## Relationship to the built-in `web_search_tool`

Both tools can be enabled at once. They have distinct names and no shared
configuration, so neither shadows the other.

| | built-in `web_search_tool` | `deepseek_search` |
| --- | --- | --- |
| Backend | `[web_search].search_provider`, default `duckduckgo` (HTML scrape) | DeepSeek Search API |
| Parameters | `query` only | `query`, `count`, `need_content`, `ori_question` |
| Output | titles, URLs, descriptions | summary, date, relevance, authority, optional page text |
| Approval | in the default `auto_approve` list | not auto-approved |
| Limits | wrapped in the host rate limiter | host fuel/time/memory limits, DeepSeek's 100 QPS |

If DeepSeek should be *the* search path, disable the built-in:

```bash
zeroclaw config set web_search.enabled false
```

Minimal tool selection (`[tools] optional = []`) also removes the built-in but
not the plugin: plugins are not gated by `[tools].optional`.

## Upgrades and removal

```bash
# same version, new bytes (copy the new package to a fresh staging path first)
zeroclaw plugin update deepseek-search --from /zeroclaw-data/pkg-deepseek-search-<new>

# remove the package (the config row and stored key are kept)
zeroclaw plugin remove deepseek-search

# stop loading every plugin, without uninstalling
zeroclaw config set plugins.enabled false
```

`plugin remove` keeps the instance row, so a reinstall reuses the key and egress
grant. A replacement that requests more authority than the installed version
needs explicit `--allow` flags; see the host's plugin documentation.

## Troubleshooting

| Symptom | Cause |
| --- | --- |
| `plugin` is not a recognized subcommand | No plugin host in this build; use a release archive or rebuild with `--features plugins-wasm-cranelift`. |
| Not in `plugin list` | Plugin system disabled, malformed manifest, missing `wasm_path`, or signature policy rejected it; check the startup log. |
| Listed but the tool never appears | `plugins.auto_discover` is false, or the daemon has not restarted. |
| `plugin info` reports a config violation | Required `api_key` is unset, or values do not match the schema. |
| WIT / instantiation error | Component built against a different `wit/v0`; rebuild against the host's contract. |
| Call denied, no approver | Headless deployment without `deepseek_search` in `auto_approve`. |
| `did not complete` / egress error | No `api.deepseek.com` grant on the instance row; run `zeroclaw plugin list` for the gap and repair command. |
| `HTTP 401` | Key missing, invalid, or revoked. |
| `HTTP 402` | DeepSeek account balance is empty. |
| `HTTP 429` | Rate limited (100 Search QPS per account); retry shortly. |
