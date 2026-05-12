use std::sync::Mutex;
use std::sync::atomic::{AtomicU32, Ordering};

use rig::completion::ToolDefinition;
use rig::tool::Tool;
use serde::Deserialize;
use serde_json::json;

use crate::agent::tools::ToolError;
use crate::agent::tools::check_perm;
use crate::permission::ask::AskSender;
use crate::permission::checker::PermCheck;

use super::store::ContextStore;

static STORE: Mutex<Option<ContextStore>> = Mutex::new(None);
static CTX_SEARCH_CALLS: AtomicU32 = AtomicU32::new(0);

const THROTTLE_REDUCE: u32 = 3;
const THROTTLE_BLOCK: u32 = 8;

pub fn init_store(data_dir: &std::path::Path) -> anyhow::Result<()> {
    let store = ContextStore::open(data_dir)?;
    let mut guard = STORE.lock().unwrap_or_else(|e| e.into_inner());
    *guard = Some(store);
    Ok(())
}

fn with_store<F, R>(f: F) -> Result<R, ToolError>
where
    F: FnOnce(&ContextStore) -> Result<R, ToolError>,
{
    let guard = STORE.lock().unwrap_or_else(|e| e.into_inner());
    match guard.as_ref() {
        Some(store) => f(store),
        None => Err(ToolError::Msg("Context store not initialized.".into())),
    }
}

pub fn auto_index_output(
    session_id: &str,
    tool: &str,
    source: &str,
    raw_output: &str,
    compressed_output: &str,
) -> Option<i64> {
    let bytes_original = raw_output.len() as i64;
    let bytes_saved = bytes_original.saturating_sub(compressed_output.len() as i64);
    if bytes_saved <= 0 {
        return None;
    }
    let guard = STORE.lock().unwrap_or_else(|e| e.into_inner());
    let store = guard.as_ref()?;
    store
        .record_tool_output(
            session_id,
            tool,
            source,
            raw_output,
            compressed_output,
            bytes_original,
            bytes_saved,
        )
        .ok()
}

pub fn record_event(
    session_id: &str,
    category: &str,
    priority: i32,
    source: &str,
    content: &str,
    meta: &str,
) {
    let guard = STORE.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(store) = guard.as_ref() {
        let _ = store.record_event(session_id, category, priority, source, content, meta);
    }
}

// ─── ctx_search ──────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct CtxSearchArgs {
    pub query: String,
    #[serde(default)]
    pub category: Option<String>,
    #[serde(default = "default_limit")]
    pub limit: i32,
}

fn default_limit() -> i32 {
    5
}

pub struct CtxSearchTool {
    pub permission: Option<PermCheck>,
    pub ask_tx: Option<AskSender>,
    pub session_id: String,
}

impl CtxSearchTool {
    pub fn new(
        permission: Option<PermCheck>,
        ask_tx: Option<AskSender>,
        session_id: String,
    ) -> Self {
        Self {
            permission,
            ask_tx,
            session_id,
        }
    }
}

impl Tool for CtxSearchTool {
    const NAME: &'static str = "ctx_search";
    type Error = ToolError;
    type Args = CtxSearchArgs;
    type Output = String;

