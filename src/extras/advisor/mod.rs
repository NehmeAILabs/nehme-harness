use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};

use rig::completion::ToolDefinition;
use rig::streaming::StreamingChat;
use rig::tool::Tool;
use serde::Deserialize;
use tokio::sync::oneshot;

use crate::agent::tools::ToolError;
use crate::provider::{AnyClient, AnyModel, OpenAiModel};
use crate::session::{MessageRole, SessionMessage};

const ADVISOR_SYSTEM_PROMPT: &str = "\
You are an expert advisor called by a coding assistant for strategic guidance. \
The assistant is driving a real coding session with file read/write/edit, \
bash, grep, and other tools at its disposal.

Below is the full conversation so far, followed by the assistant's question. \
Your role:
- Review the conversation to understand what has happened
- Provide a clear plan, approach, or course correction
- Focus on architecture, design decisions, edge cases, and risk
- Keep guidance concise: aim for 150-300 words unless the question demands more
- Do NOT produce user-facing output or call any tools yourself

The assistant will continue the task after receiving your advice. \
Give it the strategic direction it needs to proceed correctly.";

pub struct HandoffRequest {
    pub question: String,
    pub reply: oneshot::Sender<String>,
}

pub type HandoffSender = tokio::sync::mpsc::Sender<HandoffRequest>;
pub type HandoffReceiver = tokio::sync::mpsc::Receiver<HandoffRequest>;

#[derive(Clone)]
pub struct AdvisorToolConfig {
    pub client: Option<AnyClient>,
    pub advisor_model: String,
    pub human_handoff: bool,
    pub max_uses: Option<usize>,
    pub handoff_tx: Option<HandoffSender>,
    pub enabled: bool,
    pub kilobytes_limit: u32,
}

static CONFIG: Mutex<Option<AdvisorToolConfig>> = Mutex::new(None);
static SESSION_MESSAGES: Mutex<Vec<SessionMessage>> = Mutex::new(Vec::new());

pub fn init_config(cfg: AdvisorToolConfig) {
    *CONFIG.lock().unwrap_or_else(|e| e.into_inner()) = Some(cfg);
}

pub fn with_config<F, R>(f: F) -> R
where
    F: FnOnce(&AdvisorToolConfig) -> R,
{
    let guard = CONFIG.lock().unwrap_or_else(|e| e.into_inner());
    let cfg = guard.as_ref().expect("advisor config not initialized");
    f(cfg)
}

pub fn update_client(client: AnyClient) {
    let mut guard = CONFIG.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(ref mut cfg) = *guard {
        cfg.client = Some(client);
    }
}

pub fn set_session_messages(msgs: Vec<SessionMessage>) {
    *SESSION_MESSAGES.lock().unwrap_or_else(|e| e.into_inner()) = msgs;
}

#[derive(Deserialize)]
pub struct AdvisorArgs {
    pub question: String,
}

pub struct AdvisorTool {
    uses: AtomicUsize,
}

impl AdvisorTool {
    pub fn new() -> Self {
        Self {
            uses: AtomicUsize::new(0),
        }
    }
}

impl Tool for AdvisorTool {
    const NAME: &'static str = "advisor";
    type Error = ToolError;
    type Args = AdvisorArgs;
    type Output = String;

    async fn definition(&self, _p: String) -> ToolDefinition {
        let human_handoff = CONFIG
            .lock()
            .ok()
            .and_then(|g| g.as_ref().map(|c| c.human_handoff))
            .unwrap_or(false);

        let desc = if human_handoff {
            "Consult the user for strategic guidance. \
Call this before substantive work, before writing, before committing to an \
interpretation, when stuck, or when considering a change of approach. \
The user sees your full conversation so far. \
Describe your question clearly — include relevant context, what you're \
trying to do, what you've tried, and what you need guidance on."
        } else {
            "Consult an expert advisor model for strategic guidance. \
The advisor receives your full conversation transcript automatically. \
Call this before substantive work, before writing, before committing to an \
interpretation, when stuck, or when considering a change of approach. \
Describe your question clearly — the advisor already sees the full \
conversation, so focus your question on the specific decision you need help with."
        };

        ToolDefinition {
            name: Self::NAME.to_string(),
            description: desc.to_string(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "question": {
                        "type": "string",
                        "description": "Your question for the advisor. The advisor \
            already sees the full conversation transcript. Focus on the specific decision, \
            approach, or problem you need guidance on."
                    }
                },
                "required": ["question"]
            }),
        }
    }

    async fn call(&self, args: AdvisorArgs) -> Result<String, ToolError> {
        if args.question.is_empty() {
            return Err(ToolError::Msg("advisor: question must not be empty".into()));
        }

        let cfg = with_config(|c| c.clone());

        if let Some(max) = cfg.max_uses {
            self.uses
                .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |u| {
                    if u >= max { None } else { Some(u + 1) }
                })
                .map_err(|_| {
                    ToolError::Msg("Advisor call limit reached for this request".into())
                })?;
        } else {
            self.uses.fetch_add(1, Ordering::Relaxed);
        }

        if cfg.human_handoff {
            let Some(ref tx) = cfg.handoff_tx else {
                return Err(ToolError::Msg(
                    "Human handoff unavailable (non-interactive mode)".into(),
                ));
            };
            let (reply_tx, reply_rx) = oneshot::channel();
            tx.send(HandoffRequest {
                question: args.question,
                reply: reply_tx,
            })
            .await
            .map_err(|_| ToolError::Msg("Handoff channel closed".into()))?;

            match reply_rx.await {
                Ok(response) => {
                    if response.is_empty() {
                        Ok("[User provided no response]".to_string())
                    } else {
                        Ok(response)
                    }
                }
                Err(_) => Err(ToolError::Msg("Handoff cancelled".into())),
            }
        } else {
            let Some(ref client) = cfg.client else {
                return Err(ToolError::Msg("Advisor model not configured".into()));
            };

            let model = client.completion_model(cfg.advisor_model.clone());
            let messages = SESSION_MESSAGES
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .clone();
            run_advisor_completion(model, &args.question, &messages)
                .await
                .map_err(|e| ToolError::Msg(format!("Advisor call failed: {e}")))
        }
    }
}

