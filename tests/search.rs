//! Host-run tests for the wire logic. No wasm toolchain is needed: the
//! component glue in `src/lib.rs` is exercised by the plugin host, while
//! everything it calls is covered here with plain `cargo test`.

use deepseek_search::search::{
    build_request, format_results, parse_response, status_error, SearchOptions, SearchResponse,
    SearchResult, MAX_CONTENT_CHARS, MAX_COUNT, MAX_QUERY_CHARS,
};

const OPTIONS: SearchOptions = SearchOptions {
    count: 5,
    need_content: false,
};

fn body_json(body: &str) -> serde_json::Value {
    serde_json::from_str(body).expect("request body is JSON")
}

#[test]
fn request_trims_query_and_sends_resolved_options() {
    let body = build_request("  rust async  ", None, OPTIONS).expect("valid request");
    let json = body_json(&body);

    assert_eq!(json["query"], "rust async");
    assert_eq!(json["count"], 5);
    assert_eq!(json["need_content"], false);
    assert!(json.get("ori_question").is_none());
}

#[test]
fn request_omits_a_blank_ori_question() {
    let body = build_request("rust", Some("   "), OPTIONS).expect("valid request");
    assert!(body_json(&body).get("ori_question").is_none());

    let body =
        build_request("rust", Some(" how do I write a server? "), OPTIONS).expect("valid request");
    assert_eq!(body_json(&body)["ori_question"], "how do I write a server?");
}

#[test]
fn request_rejects_an_empty_query() {
    let error = build_request("   ", None, OPTIONS).expect_err("empty query is rejected");
    assert!(error.contains("must not be empty"), "{error}");
}

#[test]
fn request_rejects_an_over_long_query_by_characters() {
    let query = "é".repeat(MAX_QUERY_CHARS + 1);
    let error = build_request(&query, None, OPTIONS).expect_err("query is too long");
    assert!(error.contains("129 characters"), "{error}");

    // 128 multi-byte characters is still within the API's limit.
    let query = "é".repeat(MAX_QUERY_CHARS);
    assert!(build_request(&query, None, OPTIONS).is_ok());
}

#[test]
fn request_rejects_an_out_of_range_count() {
    for count in [0, MAX_COUNT + 1] {
        let options = SearchOptions {
            count,
            need_content: false,
        };
        let error = build_request("rust", None, options).expect_err("count is out of range");
        assert!(error.contains("between 1 and 50"), "{error}");
    }
}

#[test]
fn response_parses_every_reported_field() {
    let body = r#"{
        "results": [{
            "rerank_score": 2.4,
            "title": "DeepSeek V4 released",
            "url": "https://example.com/v4",
            "snippet": "short",
            "summary": "long summary",
            "content": "page text",
            "site_name": "Example News",
            "site_icon": "https://example.com/favicon.ico",
            "date": "2026-06-01T00:00:00+00:00",
            "authority": 3
        }]
    }"#;
    let response = parse_response(body).expect("valid response");
    let result = &response.results[0];

    assert_eq!(result.title, "DeepSeek V4 released");
    assert_eq!(result.url, "https://example.com/v4");
    assert_eq!(result.snippet, "short");
    assert_eq!(result.summary, "long summary");
    assert_eq!(result.content, "page text");
    assert_eq!(result.site_name.as_deref(), Some("Example News"));
    assert_eq!(result.date.as_deref(), Some("2026-06-01T00:00:00+00:00"));
    assert_eq!(result.rerank_score, Some(2.4));
    assert_eq!(result.authority, Some(3));
}

#[test]
fn response_tolerates_unknown_and_missing_fields() {
    let body = r#"{"results":[{"title":"only a title","future_field":true}],"extra":1}"#;
    let response = parse_response(body).expect("valid response");
    let result = &response.results[0];

    assert_eq!(result.title, "only a title");
    assert!(result.url.is_empty());
    assert!(result.summary.is_empty());
    assert_eq!(result.rerank_score, None);
    assert_eq!(result.site_name, None);
}

