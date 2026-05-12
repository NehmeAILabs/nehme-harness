use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use rig::completion::ToolDefinition;
use rig::tool::Tool;
use serde::Deserialize;
use serde_json::json;

use crate::agent::tools::{PermCheck, ToolError, check_perm};
use crate::permission::ask::AskSender;

pub(crate) fn code_graph_tool_definition() -> ToolDefinition {
    ToolDefinition {
        name: "code_graph".to_string(),
        description: "Query the code structure graph. Answers structural questions about the codebase without reading entire files. Much cheaper than grep for understanding code relationships.\n\nQuery types:\n- \"defs PATTERN\": Find where symbols (functions, classes, structs, traits, methods) matching PATTERN are defined. Returns file:line for each.\n- \"callers NAME\": Find all functions/methods that call NAME. Traverses call chains.\n- \"callees NAME\": Find all functions/methods called by NAME.\n- \"hierarchies NAME\": Show class/trait/struct inheritance and implementation chains.\n- \"implementations NAME\": Find all implementations of a trait/interface.\n- \"imports NAME\": Find files that import module NAME.\n- \"dead_code\": Find functions/methods that are never called (potential dead code).\n- \"complexity\": List functions by cyclomatic complexity (approximation).\n- \"overview\": Give a high-level summary of the codebase structure.".to_string(),
        parameters: json!({
            "type": "object",
            "properties": {
                "query": {
                    "type": "string",
                    "description": "Query type and argument, e.g. \"defs process_payment\", \"callers main\", \"hierarchies User\", \"dead_code\", \"overview\""
                }
            },
            "required": ["query"]
        }),
    }
}

#[derive(Debug, Deserialize)]
pub struct CodeGraphArgs {
    pub query: String,
}

#[derive(Clone)]
pub struct CodeGraphTool {
    pub permission: Option<PermCheck>,
    pub ask_tx: Option<AskSender>,
}

static INDEX: Mutex<Option<CodeIndex>> = Mutex::new(None);

impl CodeGraphTool {
    pub fn new(permission: Option<PermCheck>, ask_tx: Option<AskSender>) -> Self {
        Self { permission, ask_tx }
    }
}

impl Tool for CodeGraphTool {
    const NAME: &'static str = "code_graph";
    type Error = ToolError;
    type Args = CodeGraphArgs;
    type Output = String;

    async fn definition(&self, _prompt: String) -> ToolDefinition {
        code_graph_tool_definition()
    }

    async fn call(&self, args: CodeGraphArgs) -> Result<String, ToolError> {
        let coaching =
            check_perm(&self.permission, &self.ask_tx, "code_graph", &args.query).await?;

        let result = handle_query(&args.query)?;

        Ok(if let Some(c) = coaching {
            format!("{c}\n\n{result}")
        } else {
            result
        })
    }
}

fn handle_query(query: &str) -> Result<String, ToolError> {
    let mut index = INDEX.lock().unwrap_or_else(|e| e.into_inner());
    if index.is_none() {
        let cwd =
            std::env::current_dir().map_err(|e| ToolError::Msg(format!("Cannot get cwd: {e}")))?;
        *index = Some(build_index(&cwd)?);
    }
    let idx = index.as_ref().unwrap();

    let (query_type, arg) = query.split_once(' ').unwrap_or((query.trim(), ""));
    let arg = arg.trim();

    let result = match query_type {
        "defs" | "def" | "define" | "defines" | "definition" | "definitions" => {
            idx.find_definitions(arg)
        }
        "callers" | "caller" | "who_calls" => idx.find_callers(arg),
        "callees" | "callee" | "calls" | "what_calls" => idx.find_callees(arg),
        "hierarchies" | "hierarchy" | "hier" | "inherits" | "extends" | "impls" => {
            idx.find_hierarchies(arg)
        }
        "implementations" | "impl" | "implementations_of" => idx.find_implementations(arg),
        "imports" | "import" | "who_imports" | "uses" => idx.find_imports(arg),
        "dead_code" | "dead" | "unused" => idx.find_dead_code(),
        "complexity" => idx.find_complexity(),
        "overview" | "summary" | "structure" => idx.overview(),
        _ => Err(ToolError::Msg(format!(
            "Unknown query type: '{query_type}'. Use: defs, callers, callees, hierarchies, implementations, imports, dead_code, complexity, overview"
        ))),
    }?;

    Ok(result)
}

// ─── Index Types ───

#[derive(Debug, Clone)]
struct Symbol {
    name: String,
    kind: SymbolKind,
    file: PathBuf,
    line: u32,
    end_line: u32,
    parent: Option<String>,
    language: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
#[allow(dead_code)]
enum SymbolKind {
    Function,
    Method,
    Class,
    Struct,
    Trait,
    Interface,
    Enum,
    Module,
    Const,
    Macro,
}

impl std::fmt::Display for SymbolKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SymbolKind::Function => write!(f, "fn"),
            SymbolKind::Method => write!(f, "method"),
            SymbolKind::Class => write!(f, "class"),
            SymbolKind::Struct => write!(f, "struct"),
            SymbolKind::Trait => write!(f, "trait"),
            SymbolKind::Interface => write!(f, "interface"),
            SymbolKind::Enum => write!(f, "enum"),
            SymbolKind::Module => write!(f, "module"),
            SymbolKind::Const => write!(f, "const"),
            SymbolKind::Macro => write!(f, "macro"),
        }
    }
}

#[derive(Debug, Clone)]
struct CallEdge {
    caller_file: PathBuf,
    caller_line: u32,
    caller_name: String,
    callee_name: String,
}

#[derive(Debug, Clone)]
struct ImportEdge {
    importer_file: PathBuf,
    imported_name: String,
    line: u32,
}

#[derive(Debug, Clone)]
struct InheritsEdge {
    child: String,
    child_file: PathBuf,
    parent: String,
}

