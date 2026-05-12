use rig::completion::ToolDefinition;
use rig::tool::Tool;

use crate::agent::tools::crc::crc32_hex;
use crate::agent::tools::{
    AskSender, PermCheck, ReadArgs, ToolError, check_perm_path, edit_system,
};
use crate::config::types::EditSystem;
use crate::filter::code_filter::{FilterLevel, Language, filter_source};

#[cfg(feature = "context")]
use crate::extras::context::{auto_index_output, event, record_event};

const DEFAULT_MAX_TEXT_SIZE: u64 = 1024 * 1024;
const SOURCE_FILTER_THRESHOLD: usize = 200;

pub struct ReadTool {
    pub permission: Option<PermCheck>,
    pub ask_tx: Option<AskSender>,
    pub max_text_file_size: u64,
    pub max_lines: u64,
    pub session_id: String,
}

impl ReadTool {
    pub fn new(
        permission: Option<PermCheck>,
        ask_tx: Option<AskSender>,
        max_text_file_size: Option<u64>,
        max_lines: u64,
        session_id: String,
    ) -> Self {
        ReadTool {
            permission,
            ask_tx,
            max_text_file_size: max_text_file_size.unwrap_or(DEFAULT_MAX_TEXT_SIZE),
            max_lines,
            session_id,
        }
    }
}

impl Tool for ReadTool {
    const NAME: &'static str = "read";

    type Error = ToolError;
    type Args = ReadArgs;
    type Output = String;

    async fn definition(&self, _prompt: String) -> ToolDefinition {
        let (desc, params) = match edit_system() {
            EditSystem::Similarity => (
                format!(
                    "Read the contents of a file. Supports text files. Defaults to first {} lines. Use offset/limit for large files.",
                    self.max_lines
                ),
                serde_json::json!({
                    "type": "object",
                    "properties": {
                        "path": { "type": "string", "description": "Path to the file (relative or absolute)" },
                        "offset": { "type": "integer", "description": "Line number to start from (1-indexed)" },
                        "limit": { "type": "integer", "description": "Maximum number of lines to read" },
                        "intent": { "type": "string", "description": "What you're looking for in the file. If file is large, only matching lines are returned (full content is indexed for ctx_search). Omit for full content." }
                    },
                    "required": ["path"]
                }),
            ),
            EditSystem::Hashedit => (
                format!(
                    "Read file contents with CRC-32 tagged lines for tag-based editing. Each line is prefixed with 'N|TAG' where TAG is an 8-char hex CRC-32 of the line content. Use these tags with the edit tool for CAS-guarded edits. Defaults to first {} lines.",
                    self.max_lines
                ),
                serde_json::json!({
                    "type": "object",
                    "properties": {
                        "path": { "type": "string", "description": "Path to the file (relative or absolute)" },
                        "offset": { "type": "integer", "description": "Line number to start from (1-indexed)" },
                        "limit": { "type": "integer", "description": "Maximum number of lines to read" },
                        "intent": { "type": "string", "description": "What you're looking for in the file. If file is large, only matching lines are returned (full content is indexed for ctx_search). Omit for full content." }
                    },
                    "required": ["path"]
                }),
            ),
        };

        ToolDefinition {
            name: "read".to_string(),
            description: desc,
            parameters: params,
        }
    }