    async fn definition(&self, _prompt: String) -> ToolDefinition {
        ToolDefinition {
            name: "ctx_search".into(),
            description: "Search previously indexed content from this session. Large tool outputs (build logs, test results, API responses) are automatically indexed. Use this to retrieve specific information without re-reading it into context. Supports BM25 full-text search.".into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "query": {
                        "type": "string",
                        "description": "Search query. Supports BM25 full-text search with Porter stemming."
                    },
                    "limit": {
                        "type": "integer",
                        "description": "Max results to return (default: 5)"
                    }
                },
                "required": ["query"]
            }),
        }
    }

    async fn call(&self, args: CtxSearchArgs) -> Result<String, ToolError> {
        check_perm(&self.permission, &self.ask_tx, "ctx_search", &args.query).await?;

        let call_count = CTX_SEARCH_CALLS.fetch_add(1, Ordering::Relaxed) + 1;

        if call_count > THROTTLE_BLOCK {
            return Ok(
                "ctx_search is throttled for this session. The context store is saturated — \
                 rely on your existing context instead of re-querying indexed content."
                    .into(),
            );
        }

        let preview_chars = if call_count > THROTTLE_REDUCE {
            200
        } else {
            500
        };

        with_store(|store| {
            let mut hits = store
                .search_events(&self.session_id, &args.query, args.limit)
                .map_err(|e| ToolError::Msg(format!("Search failed: {e}")))?;
            let mut tool_hits = store
                .search_tool_output(&self.session_id, &args.query, args.limit)
                .map_err(|e| ToolError::Msg(format!("Search failed: {e}")))?;

            if let Some(category) = args.category.as_deref() {
                hits.retain(|hit| hit.category == category);
                tool_hits.retain(|hit| hit.category == category);
            }
            hits.extend(tool_hits);
            hits.sort_by(|a, b| {
                b.rank
                    .partial_cmp(&a.rank)
                    .unwrap_or(std::cmp::Ordering::Equal)
            });
            hits.truncate(args.limit.max(0) as usize);

            if hits.is_empty() {
                return Ok("No results found. Try broader terms or different keywords.".into());
            }

            let mut out = format!("Found {} result(s):\n\n", hits.len());
            for hit in &hits {
                let preview: String = hit.content.chars().take(preview_chars).collect();
                let truncated = if hit.content.len() > preview_chars {
                    "..."
                } else {
                    ""
                };
                out.push_str(&format!(
                    "[{}] {} (source: {})\n{}\n{}\n\n",
                    hit.id, hit.category, hit.source, preview, truncated
                ));
            }
            Ok(out)
        })
    }
}

// ─── ctx_execute: Think in Code ──────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct CtxExecuteArgs {
    pub language: String,
    pub code: String,
    #[serde(default)]
    pub intent: String,
}

pub struct CtxExecuteTool {
    pub permission: Option<PermCheck>,
    pub ask_tx: Option<AskSender>,
    pub session_id: String,
}

impl CtxExecuteTool {
    pub fn new(
        permission: Option<PermCheck>,
        ask_tx: Option<AskSender>,
        session_id: String,
    ) -> Self {
        Self {
            permission,
            ask_tx,
            session_id,
        }
    }
}

impl Tool for CtxExecuteTool {
    const NAME: &'static str = "ctx_execute";
    type Error = ToolError;
    type Args = CtxExecuteArgs;
    type Output = String;

