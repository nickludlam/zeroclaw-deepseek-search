# Security policy

## Supported versions

| Version | Supported |
| --- | --- |
| 0.1.x | Yes |

A plugin is compiled into a component and installed into a host; fixes ship as a
new package version. Older versions are not backported.

## Reporting a vulnerability

Please report suspected vulnerabilities privately through this repository's
**Security → Report a vulnerability** advisory form rather than a public issue.
Include the plugin version, host version, the manifest, and reproduction steps.
Do not include live API keys or other secrets.

## Design boundaries

- **Sandbox.** The plugin runs as a WebAssembly component in a store with no
  filesystem preopens, no ambient network, and per-call fuel, memory, and
  wall-clock limits. It reaches the host only through the imports its world
  declares.
- **Network.** `http_client` grants the `wasi:http` surface; the operator's
  `plugins.entries.<instance>.egress_hosts` decides which destinations are
  reachable. This package requests only `api.deepseek.com`. The sandbox does not
  bound *where* a granted plugin sends data, so trust in the code matters as
  much as the grant.
- **Credentials.** `api_key` is a schema-marked secret. The host removes it from
  the model-visible `__config` object and serves it through the scoped
  `secrets.get` import only during an authorized `execute` frame. A trusted
  guest necessarily receives the plaintext; the host cannot enforce
  non-retention after returning it.
- **Untrusted output.** Search results are attacker-controllable web content.
  The plugin renders them as text; they must be treated as data, never as
  instructions to the model or the operator.
- **Logging.** The plugin emits one `log-record` per call with the HTTP status,
  result count, and the `need_content` flag. It never logs the query, the API
  key, or page content.

## Operator hardening

- Keep `signature_mode = "strict"` with a publisher key you trust, and verify
  the `wasm_sha256` in a signed manifest matches the component you installed.
- Grant only `api.deepseek.com` in the instance's `egress_hosts`; do not add
  wildcards.
- Store the key with the masked `config set` prompt, not on a command line.
- Decide deliberately whether `deepseek_search` belongs in an agent's
  `auto_approve` list; approval is the operator's control point for outbound
  data.
- Rotate the DeepSeek key if it may have been exposed. The stored value is
  encrypted at rest but decryptable by the host's config secret.
