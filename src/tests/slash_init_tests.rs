use crate::ui::slash::init::AGENTS_CREATION_PROMPT;

#[test]
fn test_prompt_is_non_empty() {
    assert!(!AGENTS_CREATION_PROMPT.is_empty());
}

#[test]
fn test_prompt_contains_key_phrases() {
    assert!(AGENTS_CREATION_PROMPT.contains("AGENTS.md"));
    assert!(AGENTS_CREATION_PROMPT.contains("build"));
    assert!(AGENTS_CREATION_PROMPT.contains("test"));
    assert!(AGENTS_CREATION_PROMPT.contains("coding agent"));
}

#[test]
fn test_prompt_starts_with_create() {
    assert!(AGENTS_CREATION_PROMPT.starts_with("Create an AGENTS.md"));
}