struct CodeIndex {
    symbols: Vec<Symbol>,
    symbols_by_name: HashMap<String, Vec<usize>>,
    calls: Vec<CallEdge>,
    calls_by_callee: HashMap<String, Vec<usize>>,
    calls_by_caller: HashMap<String, Vec<usize>>,
    imports: Vec<ImportEdge>,
    imports_by_name: HashMap<String, Vec<usize>>,
    inherits: Vec<InheritsEdge>,
    inherits_by_child: HashMap<String, Vec<usize>>,
    inherits_by_parent: HashMap<String, Vec<usize>>,
    file_count: usize,
}

const SUPPORTED_EXTENSIONS: &[&str] = &[
    "rs", "py", "js", "ts", "jsx", "tsx", "go", "java", "c", "cpp", "h", "hpp", "rb", "swift",
    "kt", "scala", "cs", "php", "m", "mm",
];

const SKIP_DIRS: &[&str] = &[
    "node_modules",
    "target",
    ".git",
    "vendor",
    "__pycache__",
    ".next",
    "dist",
    "build",
    "out",
    ".venv",
    "venv",
    "env",
    ".tox",
    "bazel-bin",
    "bazel-out",
    ".cargo",
    "Pods",
];

fn build_index(root: &Path) -> Result<CodeIndex, ToolError> {
    let mut symbols = Vec::new();
    let mut calls = Vec::new();
    let mut imports = Vec::new();
    let mut inherits = Vec::new();
    let mut file_count = 0usize;

    let walker = ignore::WalkBuilder::new(root)
        .hidden(true)
        .git_ignore(true)
        .git_global(true)
        .build();

    for entry in walker.flatten() {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let ext = match path.extension().and_then(|e| e.to_str()) {
            Some(e) => e,
            None => continue,
        };
        if !SUPPORTED_EXTENSIONS.contains(&ext) {
            continue;
        }
        if let Some(parent) = path
            .parent()
            .and_then(|p| p.file_name())
            .and_then(|n| n.to_str())
        {
            if SKIP_DIRS.contains(&parent) {
                continue;
            }
        }

        let content = match std::fs::read_to_string(path) {
            Ok(c) => c,
            Err(_) => continue,
        };

        file_count += 1;
        let relative = path.strip_prefix(root).unwrap_or(path).to_path_buf();

        match ext {
            "rs" => parse_rust(
                &content,
                &relative,
                &mut symbols,
                &mut calls,
                &mut imports,
                &mut inherits,
            ),
            "py" => parse_python(
                &content,
                &relative,
                &mut symbols,
                &mut calls,
                &mut imports,
                &mut inherits,
            ),
            "js" | "jsx" | "ts" | "tsx" => parse_js_ts(
                &content,
                &relative,
                ext,
                &mut symbols,
                &mut calls,
                &mut imports,
                &mut inherits,
            ),
            "go" => parse_go(&content, &relative, &mut symbols, &mut calls, &mut imports),
            "java" => parse_java(
                &content,
                &relative,
                &mut symbols,
                &mut calls,
                &mut imports,
                &mut inherits,
            ),
            _ => parse_generic(&content, &relative, ext, &mut symbols),
        }
    }

    let mut symbols_by_name: HashMap<String, Vec<usize>> = HashMap::new();
    for (i, sym) in symbols.iter().enumerate() {
        symbols_by_name
            .entry(sym.name.to_lowercase())
            .or_default()
            .push(i);
        if let Some(parent) = &sym.parent {
            symbols_by_name
                .entry(format!(
                    "{}::{}",
                    parent.to_lowercase(),
                    sym.name.to_lowercase()
                ))
                .or_default()
                .push(i);
        }
    }

    let mut calls_by_callee: HashMap<String, Vec<usize>> = HashMap::new();
    let mut calls_by_caller: HashMap<String, Vec<usize>> = HashMap::new();
    for (i, call) in calls.iter().enumerate() {
        calls_by_callee
            .entry(call.callee_name.to_lowercase())
            .or_default()
            .push(i);
        calls_by_caller
            .entry(call.caller_name.to_lowercase())
            .or_default()
            .push(i);
    }

    let mut imports_by_name: HashMap<String, Vec<usize>> = HashMap::new();
    for (i, imp) in imports.iter().enumerate() {
        imports_by_name
            .entry(imp.imported_name.to_lowercase())
            .or_default()
            .push(i);
    }

    let mut inherits_by_child: HashMap<String, Vec<usize>> = HashMap::new();
    let mut inherits_by_parent: HashMap<String, Vec<usize>> = HashMap::new();
    for (i, inh) in inherits.iter().enumerate() {
        inherits_by_child
            .entry(inh.child.to_lowercase())
            .or_default()
            .push(i);
        inherits_by_parent
            .entry(inh.parent.to_lowercase())
            .or_default()
            .push(i);
    }

    Ok(CodeIndex {
        symbols,
        symbols_by_name,
        calls,
        calls_by_callee,
        calls_by_caller,
        imports,
        imports_by_name,
        inherits,
        inherits_by_child,
        inherits_by_parent,
        file_count,
    })
}

impl CodeIndex {
    fn find_definitions(&self, pattern: &str) -> Result<String, ToolError> {
        let pat = pattern.to_lowercase();
        let mut results: Vec<&Symbol> = Vec::new();

        if let Some(indices) = self.symbols_by_name.get(&pat) {
            for &i in indices {
                results.push(&self.symbols[i]);
            }
        }

        if results.is_empty() {
            let pat_prefix = pat.trim_end_matches(|c: char| c == '*');
            for sym in &self.symbols {
                if sym.name.to_lowercase().contains(pat_prefix) {
                    results.push(sym);
                }
            }
        }

        if results.is_empty() {
            return Ok(format!("No definitions found for '{pattern}'."));
        }

        results.sort_by_key(|s| (s.kind, s.file.clone(), s.line));
        let mut output = format!("Definitions of '{}':\n", pattern);
        for sym in results.iter().take(30) {
            output.push_str(&format!(
                "  {} {} at {}:{}\n",
                sym.kind,
                sym.name,
                sym.file.display(),
                sym.line
            ));
            if let Some(parent) = &sym.parent {
                output.push_str(&format!("    (in {})\n", parent));
            }
        }
        if results.len() > 30 {
            output.push_str(&format!("  ... and {} more\n", results.len() - 30));
        }
        Ok(output)
    }