    async fn call(&self, args: ReadArgs) -> Result<String, ToolError> {
        let path = crate::fs::expand_tilde(&args.path);
        let coaching = check_perm_path(&self.permission, &self.ask_tx, "read", &path).await?;

        #[cfg(feature = "context")]
        record_event(
            &self.session_id,
            event::FILE_READ,
            event::PRIORITY_LOW,
            "read",
            &path,
            &format!(
                "{{\"offset\":{},\"limit\":{}}}",
                args.offset.unwrap_or(1),
                args.limit.unwrap_or(0)
            ),
        );

        let offset = args.offset.unwrap_or(1).saturating_sub(1);
        let limit = args.limit.unwrap_or(self.max_lines as usize);

        if let Some(msg) = crate::agent::tools::track_read(&path, offset, limit) {
            return Err(ToolError::Msg(msg));
        }

        let metadata = tokio::fs::metadata(&path).await?;
        let file_size = metadata.len();
        if file_size > self.max_text_file_size {
            return Err(ToolError::Msg(format!(
                "File too large ({} bytes). Maximum allowed file size is {} bytes.",
                file_size, self.max_text_file_size
            )));
        }
        let content = tokio::fs::read_to_string(&path).await?;
        let total_lines = content.lines().count();

        let end = (offset + limit).min(total_lines);

        // Track read lifecycle — detect stale/superseded reads
        let stale_info =
            crate::agent::read_lifecycle::record_read(&path, offset, end - offset, content.len());

        let es = edit_system();

        let excerpt: String = match es {
            EditSystem::Hashedit => content
                .lines()
                .skip(offset)
                .take(end - offset)
                .enumerate()
                .map(|(i, line)| {
                    let line_num = offset + i + 1;
                    let tag = crc32_hex(line.as_bytes());
                    let line_num_width = if total_lines >= 1000 { 4 } else { 3 };
                    format!(
                        "{:>width$}|{} {}",
                        line_num,
                        tag,
                        line,
                        width = line_num_width
                    )
                })
                .collect::<Vec<_>>()
                .join("\n"),
            EditSystem::Similarity => {
                let raw: String = content
                    .lines()
                    .skip(offset)
                    .take(end - offset)
                    .collect::<Vec<_>>()
                    .join("\n");

                let ext = path.rsplit('.').next().unwrap_or("");
                let lang = Language::from_extension(ext);
                let excerpt_lines = raw.lines().count();
                if args.intent.is_some()
                    && lang != Language::Data
                    && lang != Language::Unknown
                    && excerpt_lines > SOURCE_FILTER_THRESHOLD
                {
                    let filtered = filter_source(&raw, &lang, FilterLevel::Minimal);
                    if filtered.len() < raw.len() {
                        let saved_pct = ((raw.len() - filtered.len()) * 100) / raw.len().max(1);
                        format!("[source filtered: -{}%]\n{}", saved_pct, filtered)
                    } else {
                        raw
                    }
                } else {
                    raw
                }
            }
        };

        let info = match es {
            EditSystem::Hashedit => {
                let file_crc = crc32_hex(content.replace("\r\n", "\n").as_bytes());
                format!(
                    "File: {} ({} lines total, lines {}-{}) [CRC: {}]\n\n{}",
                    path,
                    total_lines,
                    offset + 1,
                    end,
                    file_crc,
                    excerpt
                )
            }
            EditSystem::Similarity => {
                format!(
                    "File: {} ({} lines total, showing lines {}-{})\n\n{}",
                    path,
                    total_lines,
                    offset + 1,
                    end,
                    excerpt
                )
            }
        };

        let info = if end < total_lines {
            let remaining = total_lines - end;
            format!(
                "{}\n\n[truncated after {} lines — {} more lines (lines {}-{}); re-call with offset/limit to see more]",
                info,
                end - offset,
                remaining,
                end + 1,
                total_lines,
            )
        } else {
            info
        };

        let info = match coaching {
            Some(msg) => format!("{}\n\n{}", msg, info),
            None => info,
        };

        // Add stale read coaching if previous reads are now stale/superseded
        let info = if let Some(stale_msg) = stale_info.coaching_message() {
            format!("{}\n\n{}", stale_msg, info)
        } else {
            info
        };

        #[cfg(feature = "context")]
        {
            if let Some(ref intent) = args.intent
                && info.len() > 5000
            {
                let filtered = crate::extras::context::intent_filter(&info, intent);
                auto_index_output(&self.session_id, "read", &path, &info, &filtered);
                return Ok(format!(
                    "{}\n\n[{} bytes original, {} bytes returned — full content indexed, use ctx_search to retrieve]",
                    filtered,
                    info.len(),
                    filtered.len()
                ));
            }
        }

        #[cfg(feature = "context")]
        {
            let raw_len = content.len();
            let info_len = info.len();
            let bytes_saved = raw_len.saturating_sub(info_len);
            if bytes_saved > 2048 {
                auto_index_output(&self.session_id, "read", &path, &content, &info);
            }
        }

        Ok(info)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::tools::{set_deny_repeated_reads, set_edit_system};
    use rig::tool::Tool;

    #[tokio::test]
    async fn similarity_read_without_intent_preserves_large_source_comments() {
        set_edit_system(EditSystem::Similarity);
        set_deny_repeated_reads(false);

        let dir =
            std::env::temp_dir().join(format!("nehme-harness-read-test-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("large.rs");
        let mut content = String::new();
        for i in 1..=220 {
            content.push_str(&format!("// important comment {i}\n"));
            content.push_str(&format!("fn generated_{i}() {{}}\n"));
        }
        std::fs::write(&path, content).unwrap();

        let tool = ReadTool::new(None, None, None, 500, "test-session".to_string());
        let out = tool
            .call(ReadArgs {
                path: path.to_string_lossy().to_string(),
                offset: Some(1),
                limit: Some(440),
                intent: None,
            })
            .await
            .unwrap();

        assert!(out.contains("// important comment 1"));
        assert!(out.contains("// important comment 220"));
        assert!(!out.contains("[source filtered:"));

        std::fs::remove_dir_all(dir).unwrap();
    }
}
