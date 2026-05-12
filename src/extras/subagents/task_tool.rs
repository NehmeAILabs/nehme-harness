use std::time::Duration;

use futures::future::join_all;
use rig::completion::ToolDefinition;
use rig::tool::Tool;
use serde::Deserialize;

use crate::agent::tools::{ToolError, check_perm};
use crate::extras::subagents::builder;
use crate::extras::subagents::{clone_subagent_event_tx, with_config};
use crate::extras::truncate::truncate_cjk;
use crate::permission::ask::AskSender;
use crate::permission::checker::PermCheck;

#[cfg(feature = "context")]
use crate::extras::context::{auto_index_output, event, record_event};

const SUBAGENT_TIMEOUT: Duration = Duration::from_secs(300);

const MAX_SUBAGENT_RESPONSE_BYTES: usize = 32 * 1024;

const EXTERNALIZE_THRESHOLD: usize = 100 * 1024;

#[derive(Deserialize)]
pub struct TaskArgs {
    /// One or more exploration prompts. When multiple are provided,
    /// they are explored in parallel subagents and results are combined.
    pub prompts: Vec<String>,
}

pub struct TaskTool {
    permission: Option<PermCheck>,
    ask_tx: Option<AskSender>,
    session_id: String,
}

impl TaskTool {
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

impl Tool for TaskTool {
    const NAME: &'static str = "task";
    type Error = ToolError;
    type Args = TaskArgs;
    type Output = String;