    fn find_callers(&self, name: &str) -> Result<String, ToolError> {
        let pat = name.to_lowercase();
        let indices = match self.calls_by_callee.get(&pat) {
            Some(i) => i,
            None => {
                return Ok(format!(
                    "No callers found for '{name}'. Try `defs {name}` to see if it exists."
                ));
            }
        };

        let mut output = format!("Callers of '{}':\n", name);
        for &i in indices.iter().take(30) {
            let call = &self.calls[i];
            output.push_str(&format!(
                "  {} at {}:{} calls {}\n",
                call.caller_name,
                call.caller_file.display(),
                call.caller_line,
                call.callee_name
            ));
        }
        if indices.len() > 30 {
            output.push_str(&format!("  ... and {} more callers\n", indices.len() - 30));
        }
        Ok(output)
    }

    fn find_callees(&self, name: &str) -> Result<String, ToolError> {
        let pat = name.to_lowercase();
        let indices = match self.calls_by_caller.get(&pat) {
            Some(i) => i,
            None => {
                if let Some(sym_indices) = self.symbols_by_name.get(&pat) {
                    if sym_indices.is_empty() {
                        return Ok(format!("No callees found for '{name}'."));
                    }
                    sym_indices
                } else {
                    return Ok(format!(
                        "No callees found for '{name}'. Try `defs {name}` to see if it exists."
                    ));
                }
            }
        };

        let mut output = format!("Callees of '{}':\n", name);
        for &i in indices.iter().take(30) {
            let call = &self.calls[i];
            output.push_str(&format!(
                "  {} calls {} at {}:{}\n",
                call.caller_name,
                call.callee_name,
                call.caller_file.display(),
                call.caller_line
            ));
        }
        if indices.len() > 30 {
            output.push_str(&format!("  ... and {} more callees\n", indices.len() - 30));
        }
        Ok(output)
    }

    fn find_hierarchies(&self, name: &str) -> Result<String, ToolError> {
        let pat = name.to_lowercase();
        let mut output = format!("Hierarchy for '{}':\n", name);

        let mut found = false;

        if let Some(indices) = self.inherits_by_child.get(&pat) {
            found = true;
            for &i in indices {
                let inh = &self.inherits[i];
                output.push_str(&format!(
                    "  {} extends/implements {} (at {})\n",
                    inh.child,
                    inh.parent,
                    inh.child_file.display()
                ));
            }
        }

        if let Some(indices) = self.inherits_by_parent.get(&pat) {
            found = true;
            output.push_str(&format!("  Subtypes of {}:\n", name));
            for &i in indices {
                let inh = &self.inherits[i];
                output.push_str(&format!(
                    "    {} (at {})\n",
                    inh.child,
                    inh.child_file.display()
                ));
            }
        }

        if !found {
            output.push_str(
                "  No inheritance relationships found. Try `defs` to see if the symbol exists.",
            );
        }

        Ok(output)
    }

    fn find_implementations(&self, name: &str) -> Result<String, ToolError> {
        self.find_hierarchies(name)
    }

    fn find_imports(&self, name: &str) -> Result<String, ToolError> {
        let pat = name.to_lowercase();
        let indices = match self.imports_by_name.get(&pat) {
            Some(i) => i,
            None => return Ok(format!("No imports found for '{name}'.")),
        };

        let mut output = format!("Importers of '{}':\n", name);
        for &i in indices.iter().take(30) {
            let imp = &self.imports[i];
            output.push_str(&format!(
                "  {} imports {} at line {}\n",
                imp.importer_file.display(),
                imp.imported_name,
                imp.line
            ));
        }
        if indices.len() > 30 {
            output.push_str(&format!("  ... and {} more\n", indices.len() - 30));
        }
        Ok(output)
    }

    fn find_dead_code(&self) -> Result<String, ToolError> {
        let called_names: std::collections::HashSet<String> = self
            .calls
            .iter()
            .map(|c| c.callee_name.to_lowercase())
            .collect();

        let mut dead: Vec<&Symbol> = self
            .symbols
            .iter()
            .filter(|s| {
                matches!(s.kind, SymbolKind::Function | SymbolKind::Method)
                    && !called_names.contains(&s.name.to_lowercase())
                    && !s.name.starts_with("test_")
                    && !s.name.starts_with("Test")
                    && !s.name.starts_with("main")
                    && !s.name.contains("new")
                    && !s.name.starts_with("drop")
                    && !s.name.starts_with("from_")
                    && !s.name.starts_with("into_")
                    && !s.name.starts_with("try_from")
            })
            .collect();

        dead.sort_by_key(|s| (s.file.clone(), s.line));

        if dead.is_empty() {
            return Ok(
                "No obviously dead code found (all functions appear to be called).".to_string(),
            );
        }

        let mut output = format!(
            "Potentially dead code ({} functions/methods never called):\n",
            dead.len()
        );
        for sym in dead.iter().take(30) {
            output.push_str(&format!(
                "  {} {} at {}:{}\n",
                sym.kind,
                sym.name,
                sym.file.display(),
                sym.line
            ));
        }
        if dead.len() > 30 {
            output.push_str(&format!("  ... and {} more\n", dead.len() - 30));
        }
        output.push_str("\nNote: This is a heuristic. Some 'dead' functions may be called dynamically or exported.");
        Ok(output)
    }

    fn find_complexity(&self) -> Result<String, ToolError> {
        let mut fns: Vec<&Symbol> = self
            .symbols
            .iter()
            .filter(|s| matches!(s.kind, SymbolKind::Function | SymbolKind::Method))
            .collect();

        fns.sort_by(|a, b| {
            (b.end_line.saturating_sub(b.line)).cmp(&(a.end_line.saturating_sub(a.line)))
        });

        let mut output = "Functions by size (approximate complexity heuristic):\n".to_string();
        for sym in fns.iter().take(20) {
            let lines = sym.end_line.saturating_sub(sym.line);
            output.push_str(&format!(
                "  {} {} ({} lines) at {}:{}\n",
                sym.kind,
                sym.name,
                lines,
                sym.file.display(),
                sym.line
            ));
        }
        Ok(output)
    }

