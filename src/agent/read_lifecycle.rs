use std::collections::HashMap;
use std::sync::Mutex;

use crate::filter::code_filter::Language;

/// Tracks read calls to detect stale and superseded reads.
///
/// A read is **stale** when the file has been edited since the read.
/// A read is **superseded** when the same file+range was read again later.
///
/// Both conditions mean the old read result is wasting context — the model
/// either has fresh data (superseded) or wrong data (stale).
static READ_HISTORY: Mutex<Vec<ReadEntry>> = Mutex::new(Vec::new());

/// Files that have been edited/written since the last read.
static EDITED_FILES: Mutex<Vec<String>> = Mutex::new(Vec::new());

#[derive(Clone, Debug)]
struct ReadEntry {
    path: String,
    offset: usize,
    limit: usize,
    byte_size: usize,
    #[allow(dead_code)]
    language: Language,
    turn: usize,
}

static TURN_COUNTER: Mutex<usize> = Mutex::new(0);

/// Called at the start of each agent turn to advance the turn counter.
pub fn advance_turn() {
    let mut t = TURN_COUNTER.lock().unwrap_or_else(|e| e.into_inner());
    *t += 1;
}

pub fn current_turn() -> usize {
    *TURN_COUNTER.lock().unwrap_or_else(|e| e.into_inner())
}

/// Record a read call. Returns a `StaleInfo` describing whether any prior
/// reads of this file are now stale or superseded.
pub fn record_read(path: &str, offset: usize, limit: usize, byte_size: usize) -> StaleInfo {
    let turn = current_turn();
    let ext = path.rsplit('.').next().unwrap_or("");
    let language = Language::from_extension(ext);

    let entry = ReadEntry {
        path: path.to_string(),
        offset,
        limit,
        byte_size,
        language,
        turn,
    };

    let mut history = READ_HISTORY.lock().unwrap_or_else(|e| e.into_inner());
    let mut stale_bytes = 0usize;
    let mut superseded_bytes = 0usize;
    let mut stale_count = 0u32;
    let mut superseded_count = 0u32;

    let edited = EDITED_FILES.lock().unwrap_or_else(|e| e.into_inner());
    let is_edited = edited.iter().any(|p| p == path);
    drop(edited);

    for prev in history.iter() {
        if prev.path != path {
            continue;
        }
        // Check overlap
        let prev_end = prev.offset + prev.limit;
        let new_end = offset + limit;
        let overlaps = offset < prev_end && prev.offset < new_end;

        if !overlaps {
            continue;
        }

        if is_edited {
            // File was edited since the old read — old data is wrong
            stale_bytes += prev.byte_size;
            stale_count += 1;
        } else if prev.turn < turn {
            // Same range re-read without edit — old read is redundant
            superseded_bytes += prev.byte_size;
            superseded_count += 1;
        }
    }

    history.push(entry);

    StaleInfo {
        stale_bytes,
        superseded_bytes,
        stale_count,
        superseded_count,
        file_was_edited: is_edited,
    }
}

/// Mark a file as edited. All prior reads of this file become stale.
pub fn mark_file_edited(path: &str) {
    let mut edited = EDITED_FILES.lock().unwrap_or_else(|e| e.into_inner());
    if !edited.iter().any(|p| p == path) {
        edited.push(path.to_string());
    }

    // Clear the read tracker so re-reads are allowed (model needs fresh data)
    crate::agent::tools::untrack_read_path(path);
}

/// Generate a compression summary for dropped content.
/// Returns a string like "150 log entries (3 with errors), 200 test results (12 failures)".
pub fn summarize_dropped_content(content: &str, language: &Language) -> String {
    let lines: Vec<&str> = content.lines().collect();
    let total = lines.len();

    if total == 0 {
        return "0 lines".to_string();
    }

    // Count by category
    let mut errors = 0u32;
    let mut warnings = 0u32;
    let mut tests_pass = 0u32;
    let mut tests_fail = 0u32;
    let mut functions = 0u32;
    let mut imports = 0u32;
    let mut blank = 0u32;

    for line in &lines {
        let trimmed = line.trim();
        let lower = trimmed.to_lowercase();
        if trimmed.is_empty() {
            blank += 1;
        } else if lower.contains("error") || lower.contains("failed") || lower.contains("panic") {
            errors += 1;
        } else if lower.contains("warning") || lower.starts_with("warn") {
            warnings += 1;
        } else if lower.contains("... ok") || lower.contains("passed") {
            tests_pass += 1;
        } else if lower.contains("... failed") || lower.contains("fail") {
            tests_fail += 1;
        } else if matches!(
            language,
            Language::Rust
                | Language::Python
                | Language::JavaScript
                | Language::TypeScript
                | Language::Go
                | Language::Java
                | Language::C
                | Language::Cpp
        ) {
            if lower.starts_with("fn ")
                || lower.starts_with("def ")
                || lower.starts_with("function ")
                || lower.starts_with("func ")
            {
                functions += 1;
            } else if lower.starts_with("import ")
                || lower.starts_with("use ")
                || lower.starts_with("#include")
                || lower.starts_with("from ")
            {
                imports += 1;
            }
        }
    }

    let mut parts: Vec<String> = Vec::new();
    if errors > 0 {
        parts.push(format!("{} error lines", errors));
    }
    if warnings > 0 {
        parts.push(format!("{} warnings", warnings));
    }
    if tests_pass > 0 || tests_fail > 0 {
        parts.push(format!(
            "{} test results ({} failures)",
            tests_pass + tests_fail,
            tests_fail
        ));
    }
    if functions > 0 {
        parts.push(format!("{} functions", functions));
    }
    if imports > 0 {
        parts.push(format!("{} imports", imports));
    }
    if blank > 0 && parts.is_empty() {
        parts.push(format!("{} lines", total));
    }

    if parts.is_empty() {
        format!("{} lines", total)
    } else {
        format!("{} lines: {}", total, parts.join(", "))
    }
}

