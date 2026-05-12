use rig::completion::ToolDefinition;
use rig::tool::Tool;
use serde::Deserialize;
use serde_json::json;

use crate::agent::tools::{PermCheck, ToolError, check_perm};
use crate::permission::ask::AskSender;

#[cfg(feature = "context")]
use crate::extras::context::{auto_index_output, event, record_event};

const DEFAULT_MAX_LENGTH: u64 = 50_000;
const USER_AGENT: &str =
    "Mozilla/5.0 (compatible; nehme-harness/1.0; +https://github.com/NehmeAILabs/nehme-harness)";

const STRIP_TAGS: &[&str] = &[
    "script", "style", "nav", "footer", "header", "aside", "iframe", "noscript",
];

pub(crate) fn web_fetch_tool_definition() -> ToolDefinition {
    ToolDefinition {
        name: "web_fetch".to_string(),
        description: "Fetch a URL and return its content as clean Markdown. Sanitizes HTML by removing scripts, styles, navigation, and other non-content elements. Use this to read web pages, documentation, or API references.".to_string(),
        parameters: json!({
            "type": "object",
            "properties": {
                "url": {
                    "type": "string",
                    "description": "The URL to fetch"
                },
                "max_length": {
                    "type": "integer",
                    "description": "Maximum character length of the returned content (default: 50000)"
                },
                "intent": {
                    "type": "string",
                    "description": "What you're looking for in the page. If the page is large, only matching sections are returned (full content is indexed for ctx_search). Omit for full content."
                }
            },
            "required": ["url"]
        }),
    }
}

#[derive(Debug, Deserialize)]
pub struct WebFetchArgs {
    pub url: String,
    pub max_length: Option<u64>,
    #[serde(default)]
    pub intent: Option<String>,
}

#[derive(Clone)]
pub struct WebFetchTool {
    pub permission: Option<PermCheck>,
    pub ask_tx: Option<AskSender>,
    pub max_length: u64,
    pub session_id: String,
}

impl WebFetchTool {
    pub fn new(
        permission: Option<PermCheck>,
        ask_tx: Option<AskSender>,
        max_length: u64,
        session_id: String,
    ) -> Self {
        Self {
            permission,
            ask_tx,
            max_length,
            session_id,
        }
    }
}

impl Tool for WebFetchTool {
    const NAME: &'static str = "web_fetch";
    type Error = ToolError;
    type Args = WebFetchArgs;
    type Output = String;

    async fn definition(&self, _prompt: String) -> ToolDefinition {
        web_fetch_tool_definition()
    }

    async fn call(&self, args: WebFetchArgs) -> Result<String, ToolError> {
        let coaching = check_perm(&self.permission, &self.ask_tx, "web_fetch", &args.url).await?;

        #[cfg(feature = "context")]
        record_event(
            &self.session_id,
            event::TOOL_CALL,
            event::PRIORITY_LOW,
            "web_fetch",
            &args.url,
            &format!(
                "{{\"max_length\":{}}}",
                args.max_length.unwrap_or(DEFAULT_MAX_LENGTH)
            ),
        );

        let max_len = args
            .max_length
            .unwrap_or(DEFAULT_MAX_LENGTH)
            .min(self.max_length)
            .min(200_000);

        let client = reqwest::Client::builder()
            .pool_max_idle_per_host(0)
            .user_agent(USER_AGENT)
            .build()
            .map_err(|e| ToolError::Msg(format!("Failed to build HTTP client: {e}")))?;

        let resp = client
            .get(&args.url)
            .send()
            .await
            .map_err(|e| ToolError::Msg(format!("Fetch request failed: {e}")))?;

        if !resp.status().is_success() {
            return Err(ToolError::Msg(format!(
                "Fetch returned status {} for {}",
                resp.status(),
                args.url
            )));
        }

        let content_type = resp
            .headers()
            .get("content-type")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("text/html")
            .to_string();

        let body = resp
            .text()
            .await
            .map_err(|e| ToolError::Msg(format!("Failed to read response body: {e}")))?;

        let is_html = content_type.contains("text/html");

        let result = if is_html {
            html_to_markdown(&body, &args.url)
        } else if content_type.contains("text/plain")
            || content_type.contains("application/json")
            || content_type.contains("text/markdown")
            || content_type.contains("text/csv")
        {
            body.clone()
        } else {
            format!("[Binary content: {content_type}, {0} bytes]", body.len())
        };

        let (truncated, was_truncated) = if result.len() > max_len as usize {
            let mut t = truncate_to_char_boundary(&result, max_len as usize).to_string();
            t.push_str("\n\n... [truncated]");
            (t, true)
        } else {
            (result.clone(), false)
        };

        #[cfg(feature = "context")]
        {
            if was_truncated {
                auto_index_output(
                    &self.session_id,
                    "web_fetch",
                    &args.url,
                    &result,
                    &truncated,
                );
            }
        }

        #[cfg(feature = "context")]
        {
            if let Some(ref intent) = args.intent
                && truncated.len() > 5000
            {
                let filtered = crate::extras::context::intent_filter(&truncated, intent);
                auto_index_output(
                    &self.session_id,
                    "web_fetch",
                    &args.url,
                    &truncated,
                    &filtered,
                );
                let out = format!(
                    "{}\n\n[{} bytes original, {} bytes returned — full content indexed, use ctx_search to retrieve]",
                    filtered,
                    truncated.len(),
                    filtered.len()
                );
                return Ok(if let Some(c) = coaching {
                    format!("{c}\n\n{out}")
                } else {
                    out
                });
            }
        }

        Ok(if let Some(c) = coaching {
            format!("{c}\n\n{truncated}")
        } else {
            truncated
        })
    }
}