    fn overview(&self) -> Result<String, ToolError> {
        let mut by_kind: HashMap<String, usize> = HashMap::new();
        for sym in &self.symbols {
            *by_kind.entry(sym.kind.to_string()).or_insert(0) += 1;
        }

        let mut by_lang: HashMap<String, usize> = HashMap::new();
        for sym in &self.symbols {
            *by_lang.entry(sym.language.clone()).or_insert(0) += 1;
        }

        let mut output = format!(
            "Codebase overview ({} files indexed):\n\nSymbols:\n",
            self.file_count
        );
        let mut kinds: Vec<_> = by_kind.iter().collect();
        kinds.sort_by(|a, b| b.1.cmp(a.1));
        for (kind, count) in &kinds {
            output.push_str(&format!("  {}: {}\n", kind, count));
        }

        output.push_str("\nBy language:\n");
        let mut langs: Vec<_> = by_lang.iter().collect();
        langs.sort_by(|a, b| b.1.cmp(a.1));
        for (lang, count) in &langs {
            output.push_str(&format!("  {}: {} symbols\n", lang, count));
        }

        output.push_str(&format!(
            "\nCall edges: {}\nImport edges: {}\nInheritance edges: {}",
            self.calls.len(),
            self.imports.len(),
            self.inherits.len()
        ));

        Ok(output)
    }
}

// ─── Language Parsers ───

fn parse_rust(
    content: &str,
    path: &Path,
    symbols: &mut Vec<Symbol>,
    calls: &mut Vec<CallEdge>,
    imports: &mut Vec<ImportEdge>,
    inherits: &mut Vec<InheritsEdge>,
) {
    let mut current_struct: Option<String> = None;
    let mut current_impl_for: Option<String> = None;

    for (i, line) in content.lines().enumerate() {
        let ln = (i + 1) as u32;
        let trimmed = line.trim();

        if trimmed.starts_with("//") || trimmed.starts_with("#[") || trimmed.starts_with("#[") {
            continue;
        }

        if let Some(name) = extract_rust_fn_name(trimmed) {
            let parent = current_impl_for
                .as_ref()
                .or(current_struct.as_ref())
                .cloned();
            let sym = Symbol {
                name: name.to_string(),
                kind: if parent.is_some() {
                    SymbolKind::Method
                } else {
                    SymbolKind::Function
                },
                file: path.to_path_buf(),
                line: ln,
                end_line: ln,
                parent: parent.clone(),
                language: "rust".to_string(),
            };

            if let Some(parent_name) = &parent {
                calls.push(CallEdge {
                    caller_file: path.to_path_buf(),
                    caller_line: ln,
                    caller_name: format!("{}::{}", parent_name, name),
                    callee_name: name.to_string(),
                });
            }

            calls.extend(extract_rust_calls(
                trimmed,
                path,
                ln,
                &format!(
                    "{}::{}",
                    current_impl_for
                        .as_deref()
                        .or(current_struct.as_deref())
                        .unwrap_or(name),
                    name
                ),
            ));

            symbols.push(sym);
            continue;
        }

        if let Some(name) = extract_rust_struct_name(trimmed) {
            current_struct = Some(name.to_string());
            symbols.push(Symbol {
                name: name.to_string(),
                kind: SymbolKind::Struct,
                file: path.to_path_buf(),
                line: ln,
                end_line: ln,
                parent: None,
                language: "rust".to_string(),
            });
            continue;
        }

        if let Some(name) = extract_rust_trait_name(trimmed) {
            symbols.push(Symbol {
                name: name.to_string(),
                kind: SymbolKind::Trait,
                file: path.to_path_buf(),
                line: ln,
                end_line: ln,
                parent: None,
                language: "rust".to_string(),
            });
            continue;
        }

        if let Some(name) = extract_rust_enum_name(trimmed) {
            symbols.push(Symbol {
                name: name.to_string(),
                kind: SymbolKind::Enum,
                file: path.to_path_buf(),
                line: ln,
                end_line: ln,
                parent: None,
                language: "rust".to_string(),
            });
            continue;
        }

        if trimmed.starts_with("impl ") {
            if let Some(for_name) = extract_rust_impl_for(trimmed) {
                current_impl_for = Some(for_name.to_string());
                if trimmed.contains(" for ") {
                    if let Some(trait_name) = extract_rust_impl_trait(trimmed) {
                        inherits.push(InheritsEdge {
                            child: for_name.to_string(),
                            child_file: path.to_path_buf(),
                            parent: trait_name,
                        });
                    }
                }
            }
        }

        if trimmed == "}" && current_impl_for.is_some() {
            current_impl_for = None;
        }

        if trimmed.starts_with("use ") {
            if let Some(import_name) = extract_rust_use(trimmed) {
                imports.push(ImportEdge {
                    importer_file: path.to_path_buf(),
                    imported_name: import_name,
                    line: ln,
                });
            }
        }
    }
}

fn extract_rust_fn_name(line: &str) -> Option<&str> {
    let line = line
        .trim_start_matches("pub ")
        .trim_start_matches("pub(crate) ")
        .trim_start_matches("pub(super) ")
        .trim_start_matches("async ")
        .trim_start_matches("const ")
        .trim_start_matches("unsafe ");
    if !line.starts_with("fn ") {
        return None;
    }
    let after_fn = &line[3..];
    let paren = after_fn.find('(')?;
    let name = &after_fn[..paren];
    if name.contains(|c: char| c == '<' || c == '>') {
        return None;
    }
    Some(name.trim())
}

fn extract_rust_struct_name(line: &str) -> Option<&str> {
    let line = line
        .trim_start_matches("pub ")
        .trim_start_matches("pub(crate) ");
    if !line.starts_with("struct ") {
        return None;
    }
    let after = &line[7..];
    let end = after
        .find(|c: char| c == '<' || c == '{' || c == '(' || c == ';')
        .unwrap_or(after.len());
    Some(after[..end].trim())
}

