use rig::completion::ToolDefinition;
use rig::tool::Tool;
use tokio::time::{Duration, timeout};

use crate::agent::tools::{AskSender, BashArgs, PermCheck, ToolError, check_perm};
use crate::extras::truncate::head_lines;
use crate::filter::compress::compress_command_output_with_intent;
use crate::sandbox::Sandbox;

#[cfg(feature = "context")]
use crate::extras::context::{auto_index_output, event, record_event};

const EXTERNALIZE_THRESHOLD: usize = 100 * 1024;

pub(crate) fn split_bash_commands(input: &str) -> Vec<String> {
    let mut result = Vec::new();
    let mut current = String::new();
    let mut in_single_quote = false;
    let mut in_double_quote = false;
    let mut chars = input.chars().peekable();

    while let Some(ch) = chars.next() {
        if ch == '\\' {
            current.push(ch);
            if let Some(next) = chars.next() {
                current.push(next);
            }
        } else if ch == '\'' && !in_double_quote {
            in_single_quote = !in_single_quote;
            current.push(ch);
        } else if ch == '"' && !in_single_quote {
            in_double_quote = !in_double_quote;
            current.push(ch);
        } else if ch == ';' && !in_single_quote && !in_double_quote {
            let trimmed = current.trim().to_string();
            if !trimmed.is_empty() {
                result.push(trimmed);
            }
            current = String::new();
        } else if ch == '&' && !in_single_quote && !in_double_quote {
            if chars.peek() == Some(&'&') {
                chars.next();
                let trimmed = current.trim().to_string();
                if !trimmed.is_empty() {
                    result.push(trimmed);
                }
                current = String::new();
            } else {
                current.push(ch);
            }
        } else if ch == '|' && !in_single_quote && !in_double_quote {
            if chars.peek() == Some(&'|') {
                chars.next();
                let trimmed = current.trim().to_string();
                if !trimmed.is_empty() {
                    result.push(trimmed);
                }
                current = String::new();
            } else {
                current.push(ch);
            }
        } else if ch == '>' && !in_single_quote && !in_double_quote {
            if chars.peek() == Some(&'>') {
                chars.next();
                let trimmed = current.trim().to_string();
                if !trimmed.is_empty() {
                    result.push(trimmed);
                }
                current = String::new();
            } else {
                current.push(ch);
            }
        } else {
            current.push(ch);
        }
    }

    let trimmed = current.trim().to_string();
    if !trimmed.is_empty() {
        result.push(trimmed);
    }

    result
}

pub struct BashTool {
    pub permission: Option<PermCheck>,
    pub ask_tx: Option<AskSender>,
    pub sandbox: Sandbox,
    pub max_output_lines: Option<u64>,
    pub session_id: String,
}

impl BashTool {
    pub fn new(
        permission: Option<PermCheck>,
        ask_tx: Option<AskSender>,
        sandbox: Sandbox,
        max_output_lines: Option<u64>,
        session_id: String,
    ) -> Self {
        BashTool {
            permission,
            ask_tx,
            sandbox,
            max_output_lines,
            session_id,
        }
    }
}

impl Tool for BashTool {
    const NAME: &'static str = "bash";

    type Error = ToolError;
    type Args = BashArgs;
    type Output = String;

