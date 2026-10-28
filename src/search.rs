//! Wire logic for the DeepSeek Search API.
//!
//! Everything that is not WebAssembly glue lives here: request validation,
//! request encoding, response parsing, result rendering, and status mapping.
//! The module has no wasm dependency, so `cargo test` exercises all of it
//! natively; `lib.rs` only reads the API key, performs one HTTP call, and
//! forwards the strings this module produces.
//!
//! Contract: <https://api-docs.deepseek.com/guides/search>.

use serde::{Deserialize, Serialize};

/// The DeepSeek Search endpoint.
pub const SEARCH_ENDPOINT: &str = "https://api.deepseek.com/v1/tools/search";

/// The API accepts a query of 1 to 128 Unicode characters.
pub const MAX_QUERY_CHARS: usize = 128;

/// Default number of results when neither the model nor the operator chooses.
pub const DEFAULT_COUNT: u32 = 10;

/// The API caps `count` at 50.
pub const MAX_COUNT: u32 = 50;

/// Per-result cap on page text, so `need_content` cannot flood a model.
pub const MAX_CONTENT_CHARS: usize = 4_000;

/// A resolved search request: model arguments merged with operator defaults.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SearchOptions {
    /// Number of results to ask for, between 1 and [`MAX_COUNT`].
    pub count: u32,
    /// Whether to ask for extracted page text in addition to summaries.
    pub need_content: bool,
}

/// One result in a DeepSeek Search response.
///
/// Every field is defaulted because the API may omit an optional one, and
/// unknown fields are ignored so a future additive field cannot break the
/// plugin.
#[derive(Debug, Clone, Default, Deserialize, PartialEq)]
pub struct SearchResult {
    /// Result title.
    #[serde(default)]
    pub title: String,
    /// Result URL.
    #[serde(default)]
    pub url: String,
    /// Short, display-oriented summary.
    #[serde(default)]
    pub snippet: String,
    /// Long, model-oriented summary.
    #[serde(default)]
    pub summary: String,
    /// Extracted page text; empty unless `need_content` was requested.
    #[serde(default)]
    pub content: String,
    /// Site name when the API resolves one.
    #[serde(default)]
    pub site_name: Option<String>,
    /// Publication date as RFC 3339 UTC when known.
    #[serde(default)]
    pub date: Option<String>,
    /// Relevance score from 0 to 3.
    #[serde(default)]
    pub rerank_score: Option<f64>,
    /// Authority tier from 0 to 4, with `-1` meaning unknown.
    #[serde(default)]
    pub authority: Option<i64>,
}

/// A successful DeepSeek Search response.
#[derive(Debug, Clone, Default, Deserialize, PartialEq)]
pub struct SearchResponse {
    /// Ranked results; empty when the search matched nothing.
    #[serde(default)]
    pub results: Vec<SearchResult>,
}

#[derive(Debug, Serialize)]
struct SearchRequestBody<'a> {
    query: &'a str,
    count: u32,
    need_content: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    ori_question: Option<&'a str>,
}

/// Validate the caller's arguments and encode the JSON request body.
///
/// A returned `Err` is a caller mistake the model can correct (empty query,
/// out-of-range count), not a broken plugin state.
pub fn build_request(
    query: &str,
    ori_question: Option<&str>,
    options: SearchOptions,
) -> Result<String, String> {
    let query = query.trim();
    let query_chars = query.chars().count();
    if query_chars == 0 {
        return Err("query must not be empty".to_string());
    }
    if query_chars > MAX_QUERY_CHARS {
        return Err(format!(
            "query is {query_chars} characters; the DeepSeek Search API allows at most \
             {MAX_QUERY_CHARS}"
        ));
    }
    if !(1..=MAX_COUNT).contains(&options.count) {
        return Err(format!(
            "count must be between 1 and {MAX_COUNT} (got {})",
            options.count
        ));
    }

    // An absent, empty, or whitespace-only original question is the same as
    // not sending the field; the API treats `null` that way too.
    let ori_question = ori_question
        .map(str::trim)
        .filter(|value| !value.is_empty());
    if let Some(question) = ori_question {
        let question_chars = question.chars().count();
        if question_chars > MAX_QUERY_CHARS {
            return Err(format!(
                "ori_question is {question_chars} characters; the DeepSeek Search API allows \
                 at most {MAX_QUERY_CHARS}"
            ));
        }
    }

    let body = SearchRequestBody {
        query,
        count: options.count,
        need_content: options.need_content,
        ori_question,
    };
    serde_json::to_string(&body)
        .map_err(|error| format!("failed to encode the search request: {error}"))
}

/// Parse a successful (HTTP 200) response body.
pub fn parse_response(body: &str) -> Result<SearchResponse, String> {
    if body.trim().is_empty() {
        return Err("DeepSeek Search returned an empty response body".to_string());
    }
    serde_json::from_str(body).map_err(|error| {
        format!("DeepSeek Search returned a body that is not a valid search response: {error}")
    })
}