fn extract_rust_trait_name(line: &str) -> Option<&str> {
    let line = line
        .trim_start_matches("pub ")
        .trim_start_matches("pub(crate) ");
    if !line.starts_with("trait ") {
        return None;
    }
    let after = &line[6..];
    let end = after
        .find(|c: char| c == '<' || c == '{' || c == ':' || c == ';')
        .unwrap_or(after.len());
    Some(after[..end].trim())
}

fn extract_rust_enum_name(line: &str) -> Option<&str> {
    let line = line
        .trim_start_matches("pub ")
        .trim_start_matches("pub(crate) ");
    if !line.starts_with("enum ") {
        return None;
    }
    let after = &line[5..];
    let end = after
        .find(|c: char| c == '<' || c == '{' || c == ';')
        .unwrap_or(after.len());
    Some(after[..end].trim())
}

fn extract_rust_impl_for(line: &str) -> Option<&str> {
    let after = &line[5..];
    if let Some(pos) = after.find(" for ") {
        let after_for = &after[pos + 5..];
        let end = after_for
            .find(|c: char| c == '<' || c == '{' || c == ' ')
            .unwrap_or(after_for.len());
        Some(after_for[..end].trim())
    } else {
        let end = after
            .find(|c: char| c == '<' || c == '{')
            .unwrap_or(after.len());
        Some(after[..end].trim())
    }
}

fn extract_rust_impl_trait(line: &str) -> Option<String> {
    let after = &line[5..];
    let for_pos = after.find(" for ")?;
    let before_for = &after[..for_pos];
    let trait_name = before_for.trim().trim_start_matches("impl").trim();
    Some(trait_name.to_string())
}

fn extract_rust_use(line: &str) -> Option<String> {
    let after = line.strip_prefix("use ")?;
    let content = after.trim_end_matches(';').trim();
    if let Some(pos) = content.find("::") {
        Some(content[..pos].to_string())
    } else {
        Some(content.to_string())
    }
}

fn extract_rust_calls(line: &str, path: &Path, line_num: u32, caller_name: &str) -> Vec<CallEdge> {
    let mut result = Vec::new();
    let re = regex::Regex::new(r"([a-z_][a-zA-Z0-9_]*)\s*\(")
        .unwrap_or_else(|_| regex::Regex::new(r".").unwrap());
    for cap in re.captures_iter(line) {
        if let Some(m) = cap.get(1) {
            let name = m.as_str();
            if !matches!(
                name,
                "if" | "while"
                    | "for"
                    | "match"
                    | "let"
                    | "return"
                    | "self"
                    | "pub"
                    | "fn"
                    | "struct"
                    | "enum"
                    | "impl"
                    | "trait"
                    | "mod"
                    | "use"
                    | "type"
                    | "where"
                    | "as"
                    | "mut"
                    | "ref"
                    | "move"
                    | "async"
                    | "await"
                    | "unsafe"
                    | "extern"
                    | "crate"
                    | "super"
                    | "true"
                    | "false"
            ) {
                result.push(CallEdge {
                    caller_file: path.to_path_buf(),
                    caller_line: line_num,
                    caller_name: caller_name.to_string(),
                    callee_name: name.to_string(),
                });
            }
        }
    }
    result
}

fn parse_python(
    content: &str,
    path: &Path,
    symbols: &mut Vec<Symbol>,
    calls: &mut Vec<CallEdge>,
    imports: &mut Vec<ImportEdge>,
    inherits: &mut Vec<InheritsEdge>,
) {
    let mut current_class: Option<String> = None;

    for (i, line) in content.lines().enumerate() {
        let ln = (i + 1) as u32;
        let trimmed = line.trim();

        if trimmed.starts_with('#') || trimmed.is_empty() {
            continue;
        }

        if let Some(name) = extract_python_def_name(trimmed) {
            if !line.starts_with(' ') && !line.starts_with('\t') {
                current_class = None;
            }
            let parent = current_class.clone();
            let caller_name = parent
                .as_ref()
                .map(|c| format!("{}.{}", c, name))
                .unwrap_or_else(|| name.to_string());
            symbols.push(Symbol {
                name: name.to_string(),
                kind: if parent.is_some() {
                    SymbolKind::Method
                } else {
                    SymbolKind::Function
                },
                file: path.to_path_buf(),
                line: ln,
                end_line: ln,
                parent: parent.clone(),
                language: "python".to_string(),
            });
            extract_python_calls(trimmed, path, ln, &caller_name, calls);
        }

        if let Some(name) = extract_python_class_name(trimmed) {
            if let Some(parents) = extract_python_inherits(trimmed) {
                for parent_name in parents {
                    inherits.push(InheritsEdge {
                        child: name.to_string(),
                        child_file: path.to_path_buf(),
                        parent: parent_name,
                    });
                }
            }
            current_class = Some(name.to_string());
            symbols.push(Symbol {
                name: name.to_string(),
                kind: SymbolKind::Class,
                file: path.to_path_buf(),
                line: ln,
                end_line: ln,
                parent: None,
                language: "python".to_string(),
            });
        }

        if trimmed.starts_with("import ") || trimmed.starts_with("from ") {
            if let Some(import_name) = extract_python_import(trimmed) {
                imports.push(ImportEdge {
                    importer_file: path.to_path_buf(),
                    imported_name: import_name,
                    line: ln,
                });
            }
        }

        if !line.starts_with(' ')
            && !line.starts_with('\t')
            && !trimmed.starts_with('#')
            && !trimmed.starts_with("def ")
            && !trimmed.starts_with("class ")
            && !trimmed.starts_with("import ")
            && !trimmed.starts_with("from ")
        {
            current_class = None;
        }
    }
}

fn extract_python_def_name(line: &str) -> Option<&str> {
    let after = line.strip_prefix("def ")?;
    let paren = after.find('(')?;
    Some(after[..paren].trim())
}

fn extract_python_class_name(line: &str) -> Option<&str> {
    let after = line.strip_prefix("class ")?;
    let end = after
        .find(|c: char| c == '(' || c == ':' || c == '[')
        .unwrap_or(after.len());
    Some(after[..end].trim())
}