async fn run_advisor_completion(
    model: AnyModel,
    question: &str,
    messages: &[SessionMessage],
) -> anyhow::Result<String> {
    let kilobytes_limit = with_config(|c| c.kilobytes_limit);
    let conversation = format_conversation(messages, kilobytes_limit);
    let prompt = format!(
        "## Conversation\n\n{}\n\n## Assistant's question\n\n{}",
        conversation, question
    );

    match model {
        AnyModel::OpenRouter(m, _) => advisor_call(m, prompt).await,
        AnyModel::OpenAI(m) => match m {
            OpenAiModel::Responses(m) => advisor_call(m, prompt).await,
            OpenAiModel::Completions(m) => advisor_call(m, prompt).await,
        },
        AnyModel::Anthropic(m) => advisor_call(m, prompt).await,
        AnyModel::Gemini(m) => advisor_call(m, prompt).await,
        AnyModel::Ollama(m) => advisor_call(m, prompt).await,
    }
}

fn format_conversation(msgs: &[SessionMessage], kilobytes_limit: u32) -> String {
    let per_side = (kilobytes_limit as usize * 1024) / 2;

    fn format_line(msg: &SessionMessage) -> String {
        let role = match msg.role {
            MessageRole::User => "User",
            MessageRole::Assistant => "Assistant",
            MessageRole::System => "System",
        };
        format!("[{role}]: {}", msg.content)
    }

    // Collect head (oldest messages)
    let mut head_end = 0usize;
    let mut head_chars = 0usize;
    for (i, msg) in msgs.iter().enumerate() {
        let line = format_line(msg);
        let needed = if head_chars > 0 {
            line.len() + 2
        } else {
            line.len()
        };
        if head_chars + needed > per_side {
            break;
        }
        head_chars += needed;
        head_end = i + 1;
    }

    // Collect tail (newest messages)
    let mut tail_start = msgs.len();
    let mut tail_chars = 0usize;
    for (i, msg) in msgs.iter().enumerate().rev() {
        let line = format_line(msg);
        let needed = if tail_chars > 0 {
            line.len() + 2
        } else {
            line.len()
        };
        if tail_chars + needed > per_side {
            break;
        }
        tail_chars += needed;
        tail_start = i;
    }

    if msgs.is_empty() {
        return String::new();
    }

    let mut result = String::new();

    // Head
    for (i, msg) in msgs.iter().enumerate().take(head_end) {
        if i > 0 {
            result.push_str("\n\n");
        }
        result.push_str(&format_line(msg));
    }

    // Omission marker if there is a gap
    if head_end < tail_start {
        result.push_str("\n\n[... conversation omitted ...]\n\n");
    } else if head_end > 0 && head_end < msgs.len() {
        result.push_str("\n\n");
    }

    // Tail (only messages not already in head)
    let tail_begin = head_end.max(tail_start);
    for (i, msg) in msgs.iter().enumerate().skip(tail_begin) {
        if i > tail_begin {
            result.push_str("\n\n");
        }
        result.push_str(&format_line(msg));
    }

    result
}