    async fn definition(&self, _prompt: String) -> ToolDefinition {
        ToolDefinition {
            name: "ctx_execute".into(),
            description: "Execute code in a subprocess and return only the result. Use this instead of reading many files into context: write a short script that processes data and prints only what you need. Output is automatically compressed and indexed for ctx_search. Supports: python3, node, bash, ruby, go.".into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "language": {
                        "type": "string",
                        "description": "Language runtime: python3, node, bash, ruby, go",
                        "enum": ["python3", "node", "bash", "ruby", "go"]
                    },
                    "code": {
                        "type": "string",
                        "description": "The code to execute."
                    },
                    "intent": {
                        "type": "string",
                        "description": "What you're looking for in the output. If output is large, only sections matching the intent are returned. Omit for full compressed output."
                    }
                },
                "required": ["language", "code"]
            }),
        }
    }

    async fn call(&self, args: CtxExecuteArgs) -> Result<String, ToolError> {
        check_perm(
            &self.permission,
            &self.ask_tx,
            "ctx_execute",
            &args.language,
        )
        .await?;

        let (cmd, filename) = match args.language.as_str() {
            "python3" | "python" => ("python3", "script.py"),
            "node" | "javascript" | "js" => ("node", "script.js"),
            "bash" | "sh" | "shell" => ("bash", "script.sh"),
            "ruby" | "rb" => ("ruby", "script.rb"),
            "go" => ("go", "script.go"),
            _ => {
                return Err(ToolError::Msg(format!(
                    "Unsupported language: {}. Supported: python3, node, bash, ruby, go",
                    args.language
                )));
            }
        };

        let tmp_dir = std::env::temp_dir().join(format!("ctx_execute_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&tmp_dir)
            .map_err(|e| ToolError::Msg(format!("Failed to create temp dir: {e}")))?;

        let script_path = tmp_dir.join(filename);
        std::fs::write(&script_path, &args.code)
            .map_err(|e| ToolError::Msg(format!("Failed to write script: {e}")))?;

        let output = tokio::process::Command::new(cmd)
            .arg(&script_path)
            .current_dir(&tmp_dir)
            .output()
            .await
            .map_err(|e| ToolError::Msg(format!("Failed to execute: {e}")))?;

        let _ = std::fs::remove_dir_all(&tmp_dir);

        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        let exit_code = output.status.code().unwrap_or(-1);

        let combined = if exit_code != 0 {
            format!(
                "EXIT CODE: {}\n\nSTDOUT:\n{}\n\nSTDERR:\n{}",
                exit_code, stdout, stderr
            )
        } else if stderr.is_empty() {
            stdout.into_owned()
        } else {
            format!("{}\n\n[stderr]\n{}", stdout, stderr)
        };

        let bytes_original = combined.len() as i64;

        if !args.intent.is_empty() && combined.len() > 5000 {
            let filtered = intent_filter(&combined, &args.intent);
            let _bytes_saved = bytes_original.saturating_sub(filtered.len() as i64);

            auto_index_output(
                &self.session_id,
                "ctx_execute",
                &format!("{cmd} {filename}"),
                &combined,
                &filtered,
            );

            Ok(format!(
                "{}\n\n[{} bytes original, {} bytes returned — full output indexed, use ctx_search to retrieve]",
                filtered,
                bytes_original,
                filtered.len()
            ))
        } else if combined.len() > 2048 {
            let compressed_display = crate::filter::compress::compress_command_output(
                &format!("{cmd} {filename}"),
                &combined,
                "",
                exit_code,
            );
            let _bytes_saved = bytes_original.saturating_sub(compressed_display.len() as i64);

            auto_index_output(
                &self.session_id,
                "ctx_execute",
                &format!("{cmd} {filename}"),
                &combined,
                &compressed_display,
            );

            Ok(format!(
                "{}\n\n[{} bytes original, {} bytes returned — full output indexed, use ctx_search to retrieve]",
                compressed_display,
                bytes_original,
                compressed_display.len()
            ))
        } else {
            Ok(combined)
        }
    }
}

pub fn intent_filter(output: &str, intent: &str) -> String {
    let mut relevant = Vec::new();
    let intent_lower = intent.to_lowercase();
    let keywords: Vec<&str> = intent_lower.split_whitespace().collect();

    for line in output.lines() {
        let line_lower = line.to_lowercase();
        if keywords.iter().any(|kw| line_lower.contains(kw)) {
            relevant.push(line.to_string());
        }
    }

    if relevant.is_empty() {
        output.chars().take(3000).collect()
    } else {
        relevant.join("\n")
    }
}

// ─── ctx_stats ───────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct CtxStatsArgs {}

pub struct CtxStatsTool {
    pub permission: Option<PermCheck>,
    pub ask_tx: Option<AskSender>,
    pub session_id: String,
}

impl CtxStatsTool {
    pub fn new(
        permission: Option<PermCheck>,
        ask_tx: Option<AskSender>,
        session_id: String,
    ) -> Self {
        Self {
            permission,
            ask_tx,
            session_id,
        }
    }
}

impl Tool for CtxStatsTool {
    const NAME: &'static str = "ctx_stats";
    type Error = ToolError;
    type Args = CtxStatsArgs;
    type Output = String;

    async fn definition(&self, _prompt: String) -> ToolDefinition {
        ToolDefinition {
            name: "ctx_stats".into(),
            description: "Show context savings statistics for this session: how many bytes have been saved by externalizing tool output.".into(),
            parameters: json!({
                "type": "object",
                "properties": {}
            }),
        }
    }

    async fn call(&self, _args: CtxStatsArgs) -> Result<String, ToolError> {
        check_perm(&self.permission, &self.ask_tx, "ctx_stats", "stats").await?;

        with_store(|store| {
            let (bytes_orig, bytes_saved) = store
                .context_savings(&self.session_id)
                .map_err(|e| ToolError::Msg(format!("Failed to query stats: {e}")))?;

            let pct = if bytes_orig > 0 {
                bytes_saved as f64 / bytes_orig as f64 * 100.0
            } else {
                0.0
            };

            Ok(format!(
                "Context savings this session:\n  Original bytes: {}\n  Bytes saved: {}\n  Reduction: {:.1}%\n  Use ctx_search to query indexed content",
                bytes_orig, bytes_saved, pct
            ))
        })
    }
}

// ── ctx_retrieve: hash/keyword-based retrieval of externalized outputs ──

#[derive(Deserialize)]
pub struct CtxRetrieveArgs {
    /// The ID returned when the output was externalized (e.g. "42" from "id:42"),
    /// or a keyword to search for the most recent matching externalized output.
    query: String,
}

pub struct CtxRetrieveTool {
    permission: Option<PermCheck>,
    ask_tx: Option<AskSender>,
    session_id: String,
}

impl CtxRetrieveTool {
    pub fn new(
        permission: Option<PermCheck>,
        ask_tx: Option<AskSender>,
        session_id: String,
    ) -> Self {
        Self {
            permission,
            ask_tx,
            session_id,
        }
    }
}

impl Tool for CtxRetrieveTool {
    const NAME: &'static str = "ctx_retrieve";
    type Error = ToolError;
    type Args = CtxRetrieveArgs;
    type Output = String;

    async fn definition(&self, _prompt: String) -> ToolDefinition {
        ToolDefinition {
            name: "ctx_retrieve".into(),
            description: "Retrieve the full raw output of a previously externalized tool result by its ID or keyword. Use when you see a message like '[Output externalized: ... id:N]' and need the complete content.".into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "query": {
                        "type": "string",
                        "description": "The externalized output ID (number) or a keyword to find the most recent matching externalized output."
                    }
                },
                "required": ["query"]
            }),
        }
    }

    async fn call(&self, args: CtxRetrieveArgs) -> Result<String, ToolError> {
        check_perm(&self.permission, &self.ask_tx, "ctx_retrieve", &args.query).await?;

        with_store(|store| {
            // Try parsing as numeric ID first
            if let Ok(id) = args.query.trim().parse::<i64>() {
                if let Some(result) = store
                    .retrieve_tool_output(id)
                    .map_err(|e| ToolError::Msg(format!("Retrieve failed: {e}")))?
                {
                    let preview: String = result.raw_output.chars().take(8000).collect();
                    let truncated = if result.raw_output.len() > 8000 {
                        format!("\n\n[Retrieved {} bytes, showing first 8000]", result.bytes)
                    } else {
                        String::new()
                    };
                    return Ok(format!(
                        "[Retrieved: {} from `{}`]\n{}{}",
                        result.tool, result.source, preview, truncated
                    ));
                }
            }

            // Fall back to keyword search
            if let Some(result) = store
                .retrieve_latest_by_keyword(&self.session_id, &args.query)
                .map_err(|e| ToolError::Msg(format!("Retrieve failed: {e}")))?
            {
                let preview: String = result.raw_output.chars().take(8000).collect();
                let truncated = if result.raw_output.len() > 8000 {
                    format!("\n\n[Retrieved {} bytes, showing first 8000]", result.bytes)
                } else {
                    String::new()
                };
                return Ok(format!(
                    "[Retrieved: {} from `{}`]\n{}{}",
                    result.tool, result.source, preview, truncated
                ));
            }

            Err(ToolError::Msg(format!(
                "No externalized output found for '{}'. Try ctx_search for keyword-based search.",
                args.query
            )))
        })
    }
}
