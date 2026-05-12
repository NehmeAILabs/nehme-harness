pub const CAP_ERRORS: usize = 20;
pub const CAP_WARNINGS: usize = 10;
pub const CAP_LIST: usize = 20;
pub const CAP_INVENTORY: usize = 50;

pub const MAX_LINE_LEN: usize = 300;
pub const MAX_PATH_SEGMENTS: usize = 5;

pub const fn reduced(cap: usize, by: usize) -> usize {
    if by < cap { cap - by } else { cap }
}

#[allow(dead_code)]
pub fn smart_truncate(output: &str, max_lines: usize) -> String {
    let lines: Vec<&str> = output.lines().collect();
    if lines.len() <= max_lines {
        return output.to_string();
    }

    let mut kept = Vec::with_capacity(max_lines);
    let mut kept_count = 0;

    for line in &lines {
        let trimmed = line.trim();
        let is_important = trimmed.starts_with("error")
            || trimmed.starts_with("Error")
            || trimmed.starts_with("FAILED")
            || trimmed.starts_with("panic")
            || trimmed.starts_with("warning")
            || trimmed.starts_with("diff --git")
            || trimmed.starts_with("@@")
            || trimmed.starts_with("+")
            || trimmed.starts_with("-")
            || trimmed.contains("error:");

        if is_important || kept_count < max_lines / 2 {
            kept.push((*line).to_string());
            kept_count += 1;
        }
        if kept_count >= max_lines - 1 {
            break;
        }
    }

    kept.push(format!("[{} more lines]", lines.len() - kept_count));
    kept.join("\n")
}

pub fn compact_path(path: &str) -> String {
    let segments: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();
    if segments.len() <= MAX_PATH_SEGMENTS {
        return path.to_string();
    }
    let start_keep = 2;
    let end_keep = MAX_PATH_SEGMENTS - 2;
    let mut result = String::new();
    if path.starts_with('/') {
        result.push('/');
    }
    for (i, seg) in segments.iter().enumerate() {
        if i > 0 {
            result.push('/');
        }
        if i < start_keep {
            result.push_str(seg);
        } else if i == start_keep {
            result.push_str("...");
        } else if i >= segments.len() - end_keep {
            result.push_str(seg);
        }
    }
    result
}

pub fn clean_line(line: &str, max_len: usize) -> String {
    if line.len() <= max_len {
        return line.to_string();
    }
    let mid = max_len / 2;
    format!("{}...{}", &line[..mid], &line[line.len() - mid..])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_reduced_preserves_current_values() {
        assert_eq!(reduced(CAP_WARNINGS, 5), 5);
        assert_eq!(reduced(CAP_LIST, 5), 15);
    }

    #[test]
    fn test_reduced_falls_back_to_cap_when_offset_too_large() {
        assert_eq!(reduced(4, 5), 4);
        assert_eq!(reduced(5, 5), 5);
    }

    #[test]
    fn test_reduced_honors_zero_cap() {
        assert_eq!(reduced(0, 5), 0);
    }

    #[test]
    fn test_smart_truncate_no_truncation_when_under_limit() {
        let input = "a\nb\nc\n";
        let output = smart_truncate(input, 10);
        assert_eq!(output, input);
    }

    #[test]
    fn test_smart_truncate_exact_limit() {
        let input = "a\nb\nc";
        let output = smart_truncate(input, 3);
        assert_eq!(output, input);
    }

    #[test]
    fn test_smart_truncate_overflow() {
        let input: String = (0..100)
            .map(|i| format!("line {}", i))
            .collect::<Vec<_>>()
            .join("\n");
        let output = smart_truncate(&input, 10);
        assert!(output.contains("more lines]"));
        assert!(output.len() < input.len());
    }

    #[test]
    fn test_compact_path_short() {
        assert_eq!(compact_path("src/main.rs"), "src/main.rs");
        assert_eq!(compact_path("a/b/c"), "a/b/c");
    }

    #[test]
    fn test_compact_path_absolute() {
        let result = compact_path("/a/b/c/d/e/f/g/h/i/file.rs");
        assert!(result.starts_with('/'));
        assert!(result.contains("..."));
    }

    #[test]
    fn test_compact_path_long() {
        let path = "a/b/c/d/e/f/g/h/file.rs";
        let compacted = compact_path(path);
        assert!(compacted.contains("..."));
        assert!(compacted.ends_with("file.rs"));
        assert!(compacted.starts_with("a/b"));
    }

    #[test]
    fn test_clean_line_short() {
        assert_eq!(clean_line("hello world", 300), "hello world");
    }

    #[test]
    fn test_clean_line_exact_limit() {
        let line = "a".repeat(100);
        assert_eq!(clean_line(&line, 100), line);
    }

    #[test]
    fn test_clean_line_long() {
        let line: String = "x".repeat(500);
        let cleaned = clean_line(&line, 300);
        assert!(cleaned.contains("..."));
        assert!(cleaned.len() <= 303);
    }

    #[test]
    fn test_clean_line_preserves_start_and_end() {
        let line = "abcdefghij".repeat(50);
        let cleaned = clean_line(&line, 20);
        assert!(cleaned.starts_with("abcdefghij"));
        assert!(cleaned.ends_with("abcdefghij"));
    }
}
