//! ZeroClaw tool plugin: web search through the DeepSeek Search API.
//!
//! The operator supplies a DeepSeek API key as a schema-marked secret. Each
//! call performs one `POST https://api.deepseek.com/v1/tools/search` and
//! renders the ranked results as Markdown for the agent.
//!
//! The component glue below is deliberately thin: arguments are validated and
//! rendered by [`search`], which is unit-tested natively. See `README.md` for
//! the operator-facing install and configuration steps.

pub mod search;

#[cfg(target_family = "wasm")]
mod component {
    wit_bindgen::generate!({
        path: "wit/v0",
        world: "tool-plugin",
        features: ["plugins-wit-v0"],
    });

    use crate::search::{
        build_request, format_results, parse_response, status_error, SearchOptions, DEFAULT_COUNT,
        SEARCH_ENDPOINT,
    };
    use exports::zeroclaw::plugin::plugin_info::Guest as PluginInfo;
    use exports::zeroclaw::plugin::tool::{Guest as Tool, ToolResult};
    use std::time::Duration;
    use zeroclaw::plugin::logging::{
        log_record, LogLevel, PluginAction, PluginEvent, PluginOutcome,
    };
    use zeroclaw::plugin::secrets::{get as secret_get, SecretError};

    /// Config property holding the DeepSeek API key (schema-marked secret).
    const API_KEY_PROPERTY: &str = "api_key";
    /// Log namespace for this component's events.
    const FUNCTION_NAME: &str = "deepseek_search::tool::execute";

    /// Operator defaults from the plugin's own config section.
    ///
    /// Both properties are optional: the schema only requires `api_key`.
    #[derive(Debug, serde::Deserialize)]
    #[serde(default, deny_unknown_fields)]
    struct PluginConfig {
        /// Result count used when the model does not pass one.
        count: u32,
        /// Whether page text is requested when the model does not say.
        need_content: bool,
    }

    impl Default for PluginConfig {
        fn default() -> Self {
            Self {
                count: DEFAULT_COUNT,
                need_content: false,
            }
        }
    }

    /// The model's arguments plus the host-injected, schema-validated config.
    #[derive(Debug, serde::Deserialize)]
    struct ExecuteArgs {
        query: String,
        #[serde(default)]
        count: Option<u32>,
        #[serde(default)]
        need_content: Option<bool>,
        #[serde(default)]
        ori_question: Option<String>,
        #[serde(default, rename = "__config")]
        config: PluginConfig,
    }

    struct DeepSeekSearchTool;

    impl PluginInfo for DeepSeekSearchTool {
        /// `Cargo.toml` is the single source for the package identity; CI
        /// checks it against `manifest.toml`, which is the release identity.
        fn plugin_name() -> String {
            env!("CARGO_PKG_NAME").to_string()
        }

        fn plugin_version() -> String {
            env!("CARGO_PKG_VERSION").to_string()
        }
    }

    impl Tool for DeepSeekSearchTool {
        fn name() -> String {
            "deepseek_search".to_string()
        }

        fn description() -> String {
            "Search the live web with the DeepSeek Search API and DeepSeek's own indexed search \
             engine. Use it for current events, documentation, prices, releases, and anything \
             newer than your training data. Returns ranked results with title, URL, site, \
             publication date, a relevance score, and a model-readable summary; set \
             `need_content` to also receive extracted page text. The query may combine search \
             operators: `site:` / `-site:` to include or exclude a site, `\"exact phrase\"` for \
             an exact match, `-word` to exclude a term, `lang:zh,en` for languages, and \
             `after:2025-01-01` / `before:2026-06-01` for publication dates."
                .to_string()
        }

        fn parameters_schema() -> String {
            // Deliberately not closed with `additionalProperties: false`: the
            // host injects its reserved `__config` key into these arguments.
            serde_json::json!({
                "type": "object",
                "properties": {
                    "query": {
                        "type": "string",
                        "minLength": 1,
                        "maxLength": 128,
                        "description": "Search query, 1-128 characters. Combine search operators \
                                        when useful, e.g. `rust async book site:doc.rust-lang.org \
                                        lang:en after:2025`."
                    },
                    "count": {
                        "type": "integer",
                        "minimum": 1,
                        "maximum": 50,
                        "description": "How many ranked results to return (1-50). Defaults to \
                                        the operator's configured value, normally 10."
                    },
                    "need_content": {
                        "type": "boolean",
                        "description": "Also return extracted page text for each result. Much \
                                        larger and slower than summaries; use it only when the \
                                        summaries do not contain the needed detail."
                    },
                    "ori_question": {
                        "type": "string",
                        "minLength": 1,
                        "maxLength": 128,
                        "description": "The user's original question that prompted this search, \
                                        1-128 characters. Improves intent understanding for \
                                        ambiguous or multi-step queries."
                    }
                },
                "required": ["query"]
            })
            .to_string()
        }