pub struct StaleInfo {
    pub stale_bytes: usize,
    pub superseded_bytes: usize,
    pub stale_count: u32,
    pub superseded_count: u32,
    #[allow(dead_code)]
    pub file_was_edited: bool,
}

impl StaleInfo {
    pub fn has_stale_reads(&self) -> bool {
        self.stale_count > 0 || self.superseded_count > 0
    }

    /// Generate a coaching message for the model about stale/superseded reads.
    pub fn coaching_message(&self) -> Option<String> {
        if !self.has_stale_reads() {
            return None;
        }

        let mut parts: Vec<String> = Vec::new();

        if self.stale_count > 0 {
            parts.push(format!(
                "{} previous read(s) of this file are now STALE (file was edited — old data is wrong, {} bytes wasted in context)",
                self.stale_count,
                format_bytes(self.stale_bytes)
            ));
        }

        if self.superseded_count > 0 {
            parts.push(format!(
                "{} previous read(s) of this file are SUPERSEDED (re-read without edit — old data is redundant, {} bytes wasted in context)",
                self.superseded_count,
                format_bytes(self.superseded_bytes)
            ));
        }

        if parts.is_empty() {
            None
        } else {
            Some(format!("[Read lifecycle: {}]", parts.join("; ")))
        }
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

/// Reset all tracking state (e.g. on new session).
#[allow(dead_code)]
pub fn reset() {
    let mut history = READ_HISTORY.lock().unwrap_or_else(|e| e.into_inner());
    history.clear();
    let mut edited = EDITED_FILES.lock().unwrap_or_else(|e| e.into_inner());
    edited.clear();
    let mut turn = TURN_COUNTER.lock().unwrap_or_else(|e| e.into_inner());
    *turn = 0;
}

/// Get a snapshot of read history for debugging.
#[allow(dead_code)]
pub fn history_snapshot() -> HashMap<String, u32> {
    let history = READ_HISTORY.lock().unwrap_or_else(|e| e.into_inner());
    let mut counts: HashMap<String, u32> = HashMap::new();
    for entry in history.iter() {
        *counts.entry(entry.path.clone()).or_insert(0) += 1;
    }
    counts
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_stale_detection() {
        reset();
        advance_turn(); // turn 1

        // Read a file
        let info = record_read("src/main.rs", 0, 100, 5000);
        assert!(!info.has_stale_reads());

        advance_turn(); // turn 2

        // Mark file as edited
        mark_file_edited("src/main.rs");

        advance_turn(); // turn 3

        // Re-read the same file — old read should be stale
        let info = record_read("src/main.rs", 0, 100, 5000);
        assert!(info.stale_count > 0);
        assert!(info.file_was_edited);
    }

    #[test]
    fn test_superseded_detection() {
        reset();
        advance_turn(); // turn 1

        // Read a file
        record_read("src/lib.rs", 0, 50, 2000);

        advance_turn(); // turn 2

        // Re-read same file without edit — should be superseded
        let info = record_read("src/lib.rs", 0, 50, 2000);
        assert!(info.superseded_count > 0);
        assert!(!info.file_was_edited);
    }

    #[test]
    fn test_no_overlap_no_stale() {
        reset();
        advance_turn();

        // Read different sections of same file
        record_read("src/main.rs", 0, 50, 2000);
        advance_turn();

        let info = record_read("src/main.rs", 200, 50, 2000);
        assert!(!info.has_stale_reads());
    }

    #[test]
    fn test_summarize_dropped_content_logs() {
        let content = "test foo ... ok\ntest bar ... FAILED\ntest baz ... ok\nerror: something broke\nwarning: unused variable";
        let summary = summarize_dropped_content(content, &Language::Unknown);
        assert!(summary.contains("error"));
        assert!(summary.contains("warning"));
    }

    #[test]
    fn test_summarize_dropped_content_code() {
        let content = "fn foo() {}\nfn bar() {}\nimport os\nuse std::io";
        let summary = summarize_dropped_content(content, &Language::Rust);
        assert!(summary.contains("functions"));
    }
}
