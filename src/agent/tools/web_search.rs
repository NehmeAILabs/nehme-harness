use rig::completion::ToolDefinition;
use rig::tool::Tool;
use serde::Deserialize;
use serde_json::json;

use crate::agent::tools::{PermCheck, ToolError, check_perm};
use crate::permission::ask::AskSender;

const DDG_HTML_URL: &str = "https://html.duckduckgo.com/html/";

const DEFAULT_MAX_RESULTS: u64 = 10;

pub(crate) fn web_search_tool_definition() -> ToolDefinition {
    ToolDefinition {
        name: "web_search".to_string(),
        description: "Search the web using DuckDuckGo. Returns a list of results with title, URL, and snippet. Use this to find current information, documentation, or answers to questions that are not in your training data.".to_string(),
        parameters: json!({
            "type": "object",
            "properties": {
                "query": {
                    "type": "string",
                    "description": "The search query"
                },
                "max_results": {
                    "type": "integer",
                    "description": "Maximum number of results to return (default: 10, max: 20)"
                }
            },
            "required": ["query"]
        }),
    }
}

#[derive(Debug, Deserialize)]
pub struct WebSearchArgs {
    pub query: String,
    pub max_results: Option<u64>,
}

#[derive(Clone)]
pub struct WebSearchTool {
    pub permission: Option<PermCheck>,
    pub ask_tx: Option<AskSender>,
    pub max_results: u64,
}

impl WebSearchTool {
    pub fn new(permission: Option<PermCheck>, ask_tx: Option<AskSender>, max_results: u64) -> Self {
        Self {
            permission,
            ask_tx,
            max_results,
        }
    }
}

impl Tool for WebSearchTool {
    const NAME: &'static str = "web_search";
    type Error = ToolError;
    type Args = WebSearchArgs;
    type Output = String;

    async fn definition(&self, _prompt: String) -> ToolDefinition {
        web_search_tool_definition()
    }

    async fn call(&self, args: WebSearchArgs) -> Result<String, ToolError> {
        let coaching =
            check_perm(&self.permission, &self.ask_tx, "web_search", &args.query).await?;

        let max = args
            .max_results
            .unwrap_or(DEFAULT_MAX_RESULTS)
            .min(self.max_results)
            .min(20);

        let client = reqwest::Client::builder()
            .pool_max_idle_per_host(0)
            .user_agent("Mozilla/5.0 (compatible; nehme-harness/1.0)")
            .build()
            .map_err(|e| ToolError::Msg(format!("Failed to build HTTP client: {e}")))?;

        let encoded_query = urlencoding::encode(&args.query);
        let url = format!("{}?q={}&kl=us-en", DDG_HTML_URL, encoded_query);

        let resp = client
            .get(&url)
            .send()
            .await
            .map_err(|e| ToolError::Msg(format!("Search request failed: {e}")))?;

        if !resp.status().is_success() {
            return Err(ToolError::Msg(format!(
                "Search returned status {}",
                resp.status()
            )));
        }

        let html = resp
            .text()
            .await
            .map_err(|e| ToolError::Msg(format!("Failed to read search response: {e}")))?;

        let results = parse_ddg_html(&html, max as usize);

        if results.is_empty() {
            let msg = "No search results found.".to_string();
            return Ok(if let Some(c) = coaching {
                format!("{c}\n\n{msg}")
            } else {
                msg
            });
        }

        let mut output = String::new();
        for (i, r) in results.iter().enumerate() {
            if i > 0 {
                output.push('\n');
            }
            output.push_str(&format!(
                "{}. {}\n   {}\n   {}",
                i + 1,
                r.title,
                r.url,
                r.snippet
            ));
        }

        Ok(if let Some(c) = coaching {
            format!("{c}\n\n{output}")
        } else {
            output
        })
    }
}

#[derive(Debug, Clone)]
struct SearchResult {
    title: String,
    url: String,
    snippet: String,
}

