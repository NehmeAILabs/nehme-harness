//! Tests for the `subagents` feature.
//!
//! Run with: cargo test --features subagents
//!
//! These tests cover the pure-logic portions that don't require an actual
//! LLM: argument parsing, response truncation, result combining, and the
//! empty-prompts guard.

#[cfg(test)]
mod tests {
    use serde_json;

    // -----------------------------------------------------------------------
    // TaskArgs deserialization
    // -----------------------------------------------------------------------

    #[test]
    fn task_args_deserializes_multiple_prompts() {
        let json = r#"{"prompts": ["explore auth module", "find api routes"]}"#;
        let args: crate::extras::subagents::task_tool::TaskArgs =
            serde_json::from_str(json).unwrap();
        assert_eq!(args.prompts.len(), 2);
        assert_eq!(args.prompts[0], "explore auth module");
        assert_eq!(args.prompts[1], "find api routes");
    }

    #[test]
    fn task_args_single_prompt() {
        let json = r#"{"prompts": ["one thing"]}"#;
        let args: crate::extras::subagents::task_tool::TaskArgs =
            serde_json::from_str(json).unwrap();
        assert_eq!(args.prompts.len(), 1);
        assert_eq!(args.prompts[0], "one thing");
    }

    #[test]
    fn task_args_empty_prompts_deserializes() {
        // The struct itself allows an empty vec; TaskTool::call rejects it.
        let json = r#"{"prompts": []}"#;
        let args: crate::extras::subagents::task_tool::TaskArgs =
            serde_json::from_str(json).unwrap();
        assert!(args.prompts.is_empty());
    }

    #[test]
    fn task_args_missing_prompts_is_error() {
        let json = r#"{}"#;
        let result: Result<crate::extras::subagents::task_tool::TaskArgs, _> =
            serde_json::from_str(json);
        assert!(result.is_err());
    }

    // -----------------------------------------------------------------------
    // Response truncation
    // -----------------------------------------------------------------------

    const CAP: usize = 32 * 1024;
    const MARKER: &str = "\n…[subagent response truncated at 32768B]";

    #[test]
    fn truncate_response_preserves_short_string() {
        let s = "hello world";
        let result = crate::extras::truncate::truncate_cjk(s, CAP, MARKER);
        assert_eq!(result, s);
    }

    #[test]
    fn truncate_response_caps_long_string() {
        let s = "x".repeat(200 * 1024); // 200KB, well over the 32KB cap
        let result = crate::extras::truncate::truncate_cjk(&s, CAP, MARKER);
        assert!(result.len() <= 32 * 1024 + 64); // cap + marker overhead
        assert!(result.contains("[subagent response truncated"));
    }

    #[test]
    fn truncate_response_does_not_panic_on_cjk() {
        // Cutting mid-char would panic with a plain String::truncate
        let cjk = "記憶".repeat(50 * 1024); // plenty of multi-byte chars
        let result = crate::extras::truncate::truncate_cjk(&cjk, CAP, MARKER);
        // Must not panic; must contain the marker
        assert!(result.contains("[subagent response truncated"));
    }

    #[test]
    fn truncate_response_starts_with_prefix_of_original() {
        let s = "AAAABBBBCCCCDDDD";
        let result = crate::extras::truncate::truncate_cjk(s, CAP, MARKER);
        assert!(result.starts_with("AAAABB"));
    }

    // -----------------------------------------------------------------------
    // Result combining
    // -----------------------------------------------------------------------

    #[test]
    fn combine_single_result_no_heading() {
        let outputs = vec![(0usize, "explore auth".into(), "Found auth module".into())];
        let combined = crate::extras::subagents::task_tool::combine_results(&outputs);
        assert_eq!(combined, "Found auth module\n");
        assert!(!combined.contains("Task"));
    }

    #[test]
    fn combine_multiple_results_with_headings() {
        let outputs = vec![
            (0, "explore auth".into(), "Found auth".into()),
            (1, "find routes".into(), "3 routes found".into()),
        ];
        let combined = crate::extras::subagents::task_tool::combine_results(&outputs);
        assert!(combined.contains("## Task 1:"));
        assert!(combined.contains("## Task 2:"));
        assert!(combined.contains("Found auth"));
        assert!(combined.contains("3 routes found"));
    }

    #[test]
    fn combine_results_sorted_by_index() {
        // Results arrive out of order; combine_results trusts the caller to sort.
        // We test the sorted case here.
        let outputs = vec![
            (0, "first".into(), "A".into()),
            (1, "second".into(), "B".into()),
        ];
        let combined = crate::extras::subagents::task_tool::combine_results(&outputs);
        let pos_a = combined.find("A").unwrap();
        let pos_b = combined.find("B").unwrap();
        assert!(pos_a < pos_b);
    }

    #[test]
    fn combine_error_result_is_preserved() {
        let outputs = vec![(0, "prompt".into(), "[error: something went wrong]".into())];
        let combined = crate::extras::subagents::task_tool::combine_results(&outputs);
        assert!(combined.contains("[error: something went wrong]"));
    }

    #[test]
    fn combine_timeout_result_is_preserved() {
        let outputs = vec![(
            0,
            "prompt".into(),
            "[timeout: subagent exceeded 300s]".into(),
        )];
        let combined = crate::extras::subagents::task_tool::combine_results(&outputs);
        assert!(combined.contains("[timeout: subagent exceeded 300s]"));
    }

    #[test]
    fn combine_result_ensures_trailing_newline() {
        let outputs = vec![(0, "p".into(), "no trailing newline".into())];
        let combined = crate::extras::subagents::task_tool::combine_results(&outputs);
        assert!(combined.ends_with('\n'));
    }

    #[test]
    fn combine_result_already_has_newline_no_double() {
        let outputs = vec![(0, "p".into(), "already has newline\n".into())];
        let combined = crate::extras::subagents::task_tool::combine_results(&outputs);
        assert!(combined.ends_with('\n'));
        // Single result shouldn't have double newline at end
        let result_part = combined.trim_end();
        assert_eq!(result_part, "already has newline");
    }

    #[test]
    fn combine_empty_outputs() {
        let outputs: Vec<(usize, String, String)> = vec![];
        let combined = crate::extras::subagents::task_tool::combine_results(&outputs);
        assert!(combined.is_empty());
    }

    #[test]
    fn combine_long_prompt_label_truncated_to_60_chars() {
        let long_prompt = "x".repeat(100);
        let outputs = vec![
            (0, long_prompt.clone(), "result".into()),
            (1, "second".into(), "B".into()),
        ];
        let combined = crate::extras::subagents::task_tool::combine_results(&outputs);
        let heading_line = combined.lines().next().unwrap();
        assert!(heading_line.len() <= "## Task 1: ".len() + 60);
    }
}