/// Render results as compact Markdown for the model.
///
/// An empty result list is a normal outcome, not a failure: the model is told
/// there were no matches and how to widen the query.
pub fn format_results(query: &str, response: &SearchResponse) -> String {
    let query = query.trim();
    let total = response.results.len();
    if total == 0 {
        return format!(
            "No results for \"{query}\". The search may be too narrow: try broader terms, \
             drop restrictive operators, or remove a site filter."
        );
    }

    let mut output = String::new();
    output.push_str(&format!(
        "Found {total} result{} for \"{query}\".\n",
        if total == 1 { "" } else { "s" }
    ));

    for (index, result) in response.results.iter().enumerate() {
        let title = if result.title.trim().is_empty() {
            "(untitled)"
        } else {
            result.title.trim()
        };
        let url = if result.url.trim().is_empty() {
            "(no URL)"
        } else {
            result.url.trim()
        };

        output.push_str(&format!("\n{}. {title}\n", index + 1));
        output.push_str(&format!("   URL: {url}\n"));

        let metadata = metadata_line(result);
        if !metadata.is_empty() {
            output.push_str(&format!("   {metadata}\n"));
        }

        // The long summary is written for a model; the snippet is the
        // display fallback when the summary is unavailable.
        let summary = if result.summary.trim().is_empty() {
            result.snippet.trim()
        } else {
            result.summary.trim()
        };
        if !summary.is_empty() {
            push_indented(&mut output, summary, "   ");
        }

        if !result.content.trim().is_empty() {
            output.push_str("\n   Page text:\n");
            let content = cap_chars(result.content.trim(), MAX_CONTENT_CHARS);
            push_indented(&mut output, &content, "   ");
        }
    }

    output
}

/// Turn a non-200 status and its body into an actionable message.
///
/// DeepSeek returns OpenAI-compatible errors with a `message`; include it when
/// present, and never include the request headers or key.
pub fn status_error(status: u16, body: &str) -> String {
    let detail = api_message(body)
        .map(|message| format!(": {message}"))
        .unwrap_or_default();

    match status {
        400 => format!(
            "DeepSeek Search rejected the request (400){detail}. A query operator or value may be \
             unsupported; fix the query and retry."
        ),
        401 => format!(
            "DeepSeek Search rejected the API key (401){detail}. The plugin's `api_key` value is \
             missing, invalid, or revoked; it must be corrected by the operator."
        ),
        402 => format!(
            "DeepSeek Search could not run because the DeepSeek account balance is insufficient \
             (402){detail}. Ask the operator to top up the account."
        ),
        403 => format!(
            "DeepSeek Search refused the request under its security policy (403){detail}. \
             Retrying the same query will not help."
        ),
        404 => format!(
            "DeepSeek Search could not retrieve a usable page (404){detail}. The result may be \
             blocked, missing, or not extractable."
        ),
        408 => format!(
            "DeepSeek Search timed out reading the request (408){detail}. Retry once; if it \
             repeats, shorten the request."
        ),
        413 => format!(
            "DeepSeek Search rejected the request as too large (413){detail}. Reduce `count` or \
             turn `need_content` off."
        ),
        429 => format!(
            "DeepSeek Search rate limit exceeded (429){detail}. DeepSeek allows 100 Search \
             requests per second per account; wait briefly and retry."
        ),
        500..=599 => format!(
            "DeepSeek Search failed upstream (HTTP {status}){detail}. This is a server-side \
             error; retry after a short wait."
        ),
        other => format!(
            "DeepSeek Search returned an unexpected HTTP status {other}{detail}. Retry only if \
             the status suggests a transient failure."
        ),
    }
}

/// Extract the `error.message` field from an OpenAI-compatible error body.
fn api_message(body: &str) -> Option<String> {
    #[derive(Deserialize)]
    struct Envelope {
        error: Option<ApiError>,
    }
    #[derive(Deserialize)]
    struct ApiError {
        message: Option<String>,
    }

    serde_json::from_str::<Envelope>(body)
        .ok()
        .and_then(|envelope| envelope.error)
        .and_then(|error| error.message)
        .map(|message| message.trim().to_string())
        .filter(|message| !message.is_empty())
}

/// Build the `Site · Published · Relevance · Authority` line, omitting
/// everything the API did not report.
fn metadata_line(result: &SearchResult) -> String {
    let mut parts = Vec::new();
    if let Some(site) = result
        .site_name
        .as_deref()
        .map(str::trim)
        .filter(|site| !site.is_empty())
    {
        parts.push(format!("Site: {site}"));
    }
    if let Some(date) = result
        .date
        .as_deref()
        .map(str::trim)
        .filter(|date| !date.is_empty())
    {
        // The API reports RFC 3339 timestamps; the publication day is the
        // useful part for a model deciding whether a page is current.
        let day = date.split_once('T').map_or(date, |(day, _)| day);
        parts.push(format!("Published: {day}"));
    }
    if let Some(score) = result.rerank_score {
        parts.push(format!("Relevance: {score:.1}/3"));
    }
    if let Some(authority) = result.authority.filter(|authority| *authority >= 0) {
        parts.push(format!("Authority: {authority}/4"));
    }
    parts.join(" · ")
}

/// Append text with every line prefixed, so multi-line summaries and page text
/// stay inside the numbered result block.
fn push_indented(output: &mut String, text: &str, prefix: &str) {
    for line in text.lines() {
        output.push_str(prefix);
        output.push_str(line);
        output.push('\n');
    }
}

/// Truncate by characters (not bytes) and mark the cut.
fn cap_chars(text: &str, max: usize) -> String {
    let mut capped: String = text.chars().take(max).collect();
    if text.chars().count() > max {
        capped.push_str("\n… [page text truncated]");
    }
    capped
}
