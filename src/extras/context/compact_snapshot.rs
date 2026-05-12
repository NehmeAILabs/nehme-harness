use crate::session::{MessageRole, SessionMessage};

const MAX_SNAPSHOT_BYTES: usize = 8192;

pub fn extract_snapshot(messages: &[SessionMessage]) -> String {
    let mut p1 = Vec::new();
    let mut p2 = Vec::new();
    let mut p3 = Vec::new();

    for msg in messages.iter().rev() {
        let content = msg.content.trim();
        if content.is_empty() {
            continue;
        }

        if is_p1(content, msg.role) {
            p1.push(content.to_string());
        } else if is_p2(content, msg.role) {
            p2.push(content.to_string());
        } else if is_p3(content) {
            p3.push(content.to_string());
        }
    }

    p1.reverse();
    p2.reverse();
    p3.reverse();

    let mut snapshot = String::new();

    if !p1.is_empty() {
        snapshot.push_str("## P1 (must keep)\n");
        for item in &p1 {
            snapshot.push_str("- ");
            snapshot.push_str(item);
            snapshot.push('\n');
        }
    }

    let budget = MAX_SNAPSHOT_BYTES.saturating_sub(snapshot.len());
    if budget > 100 && !p2.is_empty() {
        snapshot.push_str("\n## P2 (should keep)\n");
        let p2_text = p2.join("\n");
        let truncated: String = p2_text.chars().take(budget.saturating_sub(20)).collect();
        snapshot.push_str(&truncated);
        snapshot.push('\n');
    }

    let budget = MAX_SNAPSHOT_BYTES.saturating_sub(snapshot.len());
    if budget > 100 && !p3.is_empty() {
        snapshot.push_str("\n## P3 (nice to keep)\n");
        let p3_text = p3.join("\n");
        let truncated: String = p3_text.chars().take(budget.saturating_sub(20)).collect();
        snapshot.push_str(&truncated);
        snapshot.push('\n');
    }

    snapshot
}

fn is_p1(content: &str, role: MessageRole) -> bool {
    if role == MessageRole::System {
        return true;
    }
    let lower = content.to_lowercase();
    lower.contains("error")
        || lower.contains("fail")
        || lower.contains("blocked")
        || lower.contains("todo")
        || lower.contains("fixme")
        || lower.contains("task:")
        || lower.contains("current task")
        || lower.contains("next step")
        || lower.contains("working on")
        || lower.contains("decided")
        || lower.contains("decision:")
}

fn is_p2(content: &str, role: MessageRole) -> bool {
    if role == MessageRole::Assistant {
        let lower = content.to_lowercase();
        return lower.contains("wrote")
            || lower.contains("created")
            || lower.contains("modified")
            || lower.contains("edited")
            || lower.contains("deleted")
            || lower.contains("running")
            || lower.contains("test")
            || lower.contains("build")
            || lower.contains("compile")
            || lower.contains("passed")
            || lower.contains("result:");
    }
    false
}

fn is_p3(content: &str) -> bool {
    let lower = content.to_lowercase();
    !(lower.contains("hello")
        || lower.contains("hi there")
        || lower.contains("thanks")
        || lower.contains("got it")
        || lower.contains("sure"))
        && content.len() < 500
}

#[cfg(test)]
mod tests {
    use super::*;
    use compact_str::CompactString;

    fn msg(role: MessageRole, content: &str) -> SessionMessage {
        SessionMessage {
            role,
            content: CompactString::from(content),
            estimated_tokens: 0,
        }
    }

    #[test]
    fn test_extract_snapshot_p1_system() {
        let messages = vec![msg(MessageRole::System, "You are a coding assistant")];
        let snapshot = extract_snapshot(&messages);
        assert!(snapshot.contains("P1 (must keep)"));
        assert!(snapshot.contains("You are a coding assistant"));
    }

    #[test]
    fn test_extract_snapshot_p1_error() {
        let messages = vec![msg(
            MessageRole::Assistant,
            "error: failed to compile src/main.rs",
        )];
        let snapshot = extract_snapshot(&messages);
        assert!(snapshot.contains("P1 (must keep)"));
    }

    #[test]
    fn test_extract_snapshot_p2_file_change() {
        let messages = vec![msg(
            MessageRole::Assistant,
            "I wrote the changes to src/lib.rs",
        )];
        let snapshot = extract_snapshot(&messages);
        assert!(snapshot.contains("P2 (should keep)"));
    }

    #[test]
    fn test_extract_snapshot_empty() {
        let messages: Vec<SessionMessage> = vec![];
        let snapshot = extract_snapshot(&messages);
        assert!(snapshot.is_empty());
    }

    #[test]
    fn test_snapshot_size_limit() {
        let mut messages = vec![];
        for i in 0..50 {
            messages.push(msg(
                MessageRole::User,
                &format!(
                    "This is a long message number {} with lots of content to fill up space",
                    i
                ),
            ));
        }
        let snapshot = extract_snapshot(&messages);
        assert!(
            snapshot.len() <= 8400,
            "snapshot too large: {} bytes",
            snapshot.len()
        );
    }
}