fn extract_python_inherits(line: &str) -> Option<Vec<String>> {
    let paren_start = line.find('(')?;
    let paren_end = line.rfind(')')?;
    if paren_start >= paren_end {
        return None;
    }
    let parents = &line[paren_start + 1..paren_end];
    Some(
        parents
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect(),
    )
}

fn extract_python_import(line: &str) -> Option<String> {
    if line.starts_with("import ") {
        let after = &line[7..];
        let name = after.split(',').next()?.trim();
        Some(name.split('.').next()?.to_string())
    } else if line.starts_with("from ") {
        let after = &line[5..];
        let import_pos = after.find(" import ")?;
        Some(after[..import_pos].split('.').next()?.to_string())
    } else {
        None
    }
}

fn extract_python_calls(
    line: &str,
    path: &Path,
    line_num: u32,
    caller_name: &str,
    calls: &mut Vec<CallEdge>,
) {
    let re = regex::Regex::new(r"([a-zA-Z_][a-zA-Z0-9_]*)\s*\(")
        .unwrap_or_else(|_| regex::Regex::new(r".").unwrap());
    for cap in re.captures_iter(line) {
        if let Some(m) = cap.get(1) {
            let name = m.as_str();
            if !matches!(
                name,
                "if" | "while"
                    | "for"
                    | "with"
                    | "as"
                    | "not"
                    | "and"
                    | "or"
                    | "in"
                    | "is"
                    | "def"
                    | "class"
                    | "return"
                    | "yield"
                    | "raise"
                    | "assert"
                    | "lambda"
                    | "from"
                    | "import"
                    | "try"
                    | "except"
                    | "finally"
                    | "else"
                    | "elif"
                    | "pass"
                    | "break"
                    | "continue"
                    | "global"
                    | "nonlocal"
                    | "del"
                    | "print"
                    | "self"
                    | "cls"
                    | "super"
                    | "True"
                    | "False"
                    | "None"
            ) {
                calls.push(CallEdge {
                    caller_file: path.to_path_buf(),
                    caller_line: line_num,
                    caller_name: caller_name.to_string(),
                    callee_name: name.to_string(),
                });
            }
        }
    }
}

fn parse_js_ts(
    content: &str,
    path: &Path,
    _ext: &str,
    symbols: &mut Vec<Symbol>,
    _calls: &mut Vec<CallEdge>,
    imports: &mut Vec<ImportEdge>,
    inherits: &mut Vec<InheritsEdge>,
) {
    for (i, line) in content.lines().enumerate() {
        let ln = (i + 1) as u32;
        let trimmed = line.trim();
        if trimmed.starts_with("//") || trimmed.is_empty() {
            continue;
        }

        if let Some(name) = extract_js_fn_name(trimmed) {
            symbols.push(Symbol {
                name: name.to_string(),
                kind: SymbolKind::Function,
                file: path.to_path_buf(),
                line: ln,
                end_line: ln,
                parent: None,
                language: "javascript".to_string(),
            });
        }

        if let Some(name) = extract_js_class_name(trimmed) {
            symbols.push(Symbol {
                name: name.to_string(),
                kind: SymbolKind::Class,
                file: path.to_path_buf(),
                line: ln,
                end_line: ln,
                parent: None,
                language: "javascript".to_string(),
            });

            if let Some(parent) = extract_js_extends(trimmed) {
                inherits.push(InheritsEdge {
                    child: name.to_string(),
                    child_file: path.to_path_buf(),
                    parent,
                });
            }
        }

        if trimmed.starts_with("import ")
            || trimmed.starts_with("const ") && trimmed.contains("require(")
        {
            if let Some(import_name) = extract_js_import(trimmed) {
                imports.push(ImportEdge {
                    importer_file: path.to_path_buf(),
                    imported_name: import_name,
                    line: ln,
                });
            }
        }
    }
}

fn extract_js_fn_name(line: &str) -> Option<&str> {
    if line.starts_with("function ") {
        let after = &line[9..];
        let end = after
            .find(|c: char| c == '(' || c == '{')
            .unwrap_or(after.len());
        Some(after[..end].trim())
    } else if line.starts_with("async function ") {
        let after = &line[15..];
        let end = after
            .find(|c: char| c == '(' || c == '{')
            .unwrap_or(after.len());
        Some(after[..end].trim())
    } else if let Some(rest) = line.strip_prefix("const ") {
        if let Some(eq_pos) = rest.find("= ") {
            let name = rest[..eq_pos].trim();
            if rest[eq_pos + 2..].starts_with('(')
                || rest[eq_pos + 2..].starts_with("async")
                || rest[eq_pos + 2..].contains("=>")
            {
                return Some(name);
            }
        }
        None
    } else {
        None
    }
}

fn extract_js_class_name(line: &str) -> Option<&str> {
    let line = line
        .trim_start_matches("export ")
        .trim_start_matches("default ");
    if !line.starts_with("class ") {
        return None;
    }
    let after = &line[6..];
    let end = after
        .find(|c: char| c == '{' || c == ' ')
        .unwrap_or(after.len());
    Some(after[..end].trim())
}

fn extract_js_extends(line: &str) -> Option<String> {
    let pos = line.find("extends ")?;
    let after = &line[pos + 8..];
    let end = after
        .find(|c: char| c == '{' || c == ' ')
        .unwrap_or(after.len());
    Some(after[..end].trim().to_string())
}

fn extract_js_import(line: &str) -> Option<String> {
    if line.starts_with("import ") {
        let after = &line[7..];
        if after.starts_with("{") || after.starts_with("*") {
            return None;
        }
        let end = after
            .find(|c: char| c == ' ' || c == ',' || c == ';')
            .unwrap_or(after.len());
        Some(after[..end].trim().to_string())
    } else {
        None
    }
}

