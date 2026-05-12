use crate::permission::{self, SecurityMode};
use crate::session::MessageRole;
use crate::ui::slash::{SlashCtx, write_error, write_ok};

fn is_session_empty(ctx: &SlashCtx<'_>) -> bool {
    !ctx.session
        .messages
        .iter()
        .any(|m| m.role == MessageRole::User)
}

fn is_in_worktree() -> bool {
    #[cfg(feature = "git-worktree")]
    {
        crate::extras::git_worktree::detect().is_some()
    }
    #[cfg(not(feature = "git-worktree"))]
    {
        false
    }
}

fn build_default_review_message(session_empty: bool, in_worktree: bool) -> String {
    match (session_empty, in_worktree) {
        (true, true) => "Review the current worktree state. Check the diff from the base branch \
                         for correctness, design, testing, and security."
            .to_string(),
        (true, false) => "Review the current codebase for correctness, design, testing, and \
                          security."
            .to_string(),
        (false, true) => "Review the changes in this worktree session. Consider the diff from \
                          main since the branch was created. Check for correctness, design, \
                          testing, and security."
            .to_string(),
        (false, false) => "Review the changes discussed in this session for correctness, \
                           design, testing, and security."
            .to_string(),
    }
}

pub async fn handle(parts: &[&str], ctx: &mut SlashCtx<'_>) -> anyhow::Result<()> {
    if !ctx.context.prompts.contains_key("review") {
        write_error(
            ctx.renderer,
            "no 'review' prompt found. Run /regen-prompts first.",
        );
        return Ok(());
    }

    let msg = if parts.len() > 1 {
        parts[1..].join(" ")
    } else {
        let session_empty = is_session_empty(ctx);
        let in_worktree = is_in_worktree();
        build_default_review_message(session_empty, in_worktree)
    };

    // Save current prompt for one-shot restoration
    ctx.context.one_shot_restore = ctx.context.current_prompt_name.clone();

    // Switch to review prompt
    if let Some(content) = ctx.context.prompts.get("review").cloned() {
        let (mode_directive_str, clean_content) = permission::parse_prompt_mode(&content);
        let mode_directive = mode_directive_str.map(|s| s.to_string());
        ctx.context.current_prompt = Some(if mode_directive.is_some() {
            clean_content.to_string()
        } else {
            content
        });
        ctx.context.current_prompt_name = Some("review".to_string());
        if let Some(ref mode_str) = mode_directive {
            if mode_str == "last_user_mode" {
                if let Some(perm) = ctx.permission {
                    let mut guard = perm.lock().unwrap_or_else(|e| e.into_inner());
                    guard.restore_user_mode();
                }
            } else if let Some(mode) = SecurityMode::from_str(mode_str)
                && let Some(perm) = ctx.permission
            {
                let mut guard = perm.lock().unwrap_or_else(|e| e.into_inner());
                guard.set_prompt_mode(mode);
            }
        }
    }

    ctx.rebuild_agent().await;
    write_ok(ctx.renderer, format!("review: {}", msg));

    Err(anyhow::anyhow!("DEFER_REVIEW:{}", msg))
}