    async fn definition(&self, _prompt: String) -> ToolDefinition {
        ToolDefinition {
            name: "bash".to_string(),
            description: "Execute a bash command in the current working directory. Returns stdout and stderr.".to_string(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "command": { "type": "string", "description": "Bash command to execute" },
                    "timeout": { "type": "integer", "description": "Timeout in milliseconds (optional)" },
                    "intent": { "type": "string", "description": "What you're looking for in the output. If output is large, only lines matching the intent are returned (full output is indexed for ctx_search). Omit for full output." }
                },
                "required": ["command"]
            }),
        }
    }

    async fn call(&self, args: BashArgs) -> Result<String, ToolError> {
        let mut coaching: Option<String> = None;
        for cmd in split_bash_commands(&args.command) {
            if let Some(msg) = check_perm(&self.permission, &self.ask_tx, "bash", &cmd).await? {
                coaching = Some(msg);
            }
        }

        let output = if let Some(secs) = args.timeout {
            timeout(
                Duration::from_millis(secs),
                self.sandbox.wrap_command(&args.command).output(),
            )
            .await
            .map_err(|_| ToolError::Msg("Command timed out".to_string()))?
        } else {
            self.sandbox.wrap_command(&args.command).output().await
        }?;

        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        let exit_code = output.status.code().unwrap_or(-1);

        let raw_combined = format!("STDOUT:\n{stdout}\nSTDERR:\n{stderr}");
        let compressed = compress_command_output_with_intent(
            &args.command,
            &stdout,
            &stderr,
            exit_code,
            args.intent.as_deref(),
        );

        // Cache-aware gating: if compression saves < 15%, skip it to preserve
        // provider prefix cache. The cache-bust penalty of rewriting the
        // tool result outweighs the small token savings.
        let result = if raw_combined.len() > 500
            && compressed.len() as f64 / raw_combined.len() as f64 > 0.85
        {
            raw_combined.clone()
        } else {
            compressed
        };

        #[cfg(feature = "context")]
        {
            if exit_code != 0 {
                record_event(
                    &self.session_id,
                    event::COMMAND_FAIL,
                    event::PRIORITY_HIGH,
                    "bash",
                    &args.command,
                    &format!(
                        "{{\"exit_code\":{},\"bytes\":{}}}",
                        exit_code,
                        raw_combined.len()
                    ),
                );
                auto_index_output(
                    &self.session_id,
                    "bash_fail",
                    &args.command,
                    &raw_combined,
                    &result,
                );
            } else {
                record_event(
                    &self.session_id,
                    event::TOOL_RESULT,
                    event::PRIORITY_LOW,
                    "bash",
                    &args.command,
                    &format!("{{\"exit_code\":0,\"bytes\":{}}}", raw_combined.len()),
                );
            }

            if raw_combined.len() > EXTERNALIZE_THRESHOLD {
                let id = auto_index_output(
                    &self.session_id,
                    "bash",
                    &args.command,
                    &raw_combined,
                    &result,
                );
                let summary = crate::agent::read_lifecycle::summarize_dropped_content(
                    &raw_combined,
                    &crate::filter::code_filter::Language::Unknown,
                );
                let id_str = id.map(|i| format!(" id:{}", i)).unwrap_or_default();
                let pointer = format!(
                    "[Output externalized:{} {} from `{}`. Use ctx_retrieve(\"{}\") to get full content. Dropped: {}]",
                    id_str,
                    format_bytes(raw_combined.len()),
                    args.command.chars().take(80).collect::<String>(),
                    id.map(|i| i.to_string()).unwrap_or_else(|| args
                        .command
                        .chars()
                        .take(40)
                        .collect()),
                    summary,
                );
                let capped_result: String = result.chars().take(2048).collect();
                return Ok(format!("{}\n\n{}", capped_result, pointer));
            }

            let bytes_original = raw_combined.len() as i64;
            let bytes_saved = bytes_original.saturating_sub(result.len() as i64);
            if bytes_saved > 2048 {
                auto_index_output(
                    &self.session_id,
                    "bash",
                    &args.command,
                    &raw_combined,
                    &result,
                );
            }
        }

        let result = if let Some(cap) = self.max_output_lines {
            let cap = cap as usize;
            let (head, total) = head_lines(&result, cap);
            if total > cap {
                format!(
                    "{}\n\n[truncated after {} lines — {} more lines elided; re-run with a narrower invocation or pipe through `tail`/`grep` to see trailing output]",
                    head,
                    cap,
                    total - cap,
                )
            } else {
                result
            }
        } else {
            result
        };

        let result = match coaching {
            Some(msg) => format!("{}\n\n{}", msg, result),
            None => result,
        };
        Ok(result)
    }
}

fn format_bytes(n: usize) -> String {
    if n >= 1024 * 1024 {
        format!("{:.1}MB", n as f64 / (1024.0 * 1024.0))
    } else if n >= 1024 {
        format!("{:.1}KB", n as f64 / 1024.0)
    } else {
        format!("{}B", n)
    }
}