fn parse_go(
    content: &str,
    path: &Path,
    symbols: &mut Vec<Symbol>,
    _calls: &mut Vec<CallEdge>,
    imports: &mut Vec<ImportEdge>,
) {
    let current_receiver: Option<String> = None;

    for (i, line) in content.lines().enumerate() {
        let ln = (i + 1) as u32;
        let trimmed = line.trim();

        if trimmed.starts_with("//") || trimmed.is_empty() {
            continue;
        }

        if let Some(name) = extract_go_func_name(trimmed) {
            symbols.push(Symbol {
                name: name.to_string(),
                kind: if current_receiver.is_some() {
                    SymbolKind::Method
                } else {
                    SymbolKind::Function
                },
                file: path.to_path_buf(),
                line: ln,
                end_line: ln,
                parent: current_receiver.clone(),
                language: "go".to_string(),
            });
        }

        if let Some(name) = extract_go_struct_interface(trimmed, "struct") {
            symbols.push(Symbol {
                name: name.to_string(),
                kind: SymbolKind::Struct,
                file: path.to_path_buf(),
                line: ln,
                end_line: ln,
                parent: None,
                language: "go".to_string(),
            });
        }

        if let Some(name) = extract_go_struct_interface(trimmed, "interface") {
            symbols.push(Symbol {
                name: name.to_string(),
                kind: SymbolKind::Interface,
                file: path.to_path_buf(),
                line: ln,
                end_line: ln,
                parent: None,
                language: "go".to_string(),
            });
        }

        if trimmed.starts_with("import") {
            if let Some(import_name) = extract_go_import(trimmed) {
                imports.push(ImportEdge {
                    importer_file: path.to_path_buf(),
                    imported_name: import_name,
                    line: ln,
                });
            }
        }
    }
}

fn extract_go_func_name(line: &str) -> Option<&str> {
    let after = line.strip_prefix("func ")?;
    let paren = after.find('(')?;
    Some(after[..paren].trim())
}

fn extract_go_struct_interface<'a>(line: &'a str, kind: &str) -> Option<&'a str> {
    if !line.starts_with("type ") {
        return None;
    }
    let after = &line[5..];
    if kind == "struct" && after.contains(" struct") {
        let end = after.find(" struct")?;
        Some(after[..end].trim())
    } else if kind == "interface" && after.contains(" interface") {
        let end = after.find(" interface")?;
        Some(after[..end].trim())
    } else {
        None
    }
}

fn extract_go_import(line: &str) -> Option<String> {
    if line.starts_with("import \"") {
        let start = 8;
        let end = line.rfind('"')?;
        if end > start {
            let full = &line[start..end];
            Some(full.rsplit('/').next()?.to_string())
        } else {
            None
        }
    } else {
        None
    }
}

fn parse_java(
    content: &str,
    path: &Path,
    symbols: &mut Vec<Symbol>,
    _calls: &mut Vec<CallEdge>,
    imports: &mut Vec<ImportEdge>,
    inherits: &mut Vec<InheritsEdge>,
) {
    for (i, line) in content.lines().enumerate() {
        let ln = (i + 1) as u32;
        let trimmed = line.trim();

        if trimmed.starts_with("//")
            || trimmed.starts_with("/*")
            || trimmed.starts_with("*")
            || trimmed.is_empty()
        {
            continue;
        }

        if let Some(name) = extract_java_method_name(trimmed) {
            symbols.push(Symbol {
                name: name.to_string(),
                kind: SymbolKind::Method,
                file: path.to_path_buf(),
                line: ln,
                end_line: ln,
                parent: None,
                language: "java".to_string(),
            });
        }

        if let Some(name) = extract_java_class_name(trimmed) {
            symbols.push(Symbol {
                name: name.to_string(),
                kind: SymbolKind::Class,
                file: path.to_path_buf(),
                line: ln,
                end_line: ln,
                parent: None,
                language: "java".to_string(),
            });

            if let Some(parent) = extract_java_extends(trimmed) {
                inherits.push(InheritsEdge {
                    child: name.to_string(),
                    child_file: path.to_path_buf(),
                    parent,
                });
            }

            if let Some(iface) = extract_java_implements(trimmed) {
                for iface_name in iface {
                    inherits.push(InheritsEdge {
                        child: name.to_string(),
                        child_file: path.to_path_buf(),
                        parent: iface_name,
                    });
                }
            }
        }

        if trimmed.starts_with("import ") {
            if let Some(import_name) = extract_java_import(trimmed) {
                imports.push(ImportEdge {
                    importer_file: path.to_path_buf(),
                    imported_name: import_name,
                    line: ln,
                });
            }
        }
    }
}

fn extract_java_method_name(line: &str) -> Option<&str> {
    if !line.contains('(') {
        return None;
    }
    let re = regex::Regex::new(r"(?:public|private|protected|static|final|synchronized|abstract)\s+.*\s+([a-zA-Z_][a-zA-Z0-9_]*)\s*\(").ok()?;
    re.captures(line)
        .and_then(|cap| cap.get(1).map(|m| m.as_str()))
}

fn extract_java_class_name(line: &str) -> Option<&str> {
    if !line.contains("class ") {
        return None;
    }
    let pos = line.find("class ")?;
    let after = &line[pos + 6..];
    let end = after
        .find(|c: char| c == '{' || c == ' ' || c == '<')
        .unwrap_or(after.len());
    Some(after[..end].trim())
}

fn extract_java_extends(line: &str) -> Option<String> {
    let pos = line.find(" extends ")?;
    let after = &line[pos + 9..];
    let end = after
        .find(|c: char| c == '{' || c == ' ')
        .unwrap_or(after.len());
    Some(after[..end].trim().to_string())
}

fn extract_java_implements(line: &str) -> Option<Vec<String>> {
    let pos = line.find(" implements ")?;
    let after = &line[pos + 12..];
    let end = after.find('{').unwrap_or(after.len());
    Some(
        after[..end]
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect(),
    )
}

fn extract_java_import(line: &str) -> Option<String> {
    let after = line.strip_prefix("import ")?;
    let content = after.trim_end_matches(';').trim();
    if content.starts_with("static ") {
        return None;
    }
    Some(content.rsplit('.').next()?.to_string())
}