async fn advisor_call<M>(model: M, prompt: String) -> anyhow::Result<String>
where
    M: rig::completion::CompletionModel + 'static,
    M::StreamingResponse: Send + Sync + Unpin + Clone + 'static,
{
    let agent = rig::agent::AgentBuilder::new(model)
        .preamble(ADVISOR_SYSTEM_PROMPT)
        .build();

    use futures::StreamExt;
    let history: Vec<rig::completion::Message> = vec![];
    let mut stream = agent.stream_chat(prompt, history).multi_turn(1).await;

    let mut response = String::new();
    while let Some(item) = stream.next().await {
        match item {
            Ok(rig::agent::MultiTurnStreamItem::FinalResponse(res)) => {
                response = res.response().to_string();
                break;
            }
            Err(e) => return Err(anyhow::anyhow!("Advisor call failed: {e}")),
            _ => {}
        }
    }

    if response.is_empty() {
        Ok("[Advisor returned empty response]".to_string())
    } else {
        Ok(response)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session::MessageRole;

    fn msg(role: MessageRole, content: &str) -> SessionMessage {
        SessionMessage {
            role,
            content: content.into(),
            estimated_tokens: 0,
        }
    }

    #[test]
    fn format_conversation_empty() {
        assert_eq!(format_conversation(&[], 1), "");
    }

    #[test]
    fn format_conversation_single_message() {
        let msgs = [msg(MessageRole::User, "hello")];
        assert_eq!(format_conversation(&msgs, 1024), "[User]: hello");
    }

    #[test]
    fn format_conversation_two_messages() {
        let msgs = [
            msg(MessageRole::User, "hello"),
            msg(MessageRole::Assistant, "hi there"),
        ];
        assert_eq!(
            format_conversation(&msgs, 1024),
            "[User]: hello\n\n[Assistant]: hi there"
        );
    }

    #[test]
    fn format_conversation_nothing_fits_returns_omission_only() {
        let msgs = [
            msg(MessageRole::User, "aaaaaaaaaa"),
            msg(MessageRole::Assistant, "bbbbbbbbbb"),
            msg(MessageRole::User, "cccccccccc"),
            msg(MessageRole::Assistant, "dddddddddd"),
        ];
        let result = format_conversation(&msgs, 0);
        assert!(result.contains("[... conversation omitted ...]"));
        // Nothing fits in head or tail, so result is just the marker (plus \n\n wrapping)
        assert!(!result.contains("[User]:"));
        assert!(!result.contains("[Assistant]:"));
    }

    #[test]
    fn format_conversation_head_with_tail_gap() {
        // 7 messages, limit=0: head empty, tail empty, just omission marker
        let short = "aa";
        let msgs = [
            msg(MessageRole::User, short),
            msg(MessageRole::Assistant, short),
            msg(
                MessageRole::User,
                "a much longer message that takes more space",
            ),
            msg(
                MessageRole::Assistant,
                "another fairly long message here too",
            ),
            msg(
                MessageRole::User,
                "a third long message that pushes the limit",
            ),
            msg(MessageRole::Assistant, short),
            msg(MessageRole::User, short),
        ];
        let result = format_conversation(&msgs, 0);
        assert!(result.contains("[... conversation omitted ...]"));
        // With limit=0 nothing fits; head and tail are both empty
    }

    #[test]
    fn format_conversation_small_limit_head_tail_with_gap() {
        // per_side=5 bytes: first short msg (~10B) fails head; last short msg (~10B) fails tail
        // but with just 2 short messages close together, one might slip into tail
        let msgs = [
            msg(MessageRole::User, "x"),
            msg(MessageRole::Assistant, "y"),
            msg(MessageRole::User, "z"),
        ];
        let result = format_conversation(&msgs, 0);
        // per_side=0: nothing fits; expect omission marker only
        assert!(result.contains("[... conversation omitted ...]"));
    }

    #[test]
    fn format_conversation_no_gap() {
        let msgs = [
            msg(MessageRole::User, "hello"),
            msg(MessageRole::Assistant, "world"),
        ];
        let result = format_conversation(&msgs, 1024);
        assert!(!result.contains("[... conversation omitted ...]"));
    }

    #[test]
    fn format_conversation_no_duplicate_when_head_covers_all() {
        let msgs = [
            msg(MessageRole::User, "short"),
            msg(MessageRole::Assistant, "msg"),
        ];
        let result = format_conversation(&msgs, 1024);
        assert_eq!(result.matches("[User]:").count(), 1);
        assert_eq!(result.matches("[Assistant]:").count(), 1);
    }

    #[test]
    fn omission_marker_not_on_same_line_as_tail() {
        let msgs = [
            msg(MessageRole::User, "first"),
            msg(MessageRole::Assistant, "second"),
            msg(MessageRole::User, "third"),
            msg(MessageRole::Assistant, "fourth"),
            msg(MessageRole::User, "fifth"),
        ];
        let result = format_conversation(&msgs, 0);
        let marker_pos = result.find("[... conversation omitted ...]").unwrap();
        let after_marker = &result[marker_pos + "[... conversation omitted ...]".len()..];
        assert!(
            after_marker.starts_with("\n\n"),
            "marker must be separated from next message by \\n\\n, got: {after_marker:?}"
        );
    }
}