#[test]
fn response_accepts_an_empty_result_list() {
    let response = parse_response(r#"{"results":[]}"#).expect("valid response");
    assert!(response.results.is_empty());
}

#[test]
fn response_rejects_a_body_that_is_not_json() {
    let error = parse_response("<html>bad gateway</html>").expect_err("not JSON");
    assert!(error.contains("not a valid search response"), "{error}");

    let error = parse_response("   ").expect_err("empty body");
    assert!(error.contains("empty response body"), "{error}");
}

#[test]
fn rendering_lists_titles_urls_metadata_and_summary() {
    let response = SearchResponse {
        results: vec![SearchResult {
            title: "DeepSeek V4 released".into(),
            url: "https://example.com/v4".into(),
            summary: "long summary".into(),
            snippet: "short".into(),
            site_name: Some("Example News".into()),
            date: Some("2026-06-01T00:00:00+00:00".into()),
            rerank_score: Some(2.4),
            authority: Some(3),
            ..SearchResult::default()
        }],
    };

    let output = format_results("deepseek v4", &response);

    assert!(output.starts_with("Found 1 result for \"deepseek v4\"."));
    assert!(output.contains("1. DeepSeek V4 released"));
    assert!(output.contains("URL: https://example.com/v4"));
    assert!(output.contains(
        "Site: Example News · Published: 2026-06-01 · Relevance: 2.4/3 · Authority: 3/4"
    ));
    assert!(output.contains("long summary"));
    assert!(!output.contains("short"), "summary wins over snippet");
}

#[test]
fn rendering_falls_back_to_the_snippet_without_a_summary() {
    let response = SearchResponse {
        results: vec![SearchResult {
            title: "Title".into(),
            url: "https://example.com".into(),
            snippet: "display only".into(),
            ..SearchResult::default()
        }],
    };

    assert!(format_results("q", &response).contains("display only"));
}

#[test]
fn rendering_says_so_when_nothing_matched() {
    let output = format_results("nothing", &SearchResponse::default());
    assert!(output.starts_with("No results for \"nothing\"."));
    assert!(output.contains("broader terms"));
}

#[test]
fn rendering_includes_page_text_only_when_the_api_returned_it() {
    let mut result = SearchResult {
        title: "Title".into(),
        url: "https://example.com".into(),
        content: "line one\nline two".into(),
        ..SearchResult::default()
    };
    let output = format_results(
        "q",
        &SearchResponse {
            results: vec![result.clone()],
        },
    );
    assert!(output.contains("Page text:"));
    assert!(output.contains("   line one\n   line two\n"));

    result.content.clear();
    let output = format_results(
        "q",
        &SearchResponse {
            results: vec![result],
        },
    );
    assert!(!output.contains("Page text:"));
}

#[test]
fn rendering_caps_page_text_per_result() {
    let response = SearchResponse {
        results: vec![SearchResult {
            title: "Title".into(),
            url: "https://example.com".into(),
            content: "x".repeat(MAX_CONTENT_CHARS + 10),
            ..SearchResult::default()
        }],
    };

    let output = format_results("q", &response);
    assert!(output.contains("[page text truncated]"));
    assert!(output.len() < MAX_CONTENT_CHARS + 500);
}

#[test]
fn rendering_omits_unknown_metadata() {
    let response = SearchResponse {
        results: vec![SearchResult {
            title: "Title".into(),
            url: "https://example.com".into(),
            authority: Some(-1),
            ..SearchResult::default()
        }],
    };

    let output = format_results("q", &response);
    assert!(!output.contains("Authority:"));
    assert!(!output.contains("Relevance:"));
    assert!(!output.contains("Published:"));
}

#[test]
fn status_errors_include_the_upstream_message() {
    let body = r#"{"error":{"message":"'count' must be between 1 and 50","type":"invalid_request_error","param":null,"code":"invalid_request_error"}}"#;
    let message = status_error(400, body);

    assert!(message.contains("400"), "{message}");
    assert!(
        message.contains("'count' must be between 1 and 50"),
        "{message}"
    );
}

#[test]
fn status_errors_map_the_actionable_cases() {
    assert!(status_error(401, "").contains("API key"));
    assert!(status_error(402, "").contains("balance"));
    assert!(status_error(429, "").contains("rate limit"));
    assert!(status_error(504, "").contains("upstream"));
    assert!(status_error(418, "").contains("418"));
}

#[test]
fn status_errors_survive_a_non_json_body() {
    let message = status_error(502, "<html>bad gateway</html>");
    assert!(message.contains("502"), "{message}");
    assert!(!message.contains("bad gateway"), "raw body is not echoed");
}