fn parse_ddg_html(html: &str, max: usize) -> Vec<SearchResult> {
    let doc = scraper::Html::parse_document(html);

    let sel_result = match scraper::Selector::parse(".result") {
        Ok(s) => s,
        Err(_) => return Vec::new(),
    };
    let sel_title = match scraper::Selector::parse(".result__a") {
        Ok(s) => s,
        Err(_) => return Vec::new(),
    };
    let sel_snippet = match scraper::Selector::parse(".result__snippet") {
        Ok(s) => s,
        Err(_) => return Vec::new(),
    };

    let mut results = Vec::new();

    for node in doc.select(&sel_result).take(max) {
        let title = node
            .select(&sel_title)
            .next()
            .map(|el| el.text().collect::<String>())
            .unwrap_or_default()
            .trim()
            .to_string();

        let raw_url = node
            .select(&sel_title)
            .next()
            .and_then(|el| el.value().attr("href"))
            .unwrap_or("")
            .trim()
            .to_string();

        let url = if raw_url.starts_with("//duckduckgo.com/l/") {
            extract_ddg_redirect(&raw_url).unwrap_or(raw_url)
        } else if raw_url.starts_with("http") {
            raw_url
        } else {
            continue;
        };

        let snippet = node
            .select(&sel_snippet)
            .next()
            .map(|el| el.text().collect::<String>())
            .unwrap_or_default()
            .trim()
            .to_string();

        if title.is_empty() && snippet.is_empty() {
            continue;
        }

        results.push(SearchResult {
            title,
            url,
            snippet,
        });
    }

    results
}

fn extract_ddg_redirect(url: &str) -> Option<String> {
    let stripped = url.strip_prefix("//duckduckgo.com/l/")?;
    if let Some(query_start) = stripped.find("uddg=") {
        let encoded = &stripped[query_start + 5..];
        let encoded = encoded.split('&').next().unwrap_or(encoded);
        Some(urldecode(encoded))
    } else {
        Some(urldecode(stripped))
    }
}

fn urldecode(s: &str) -> String {
    let mut result = Vec::new();
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let Ok(byte) = u8::from_str_radix(&String::from_utf8_lossy(&bytes[i + 1..i + 3]), 16)
            {
                result.push(byte);
                i += 3;
                continue;
            }
        }
        result.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&result).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_empty_html() {
        let results = parse_ddg_html("<html><body></body></html>", 10);
        assert!(results.is_empty());
    }

    #[test]
    fn test_parse_ddg_results() {
        let html = r#"<html><body>
        <div class="result">
            <a class="result__a" href="https://example.com/page1">Example Title</a>
            <a class="result__snippet">This is a snippet about the result.</a>
        </div>
        <div class="result">
            <a class="result__a" href="https://example.com/page2">Second Result</a>
            <a class="result__snippet">Another snippet here.</a>
        </div>
        </body></html>"#;
        let results = parse_ddg_html(html, 10);
        assert_eq!(results.len(), 2);
        assert_eq!(results[0].title, "Example Title");
        assert_eq!(results[0].url, "https://example.com/page1");
        assert_eq!(results[0].snippet, "This is a snippet about the result.");
    }

    #[test]
    fn test_parse_ddg_redirect_url() {
        let url = "//duckduckgo.com/l/?uddg=https%3A%2F%2Fwww.rust-lang.org%2F";
        let result = extract_ddg_redirect(url);
        assert_eq!(result, Some("https://www.rust-lang.org/".to_string()));
    }

    #[test]
    fn test_max_results_limit() {
        let html = r#"<html><body>
        <div class="result"><a class="result__a" href="https://a.com">A</a><a class="result__snippet">S</a></div>
        <div class="result"><a class="result__a" href="https://b.com">B</a><a class="result__snippet">S</a></div>
        <div class="result"><a class="result__a" href="https://c.com">C</a><a class="result__snippet">S</a></div>
        </body></html>"#;
        let results = parse_ddg_html(html, 2);
        assert_eq!(results.len(), 2);
    }

    #[test]
    fn test_urldecode() {
        assert_eq!(
            urldecode("https%3A%2F%2Fwww.rust-lang.org%2F"),
            "https://www.rust-lang.org/"
        );
    }
}