    async fn definition(&self, _p: String) -> ToolDefinition {
        ToolDefinition {
            name: Self::NAME.to_string(),
            description: "Launch a subagent to search and investigate the codebase. \
The subagent has read-only tools (read, grep, find_files, list_dir) and cannot \
spawn further subagents or modify files. \
Use for any cross-file question: where is X used, how does Y work, \
find/list/count all X across the codebase, what calls Z, audit Q. \
Multiple prompts run in parallel. \
Skip only for known-location work: reading one identified file, \
editing in a known location, grepping for a literal you will act on immediately. \
When starting fresh, your prompt should contain a detailed task description \
and specify exactly what information the subagent should return in its final message."
                .to_string(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "prompts": {
                        "type": "array",
                        "items": { "type": "string" },
                        "description": "Investigation prompt for the subagent. Use one for a focused question, or multiple to run independent investigations in parallel. Examples: 'List all tests in this project', 'Where is config loaded?', 'How does the agent loop work?'"
                    }
                },
                "required": ["prompts"]
            }),
        }
    }

    async fn call(&self, args: TaskArgs) -> Result<String, ToolError> {
        if args.prompts.is_empty() {
            return Err(ToolError::Msg("task: prompts must not be empty".into()));
        }

        check_perm(
            &self.permission,
            &self.ask_tx,
            Self::NAME,
            &args.prompts.join(" | "),
        )
        .await?;

        let (client, model_name, max_turns, config, session_id) = with_config(|cfg| {
            (
                cfg.client.clone(),
                cfg.model_name.clone(),
                cfg.max_turns,
                cfg.config.clone(),
                cfg.session_id.clone(),
            )
        });

        let subagent_event_tx = clone_subagent_event_tx();

        let mut abort_handles: Vec<tokio::task::AbortHandle> = Vec::new();
        let mut handles = Vec::with_capacity(args.prompts.len());
        for (i, prompt_text) in args.prompts.iter().enumerate() {
            let prompt_text = prompt_text.clone();
            let model = client.completion_model(model_name.clone());
            let event_tx = subagent_event_tx.clone();
            let config = config.clone();
            let session_id = session_id.clone();
            let join_handle = tokio::spawn(async move {
                let work = async {
                    let agent =
                        builder::build_explore_agent(model, max_turns, &config, session_id.clone())
                            .await;
                    agent
                        .run_subagent(&prompt_text, max_turns, event_tx.as_ref())
                        .await
                };
                match tokio::time::timeout(SUBAGENT_TIMEOUT, work).await {
                    Ok(Ok(response)) => (i, prompt_text, Ok(response)),
                    Ok(Err(e)) => (i, prompt_text, Err(format!("[error: {}]", e))),
                    Err(_elapsed) => (
                        i,
                        prompt_text,
                        Err("[timeout: subagent exceeded 300s]".to_string()),
                    ),
                }
            });
            abort_handles.push(join_handle.abort_handle());
            handles.push(join_handle);
        }

        // Abort guard — if this future is dropped, all subagents are cancelled.
        // Created after all spawns complete: the window between first spawn and
        // guard creation is negligible in practice (no .await in between).
        let _guard = SubagentGuard {
            handles: abort_handles,
        };

        let results = join_all(handles).await;

        let mut outputs: Vec<(usize, String, String)> = Vec::new();
        for r in results {
            match r {
                Ok((i, prompt_text, Ok(response))) => {
                    #[cfg(feature = "context")]
                    if response.len() > EXTERNALIZE_THRESHOLD {
                        let truncated: String = response.chars().take(8192).collect();
                        let pointer = format!(
                            "\n\n[Subagent output externalized: {} bytes. Use ctx_search to retrieve full results.]",
                            response.len()
                        );
                        let summary = format!("{}{}", truncated, pointer);
                        record_event(
                            &self.session_id,
                            event::TOOL_RESULT,
                            event::PRIORITY_LOW,
                            "task",
                            &prompt_text,
                            &format!("{{\"bytes\":{},\"externalized\":true}}", response.len()),
                        );
                        auto_index_output(
                            &self.session_id,
                            "task",
                            &prompt_text,
                            &response,
                            &summary,
                        );
                        outputs.push((i, prompt_text, summary));
                    } else {
                        let truncated = truncate_cjk(
                            &response,
                            MAX_SUBAGENT_RESPONSE_BYTES,
                            &format!(
                                "\n…[subagent response truncated at {}B]",
                                MAX_SUBAGENT_RESPONSE_BYTES
                            ),
                        );
                        #[cfg(feature = "context")]
                        record_event(
                            &self.session_id,
                            event::TOOL_RESULT,
                            event::PRIORITY_LOW,
                            "task",
                            &prompt_text,
                            &format!("{{\"bytes\":{}}}", response.len()),
                        );
                        outputs.push((i, prompt_text, truncated));
                    }
                    #[cfg(not(feature = "context"))]
                    {
                        let truncated = truncate_cjk(
                            &response,
                            MAX_SUBAGENT_RESPONSE_BYTES,
                            &format!(
                                "\n…[subagent response truncated at {}B]",
                                MAX_SUBAGENT_RESPONSE_BYTES
                            ),
                        );
                        outputs.push((i, prompt_text, truncated));
                    }
                }
                Ok((i, prompt_text, Err(e))) => {
                    #[cfg(feature = "context")]
                    record_event(
                        &self.session_id,
                        event::COMMAND_FAIL,
                        event::PRIORITY_HIGH,
                        "task",
                        &prompt_text,
                        &format!(
                            "{{\"error\":\"{}\"}}",
                            e.chars().take(200).collect::<String>()
                        ),
                    );
                    outputs.push((i, prompt_text, e));
                }
                Err(e) => {
                    outputs.push((
                        outputs.len(),
                        "(unknown)".to_string(),
                        format!("[task panicked: {}]", e),
                    ));
                }
            }
        }

        outputs.sort_by_key(|(i, _, _)| *i);

        Ok(combine_results(&outputs))
    }
}

/// Combine per-task outputs into a single Markdown string, ordered by the
/// original prompt index. Multiple tasks get `## Task N:` headings; a single
/// task is emitted as-is.
pub(crate) fn combine_results(outputs: &[(usize, String, String)]) -> String {
    let mut combined = String::new();
    for (idx, (_, prompt_text, response)) in outputs.iter().enumerate() {
        if outputs.len() > 1 {
            if idx > 0 {
                combined.push('\n');
            }
            let label = prompt_text.chars().take(60).collect::<String>();
            combined.push_str(&format!("## Task {}: {}\n\n", idx + 1, label));
        }
        combined.push_str(response);
        if !combined.ends_with('\n') {
            combined.push('\n');
        }
    }
    combined
}

/// Aborts all registered subagent tasks on drop. If the parent agent cancels
/// the `task` tool call (e.g. the session ends or the loop exits), in-flight
/// subagents are stopped immediately rather than leaking.
struct SubagentGuard {
    handles: Vec<tokio::task::AbortHandle>,
}

impl Drop for SubagentGuard {
    fn drop(&mut self) {
        for h in &self.handles {
            h.abort();
        }
    }
}
