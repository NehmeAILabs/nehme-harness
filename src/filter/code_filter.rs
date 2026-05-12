use regex::Regex;
use std::str::FromStr;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FilterLevel {
    None,
    Minimal,
    Aggressive,
}

impl FromStr for FilterLevel {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "none" => Ok(FilterLevel::None),
            "minimal" => Ok(FilterLevel::Minimal),
            "aggressive" => Ok(FilterLevel::Aggressive),
            _ => Err(format!("Unknown filter level: {}", s)),
        }
    }
}

impl std::fmt::Display for FilterLevel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FilterLevel::None => write!(f, "none"),
            FilterLevel::Minimal => write!(f, "minimal"),
            FilterLevel::Aggressive => write!(f, "aggressive"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Language {
    Rust,
    Python,
    JavaScript,
    TypeScript,
    Go,
    C,
    Cpp,
    Java,
    Ruby,
    Shell,
    Data,
    Unknown,
}

impl Language {
    pub fn from_extension(ext: &str) -> Self {
        match ext.to_lowercase().as_str() {
            "rs" => Language::Rust,
            "py" | "pyw" => Language::Python,
            "js" | "mjs" | "cjs" => Language::JavaScript,
            "ts" | "tsx" => Language::TypeScript,
            "go" => Language::Go,
            "c" | "h" => Language::C,
            "cpp" | "cc" | "cxx" | "hpp" | "hh" => Language::Cpp,
            "java" => Language::Java,
            "rb" => Language::Ruby,
            "sh" | "bash" | "zsh" => Language::Shell,
            "json" | "jsonc" | "json5" | "yaml" | "yml" | "toml" | "xml" | "csv" | "tsv"
            | "graphql" | "gql" | "sql" | "md" | "markdown" | "txt" | "env" | "lock" => {
                Language::Data
            }
            _ => Language::Unknown,
        }
    }

    pub fn comment_patterns(&self) -> CommentPatterns {
        match self {
            Language::Rust => CommentPatterns {
                line: Some("//"),
                block_start: Some("/*"),
                block_end: Some("*/"),
                doc_line: Some("///"),
                doc_block_start: Some("/**"),
            },
            Language::Python => CommentPatterns {
                line: Some("#"),
                block_start: Some("\"\"\""),
                block_end: Some("\"\"\""),
                doc_line: None,
                doc_block_start: Some("\"\"\""),
            },
            Language::JavaScript
            | Language::TypeScript
            | Language::Go
            | Language::C
            | Language::Cpp
            | Language::Java => CommentPatterns {
                line: Some("//"),
                block_start: Some("/*"),
                block_end: Some("*/"),
                doc_line: None,
                doc_block_start: Some("/**"),
            },
            Language::Ruby => CommentPatterns {
                line: Some("#"),
                block_start: Some("=begin"),
                block_end: Some("=end"),
                doc_line: None,
                doc_block_start: None,
            },
            Language::Shell => CommentPatterns {
                line: Some("#"),
                block_start: None,
                block_end: None,
                doc_line: None,
                doc_block_start: None,
            },
            Language::Data => CommentPatterns {
                line: None,
                block_start: None,
                block_end: None,
                doc_line: None,
                doc_block_start: None,
            },
            Language::Unknown => CommentPatterns {
                line: Some("//"),
                block_start: Some("/*"),
                block_end: Some("*/"),
                doc_line: None,
                doc_block_start: None,
            },
        }
    }
}

#[derive(Debug, Clone)]
pub struct CommentPatterns {
    pub line: Option<&'static str>,
    pub block_start: Option<&'static str>,
    pub block_end: Option<&'static str>,
    pub doc_line: Option<&'static str>,
    pub doc_block_start: Option<&'static str>,
}

pub fn filter_source(content: &str, lang: &Language, level: FilterLevel) -> String {
    match level {
        FilterLevel::None => content.to_string(),
        FilterLevel::Minimal => filter_minimal(content, lang),
        FilterLevel::Aggressive => filter_aggressive(content, lang),
    }
}

fn filter_minimal(content: &str, lang: &Language) -> String {
    let patterns = lang.comment_patterns();
    let mut result = String::with_capacity(content.len());
    let mut in_block_comment = false;
    let mut in_docstring = false;

    for line in content.lines() {
        let trimmed = line.trim();

        if let (Some(start), Some(end)) = (patterns.block_start, patterns.block_end) {
            if !in_docstring && trimmed.contains(start) {
                let is_doc_block = patterns
                    .doc_block_start
                    .is_some_and(|db| trimmed.starts_with(db));
                if !is_doc_block {
                    in_block_comment = true;
                }
            }
            if in_block_comment {
                if trimmed.contains(end) {
                    in_block_comment = false;
                }
                continue;
            }
        }

        if *lang == Language::Python && trimmed.starts_with("\"\"\"") {
            in_docstring = !in_docstring;
            result.push_str(line);
            result.push('\n');
            continue;
        }

        if in_docstring {
            result.push_str(line);
            result.push('\n');
            continue;
        }

        if let Some(line_comment) = patterns.line {
            if trimmed.starts_with(line_comment) {
                if let Some(doc) = patterns.doc_line {
                    if trimmed.starts_with(doc) {
                        result.push_str(line);
                        result.push('\n');
                    }
                }
                continue;
            }
        }

        if trimmed.is_empty() {
            result.push('\n');
            continue;
        }

        result.push_str(line);
        result.push('\n');
    }

    let re = Regex::new(r"\n{3,}").unwrap_or_else(|_| Regex::new(r"\n{3}").unwrap());
    let result = re.replace_all(&result, "\n\n");
    result.trim().to_string()
}

fn filter_aggressive(content: &str, lang: &Language) -> String {
    if *lang == Language::Data {
        return filter_minimal(content, lang);
    }

    let minimal = filter_minimal(content, lang);
    let mut result = String::with_capacity(minimal.len() / 2);
    let mut brace_depth = 0i32;
    let mut in_impl_body = false;

    let import_re = Regex::new(r"^(use |import |from |require\(|#include)").unwrap();
    let sig_re = Regex::new(
        r"^(pub\s+)?(async\s+)?(fn|def|function|func|class|struct|enum|trait|interface|type)\s+\w+",
    )
    .unwrap();

    for line in minimal.lines() {
        let trimmed = line.trim();

        if import_re.is_match(trimmed) {
            result.push_str(line);
            result.push('\n');
            continue;
        }

        if sig_re.is_match(trimmed) {
            result.push_str(line);
            result.push('\n');
            in_impl_body = true;
            brace_depth = 0;
            continue;
        }

        let open_braces = trimmed.matches('{').count() as i32;
        let close_braces = trimmed.matches('}').count() as i32;

        if in_impl_body {
            brace_depth += open_braces;
            brace_depth -= close_braces;

            if brace_depth <= 1 && (trimmed == "{" || trimmed == "}" || trimmed.ends_with('{')) {
                result.push_str(line);
                result.push('\n');
            }

            if brace_depth <= 0 {
                in_impl_body = false;
                if !trimmed.is_empty() && trimmed != "}" {
                    result.push_str("    // ... implementation\n");
                }
            }
            continue;
        }

        if trimmed.starts_with("const ")
            || trimmed.starts_with("static ")
            || trimmed.starts_with("let ")
            || trimmed.starts_with("pub const ")
            || trimmed.starts_with("pub static ")
        {
            result.push_str(line);
            result.push('\n');
        }
    }

    result.trim().to_string()
}

pub fn strip_ansi(text: &str) -> String {
    let re =
        Regex::new(r"\x1b\[[0-9;]*[a-zA-Z]").unwrap_or_else(|_| Regex::new(r"\x1b\[").unwrap());
    re.replace_all(text, "").to_string()
}

#[allow(dead_code)]
pub fn smart_file_summary(content: &str, lang: &Language) -> String {
    if matches!(lang, Language::Data | Language::Unknown) {
        let line_count = content.lines().count();
        let size_bytes = content.len();
        return format!("{} lines, {} bytes", line_count, size_bytes);
    }

    let mut doc_comment: Option<String> = None;
    let mut public_symbols: Vec<String> = Vec::new();
    let patterns = lang.comment_patterns();
    let sig_re = Regex::new(
        r"^\s*(?:pub\s+)?(?:async\s+)?(?:fn|def|function|func|class|struct|enum|trait|interface|type|const|static|let|var|impl)\s+(\w+)",
    )
    .unwrap();
    let import_re = Regex::new(r"^(use |import |from |require\(|#include)").unwrap();

    for line in content.lines() {
        let trimmed = line.trim();

        if doc_comment.is_none() {
            if let Some(doc) = patterns.doc_line {
                if trimmed.starts_with(doc) {
                    doc_comment = Some(trimmed.trim_start_matches(doc).trim().to_string());
                    continue;
                }
            }
            if let Some(doc_block) = patterns.doc_block_start {
                if trimmed.starts_with(doc_block)
                    && !trimmed.starts_with(patterns.block_start.unwrap_or(""))
                {
                    let after_start = trimmed.trim_start_matches(doc_block).trim();
                    if !after_start.is_empty() && !after_start.starts_with('*') {
                        doc_comment = Some(after_start.trim_end_matches("*/").trim().to_string());
                    }
                    continue;
                }
            }
            if *lang == Language::Python && trimmed.starts_with("\"\"\"") {
                let after = trimmed
                    .trim_start_matches("\"\"\"")
                    .trim_end_matches("\"\"\"")
                    .trim();
                if !after.is_empty() {
                    doc_comment = Some(after.to_string());
                }
                continue;
            }
        }

        if import_re.is_match(trimmed) {
            continue;
        }

        if let Some(caps) = sig_re.captures(trimmed) {
            if let Some(name) = caps.get(1) {
                let prefix = if trimmed.starts_with("pub ") {
                    "pub "
                } else {
                    ""
                };
                let symbol = format!("{}{}", prefix, name.as_str());
                if !public_symbols.contains(&symbol) {
                    public_symbols.push(symbol);
                }
            }
        }

        if public_symbols.len() >= 15 {
            break;
        }
    }

    let line_count = content.lines().count();

    let mut summary = String::new();

    if let Some(doc) = doc_comment {
        let truncated: String = doc.chars().take(200).collect();
        summary.push_str(&truncated);
        if doc.len() > 200 {
            summary.push_str("...");
        }
        summary.push('\n');
    }

    if public_symbols.is_empty() {
        summary.push_str(&format!("{} lines", line_count));
    } else if public_symbols.len() <= 8 {
        summary.push_str(&format!(
            "{} lines, symbols: {}",
            line_count,
            public_symbols.join(", ")
        ));
    } else {
        let shown = &public_symbols[..8];
        summary.push_str(&format!(
            "{} lines, symbols: {} ... +{} more",
            line_count,
            shown.join(", "),
            public_symbols.len() - 8
        ));
    }

    summary
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_filter_level_parsing() {
        assert_eq!(FilterLevel::from_str("none").unwrap(), FilterLevel::None);
        assert_eq!(
            FilterLevel::from_str("minimal").unwrap(),
            FilterLevel::Minimal
        );
        assert_eq!(
            FilterLevel::from_str("aggressive").unwrap(),
            FilterLevel::Aggressive
        );
    }

    #[test]
    fn test_language_detection() {
        assert_eq!(Language::from_extension("rs"), Language::Rust);
        assert_eq!(Language::from_extension("py"), Language::Python);
        assert_eq!(Language::from_extension("js"), Language::JavaScript);
        assert_eq!(Language::from_extension("ts"), Language::TypeScript);
        assert_eq!(Language::from_extension("go"), Language::Go);
        assert_eq!(Language::from_extension("java"), Language::Java);
    }

    #[test]
    fn test_language_detection_data_formats() {
        assert_eq!(Language::from_extension("json"), Language::Data);
        assert_eq!(Language::from_extension("yaml"), Language::Data);
        assert_eq!(Language::from_extension("yml"), Language::Data);
        assert_eq!(Language::from_extension("toml"), Language::Data);
        assert_eq!(Language::from_extension("xml"), Language::Data);
        assert_eq!(Language::from_extension("csv"), Language::Data);
        assert_eq!(Language::from_extension("md"), Language::Data);
        assert_eq!(Language::from_extension("lock"), Language::Data);
    }

    #[test]
    fn test_json_no_comment_stripping() {
        let json = r#"{
  "workspaces": {
    "packages": [
      "packages/*"
    ]
  },
  "scripts": {
    "build": "bun run --workspaces build"
  }
}"#;
        let result = filter_source(json, &Language::Data, FilterLevel::Minimal);
        assert!(
            result.contains("packages/*"),
            "packages/* should not be treated as block comment start"
        );
        assert!(result.contains("scripts"), "scripts must be preserved");
    }

    #[test]
    fn test_json_aggressive_preserves_structure() {
        let json = r#"{
  "name": "my-app",
  "scripts": {
    "dev": "next dev /* not a comment */"
  }
}"#;
        let result = filter_source(json, &Language::Data, FilterLevel::Aggressive);
        assert!(
            result.contains("/* not a comment */"),
            "Aggressive filter must not strip comment-like patterns in JSON"
        );
    }

    #[test]
    fn test_minimal_filter_removes_comments() {
        let code = r#"
// This is a comment
fn main() {
    println!("Hello");
}
"#;
        let result = filter_source(code, &Language::Rust, FilterLevel::Minimal);
        assert!(!result.contains("// This is a comment"));
        assert!(result.contains("fn main()"));
    }

    #[test]
    fn test_minimal_filter_keeps_doc_comments() {
        let code = r#"
/// This is a doc comment
fn main() {
    println!("Hello");
}
"#;
        let result = filter_source(code, &Language::Rust, FilterLevel::Minimal);
        assert!(result.contains("/// This is a doc comment"));
        assert!(result.contains("fn main()"));
    }

    #[test]
    fn test_aggressive_filter_strips_bodies() {
        let code = r#"
use std::io;

fn short() -> i32 {
    42
}

fn long_function(x: i32) -> i32 {
    let a = x + 1;
    let b = a * 2;
    let c = b + 3;
    let d = c - 4;
    a + b + c + d
}
"#;
        let result = filter_source(code, &Language::Rust, FilterLevel::Aggressive);
        assert!(result.contains("use std::io;"));
        assert!(result.contains("fn short()"));
        assert!(result.contains("fn long_function"));
        assert!(!result.contains("let a = x + 1"));
    }

    #[test]
    fn test_aggressive_filter_keeps_constants() {
        let code = r#"
const MAX_SIZE: usize = 1024;
pub static VERSION: &str = "1.0";

fn main() {
    println!("hello");
}
"#;
        let result = filter_source(code, &Language::Rust, FilterLevel::Aggressive);
        assert!(result.contains("const MAX_SIZE"));
        assert!(result.contains("pub static VERSION"));
    }

    #[test]
    fn test_strip_ansi() {
        let with_ansi = "\x1b[32mgreen text\x1b[0m normal \x1b[1;31mbold red\x1b[0m";
        let stripped = strip_ansi(with_ansi);
        assert_eq!(stripped, "green text normal bold red");
    }

    #[test]
    fn test_strip_ansi_no_codes() {
        let plain = "hello world";
        assert_eq!(strip_ansi(plain), plain);
    }

    #[test]
    fn test_python_docstring_preserved() {
        let code = r#"def foo():
    """This is a docstring."""
    pass
"#;
        let result = filter_source(code, &Language::Python, FilterLevel::Minimal);
        assert!(result.contains("\"\"\"This is a docstring.\"\"\""));
    }

    #[test]
    fn test_block_comment_stripping() {
        let code = r#"
/* This is a block comment
   spanning multiple lines */
fn main() {
    println!("Hello");
}
"#;
        let result = filter_source(code, &Language::Rust, FilterLevel::Minimal);
        assert!(!result.contains("This is a block comment"));
        assert!(result.contains("fn main()"));
    }

    #[test]
    fn test_smart_file_summary_rust() {
        let code = r#"
/// Processes input data and returns results.
pub fn process(data: &str) -> Vec<String> {
    let result = data.lines().map(|l| l.to_string()).collect();
    result
}

pub fn validate(input: &str) -> bool {
    !input.is_empty()
}

fn helper() -> i32 {
    42
}
"#;
        let summary = smart_file_summary(code, &Language::Rust);
        assert!(summary.contains("Processes input data"));
        assert!(summary.contains("pub process"));
        assert!(summary.contains("pub validate"));
        assert!(summary.contains("lines"));
    }

    #[test]
    fn test_smart_file_summary_python() {
        let code = r#"
def calculate(x, y):
    return x + y

class DataStore:
    def __init__(self):
        self.data = {}
"#;
        let summary = smart_file_summary(code, &Language::Python);
        assert!(summary.contains("calculate"));
        assert!(summary.contains("DataStore"));
    }

    #[test]
    fn test_smart_file_summary_data() {
        let summary = smart_file_summary("hello world", &Language::Data);
        assert!(summary.contains("1 lines"));
    }
}