fn truncate_to_char_boundary(s: &str, max_len: usize) -> &str {
    if max_len >= s.len() {
        return s;
    }

    let mut end = max_len;
    while end > 0 && !s.is_char_boundary(end) {
        end -= 1;
    }
    &s[..end]
}

fn html_to_markdown(html: &str, url: &str) -> String {
    let doc = scraper::Html::parse_document(html);

    let title_sel = scraper::Selector::parse("title").unwrap();
    let title = doc
        .select(&title_sel)
        .next()
        .map(|el| el.text().collect::<String>())
        .unwrap_or_default()
        .trim()
        .to_string();

    let content = select_main_content(&doc);

    let stripped_html = strip_unwanted_elements(&content);

    let markdown = html2text::config::plain()
        .string_from_read(stripped_html.as_bytes(), 120)
        .unwrap_or_else(|_| stripped_html.clone());

    let mut output = String::new();

    if !title.is_empty() {
        output.push_str("# ");
        output.push_str(&title);
        output.push('\n');
    }

    output.push_str("URL Source: ");
    output.push_str(url);
    output.push_str("\n\n");

    let trimmed_md = markdown.trim();
    if !trimmed_md.is_empty() {
        output.push_str(trimmed_md);
    }

    output
}

fn select_main_content(doc: &scraper::Html) -> String {
    let selectors = [
        "main",
        "article",
        "[role='main']",
        "#content",
        "#main-content",
        ".post-content",
        ".article-content",
        ".entry-content",
    ];

    for selector_str in &selectors {
        if let Ok(sel) = scraper::Selector::parse(selector_str) {
            if let Some(el) = doc.select(&sel).next() {
                return el.html();
            }
        }
    }

    if let Ok(sel) = scraper::Selector::parse("body") {
        if let Some(el) = doc.select(&sel).next() {
            return el.html();
        }
    }

    doc.root_element().html()
}

fn strip_unwanted_elements(html: &str) -> String {
    let mut result = html.to_string();

    for tag in STRIP_TAGS {
        let open_tag = format!("<{tag}");
        let close_tag = format!("</{tag}>");

        'outer: loop {
            let start = match result.find(&open_tag as &str) {
                Some(s) => s,
                None => break 'outer,
            };

            let tag_end = match result[start..].find('>') {
                Some(offset) => start + offset + 1,
                None => break 'outer,
            };

            let close_start = match result[tag_end..].find(&close_tag as &str) {
                Some(offset) => tag_end + offset,
                None => {
                    result.replace_range(start.., "");
                    break 'outer;
                }
            };

            let close_end = close_start + close_tag.len();
            result.replace_range(start..close_end, "");
        }
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_html_to_markdown_basic() {
        let html = r#"<html><head><title>Test Page</title></head><body><p>Hello, world!</p></body></html>"#;
        let result = html_to_markdown(html, "https://example.com");
        assert!(result.contains("Test Page"));
        assert!(result.contains("Hello, world!"));
        assert!(result.contains("https://example.com"));
    }

    #[test]
    fn test_html_to_markdown_strips_script() {
        let html = r#"<html><head><title>Test</title></head><body>
            <p>Content here</p>
        </body></html>"#;
        let result = html_to_markdown(html, "https://example.com");
        assert!(result.contains("Content here"));
    }

    #[test]
    fn test_html_to_markdown_strips_nav() {
        let html = r#"<html><body>
            <nav><a href="/">Home</a><a href="/about">About</a></nav>
            <main><p>Main content</p></main>
        </body></html>"#;
        let result = html_to_markdown(html, "https://example.com");
        assert!(result.contains("Main content"));
    }

    #[test]
    fn test_html_to_markdown_headings() {
        let html = r#"<html><body>
            <h1>Title</h1>
            <h2>Section</h2>
            <p>Text</p>
        </body></html>"#;
        let result = html_to_markdown(html, "https://example.com");
        assert!(result.contains("Title"));
        assert!(result.contains("Section"));
    }

    #[test]
    fn test_strip_script_tags() {
        let html = r#"<html><body><script>var x = 1;</script><p>Content</p></body></html>"#;
        let result = strip_unwanted_elements(html);
        assert!(!result.contains("<script>"));
        assert!(!result.contains("var x"));
        assert!(result.contains("<p>Content</p>"));
    }

    #[test]
    fn test_strip_style_tags() {
        let html = r#"<html><body><style>body { color: red; }</style><p>Content</p></body></html>"#;
        let result = strip_unwanted_elements(html);
        assert!(!result.contains("<style>"));
        assert!(result.contains("<p>Content</p>"));
    }

    #[test]
    fn test_truncate_to_char_boundary() {
        let text = "éclair";

        assert_eq!(truncate_to_char_boundary(text, 1), "");
        assert_eq!(truncate_to_char_boundary(text, 2), "é");
        assert_eq!(truncate_to_char_boundary(text, 5), "écla");
    }
}