fn parse_generic(content: &str, path: &Path, ext: &str, symbols: &mut Vec<Symbol>) {
    let re_fn =
        regex::Regex::new(r"(?:function|def|fn|func|proc|sub)\s+([a-zA-Z_][a-zA-Z0-9_]*)\s*[\(<{]")
            .ok();
    let re_class = regex::Regex::new(
        r"(?:class|struct|interface|enum|trait|type)\s+([a-zA-Z_][a-zA-Z0-9_]*)\s*[{<(]",
    )
    .ok();

    for (i, line) in content.lines().enumerate() {
        let ln = (i + 1) as u32;
        let trimmed = line.trim();
        if trimmed.starts_with('#')
            || trimmed.starts_with("//")
            || trimmed.starts_with("/*")
            || trimmed.is_empty()
        {
            continue;
        }

        if let Some(ref re) = re_fn {
            if let Some(cap) = re.captures(trimmed) {
                if let Some(m) = cap.get(1) {
                    symbols.push(Symbol {
                        name: m.as_str().to_string(),
                        kind: SymbolKind::Function,
                        file: path.to_path_buf(),
                        line: ln,
                        end_line: ln,
                        parent: None,
                        language: ext.to_string(),
                    });
                }
            }
        }

        if let Some(ref re) = re_class {
            if let Some(cap) = re.captures(trimmed) {
                if let Some(m) = cap.get(1) {
                    let kind = if trimmed.contains("struct") {
                        SymbolKind::Struct
                    } else if trimmed.contains("interface") || trimmed.contains("trait") {
                        SymbolKind::Interface
                    } else if trimmed.contains("enum") {
                        SymbolKind::Enum
                    } else {
                        SymbolKind::Class
                    };
                    symbols.push(Symbol {
                        name: m.as_str().to_string(),
                        kind,
                        file: path.to_path_buf(),
                        line: ln,
                        end_line: ln,
                        parent: None,
                        language: ext.to_string(),
                    });
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn build_test_index(content: &str, ext: &str) -> CodeIndex {
        let _root = PathBuf::from("/test");
        let path = PathBuf::from(format!("test.{}", ext));
        let mut symbols = Vec::new();
        let mut calls = Vec::new();
        let mut imports = Vec::new();
        let mut inherits = Vec::new();

        match ext {
            "rs" => parse_rust(
                content,
                &path,
                &mut symbols,
                &mut calls,
                &mut imports,
                &mut inherits,
            ),
            "py" => parse_python(
                content,
                &path,
                &mut symbols,
                &mut calls,
                &mut imports,
                &mut inherits,
            ),
            "js" | "ts" => parse_js_ts(
                content,
                &path,
                ext,
                &mut symbols,
                &mut calls,
                &mut imports,
                &mut inherits,
            ),
            "go" => parse_go(content, &path, &mut symbols, &mut calls, &mut imports),
            "java" => parse_java(
                content,
                &path,
                &mut symbols,
                &mut calls,
                &mut imports,
                &mut inherits,
            ),
            _ => parse_generic(content, &path, ext, &mut symbols),
        }

        let mut symbols_by_name: HashMap<String, Vec<usize>> = HashMap::new();
        for (i, sym) in symbols.iter().enumerate() {
            symbols_by_name
                .entry(sym.name.to_lowercase())
                .or_default()
                .push(i);
        }

        CodeIndex {
            symbols,
            symbols_by_name,
            calls,
            calls_by_callee: HashMap::new(),
            calls_by_caller: HashMap::new(),
            imports,
            imports_by_name: HashMap::new(),
            inherits,
            inherits_by_child: HashMap::new(),
            inherits_by_parent: HashMap::new(),
            file_count: 1,
        }
    }

    #[test]
    fn test_rust_fn_extraction() {
        let code = r#"
fn main() {
    let x = foo();
}

fn foo() -> i32 {
    42
}
"#;
        let idx = build_test_index(code, "rs");
        assert!(
            idx.symbols
                .iter()
                .any(|s| s.name == "main" && s.kind == SymbolKind::Function)
        );
        assert!(
            idx.symbols
                .iter()
                .any(|s| s.name == "foo" && s.kind == SymbolKind::Function)
        );
    }

    #[test]
    fn test_rust_struct_extraction() {
        let code = r#"
struct User {
    name: String,
    age: u32,
}
"#;
        let idx = build_test_index(code, "rs");
        assert!(
            idx.symbols
                .iter()
                .any(|s| s.name == "User" && s.kind == SymbolKind::Struct)
        );
    }

    #[test]
    fn test_rust_impl_trait() {
        let code = r#"
trait Animal {
    fn speak(&self) -> String;
}

struct Dog;
impl Animal for Dog {
    fn speak(&self) -> String {
        "Woof".to_string()
    }
}
"#;
        let idx = build_test_index(code, "rs");
        assert!(
            idx.inherits
                .iter()
                .any(|i| i.child == "Dog" && i.parent == "Animal")
        );
    }

    #[test]
    fn test_python_class_extraction() {
        let code = r#"
class User(BaseModel):
    name: str
    age: int

def get_user(user_id: int) -> User:
    pass
"#;
        let idx = build_test_index(code, "py");
        assert!(
            idx.symbols
                .iter()
                .any(|s| s.name == "User" && s.kind == SymbolKind::Class)
        );
        assert!(
            idx.symbols
                .iter()
                .any(|s| s.name == "get_user" && s.kind == SymbolKind::Function)
        );
        assert!(
            idx.inherits
                .iter()
                .any(|i| i.child == "User" && i.parent == "BaseModel")
        );
    }

    #[test]
    fn test_js_class_extraction() {
        let code = r#"
class App extends Component {
    render() {}
}

function handleClick() {}
"#;
        let idx = build_test_index(code, "js");
        assert!(
            idx.symbols
                .iter()
                .any(|s| s.name == "App" && s.kind == SymbolKind::Class)
        );
        assert!(
            idx.inherits
                .iter()
                .any(|i| i.child == "App" && i.parent == "Component")
        );
        assert!(
            idx.symbols
                .iter()
                .any(|s| s.name == "handleClick" && s.kind == SymbolKind::Function)
        );
    }
}