        fn execute(args: String) -> Result<ToolResult, String> {
            let args: ExecuteArgs = match serde_json::from_str(&args) {
                Ok(args) => args,
                Err(error) => return Ok(failed(format!("invalid arguments: {error}"))),
            };

            // Model arguments win over operator defaults. Validation happens in
            // `build_request`, so a bad argument is reported as a normal failed
            // call the model can correct rather than as a plugin fault.
            let options = SearchOptions {
                count: args.count.unwrap_or(args.config.count),
                need_content: args.need_content.unwrap_or(args.config.need_content),
            };
            let request_body =
                match build_request(&args.query, args.ori_question.as_deref(), options) {
                    Ok(body) => body,
                    Err(message) => return Ok(failed(message)),
                };

            // The key never enters `__config`; it is reachable only here,
            // during an authorized `execute` frame.
            let api_key = match secret_get(API_KEY_PROPERTY) {
                Ok(api_key) => api_key,
                Err(error) => return Err(secret_failure(error)),
            };

            let sent = waki::Client::new()
                .post(SEARCH_ENDPOINT)
                .headers([
                    ("content-type", "application/json"),
                    ("accept", "application/json"),
                ])
                .header("authorization", format!("Bearer {api_key}"))
                .connect_timeout(Duration::from_secs(5))
                .body(request_body.into_bytes())
                .send();
            let response = match sent {
                Ok(response) => response,
                Err(error) => {
                    return Ok(failed(format!(
                        "the request to DeepSeek Search did not complete: {error}. Check that this \
                         plugin instance is granted the host `api.deepseek.com` and that the host \
                         can reach the internet."
                    )));
                }
            };

            let status = response.status_code();
            let raw_body = match response.body() {
                Ok(body) => body,
                Err(error) => {
                    return Ok(failed(format!(
                        "the DeepSeek Search response body could not be read (HTTP {status}): \
                         {error}"
                    )));
                }
            };
            let text = String::from_utf8_lossy(&raw_body);

            if !(200..300).contains(&status) {
                let message = status_error(status, &text);
                log_event(
                    LogLevel::Warn,
                    PluginAction::Fail,
                    PluginOutcome::Failure,
                    None,
                    &message,
                );
                return Ok(failed(message));
            }

            let response = match parse_response(&text) {
                Ok(response) => response,
                Err(message) => {
                    log_event(
                        LogLevel::Warn,
                        PluginAction::Fail,
                        PluginOutcome::Failure,
                        None,
                        &message,
                    );
                    return Ok(failed(message));
                }
            };

            let output = format_results(&args.query, &response);
            log_event(
                LogLevel::Info,
                PluginAction::Complete,
                PluginOutcome::Success,
                Some(serde_json::json!({
                    "status": status,
                    "results": response.results.len(),
                    "need_content": options.need_content,
                })),
                "deepseek search completed",
            );

            Ok(ToolResult {
                success: true,
                output,
                error: None,
            })
        }
    }

    /// A failed call the model can see and react to.
    fn failed(message: String) -> ToolResult {
        ToolResult {
            success: false,
            output: String::new(),
            error: Some(message),
        }
    }

    /// Map the scoped-secret failures to an operator-actionable fault.
    ///
    /// Any of these means the instance cannot search at all, so the call is a
    /// plugin fault (`Err`) rather than a call the model can retry its way out
    /// of. The host does not leak resolver details here, and neither do we.
    fn secret_failure(error: SecretError) -> String {
        let reason = match error {
            SecretError::AccessDenied => "this instance does not hold the `config_read` grant",
            SecretError::NotFound => {
                "no schema-marked `api_key` secret is configured for this instance"
            }
            SecretError::Unavailable => "the host config service was unavailable during this call",
        };
        format!("DeepSeek Search cannot run: {reason}. Ask the operator to configure the plugin.")
    }

    /// Emit one structured event. Never includes the query, the key, or page
    /// content: `attrs` carries only status, result count, and options.
    fn log_event(
        level: LogLevel,
        action: PluginAction,
        outcome: PluginOutcome,
        attrs: Option<serde_json::Value>,
        message: &str,
    ) {
        log_record(
            level,
            &PluginEvent {
                function_name: FUNCTION_NAME.to_string(),
                action,
                outcome: Some(outcome),
                duration_ms: None,
                attrs: attrs.map(|value| value.to_string()),
                message: message.to_string(),
            },
        );
    }

    export!(DeepSeekSearchTool);
}
