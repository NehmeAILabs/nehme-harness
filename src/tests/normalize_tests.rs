use crate::agent::tools::normalize::{levenshtein_similarity, normalize_whitespace};

#[test]
fn normalize_tabs_to_spaces() {
    assert_eq!(
        normalize_whitespace("\tfn foo() {\n\t    bar\n\t}\n"),
        "    fn foo() {\n        bar\n    }\n"
    );
}

#[test]
fn normalize_trailing_spaces() {
    assert_eq!(normalize_whitespace("hello   \nworld\n"), "hello\nworld\n");
}

#[test]
fn normalize_collapse_blank_lines() {
    assert_eq!(normalize_whitespace("a\n\n\nb\n"), "a\n\nb\n");
}

#[test]
fn levenshtein_identical() {
    assert!((levenshtein_similarity("hello", "hello") - 1.0).abs() < 0.001);
}

#[test]
fn levenshtein_similar() {
    let sim = levenshtein_similarity("hello world", "helo world");
    assert!(sim > 0.85, "expected >0.85, got {sim}");
}

#[test]
fn levenshtein_different() {
    let sim = levenshtein_similarity("hello", "zzzzz");
    assert!(sim < 0.4, "expected <0.4, got {sim}");
}
