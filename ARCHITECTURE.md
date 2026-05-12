# Architecture — nehme-harness v0.1.0-beta

Minimal coding agent in Rust, optimized for token efficiency and memory footprint.
Single crate, no workspace. All source under `src/`. Binary: `nh`.

## Directory Layout

| Path | Responsibility |
|---|---|
| `src/main.rs` | Entry point, CLI dispatch, mode routing |
| `src/cli.rs` | `clap::Parser` CLI argument definition |
| `src/provider.rs` | LLM provider abstraction (type-erased: `AnyClient`, `AnyModel`, `AnyAgent` enums) |
| `src/auth.rs` | API key resolution (`AuthResolver`, `ProviderKind` enum) |
| `src/event.rs` | `AgentEvent` (streaming LLM output) and `UserEvent` (TUI input) enums |
| `src/agent/` | Agent lifecycle: `builder.rs` (rig Agent construction + tool injection), `runner.rs` (spawn, stream, per-call usage tracking), `prompt.rs` (system prompts), `read_lifecycle.rs` (stale/superseded read detection), `tools/` (tool implementations) |
| `src/session/` | Session state: `mod.rs` (messages, compactions, costs), `storage.rs` (JSON file I/O), `chat_history.rs` |
| `src/permission/` | Security: `checker.rs` (glob+regex rules, doom-loop detection, read-tool classification), `ask.rs` (user prompt UI), `pattern.rs` |
| `src/ui/` | Custom TUI on crossterm (no ratatui): `mod.rs` (event loop), `terminal.rs` (raw mode guard), `renderer.rs` (line buffer + viewport), `input/` (text editor + pickers), `status.rs`, `markdown.rs`, `event_handler.rs`, `cmd_picker.rs`, `slash/` (slash command implementations) |
| `src/context/` | Context gathering: embedded prompt themes (`prompts.rs`, `themes.rs`), AGENTS.md/ARCHITECTURE.md loading |
| `src/config/` | Configuration: `load.rs` (TOML/JSON from disk+env), `types.rs` (QuickModel, CustomProvider, Colors, EditSystem, ChainConfig, AdvisorConfig) |
| `src/extras/` | Feature-gated extensions: `context/` (SQLite externalization + retrieval), `loop/` (headless), `mcp/` (MCP client), `acp/` (ACP server), `memory/` (persistent memory), `subagents/` (parallel task delegation), `git_worktree/`, `archmd/` |
| `src/filter/` | Output compression: `compress.rs` (command-specific compressors for git/cargo/go/pytest/npm/docker, intent filtering) |
| `src/sandbox.rs` | `bwrap`/`zerobox` command wrapping |
| `src/fs.rs` | Filesystem utilities |
| `src/pricing.rs` | Token pricing constants |

## Key Types & Relationships

- **`Config`** (`src/config/mod.rs`) — central deserialized config, drives all runtime behavior.
- **`Cli`** (`src/cli.rs`) — `clap::Parser` args, overrides `Config` fields.
- **`AnyClient` / `AnyModel` / `AnyAgent`** (`src/provider.rs`) — type-erased enums wrapping rig's provider-specific clients (OpenAI, Anthropic, Gemini, Ollama, OpenRouter, DeepSeek). `AnyAgent` provides `run_print()` and `spawn_runner()`. Enum dispatch replaces dynamic dispatch.
- **`AgentRunner`** (`src/agent/runner.rs`) — holds `mpsc::Receiver<AgentEvent>`, spawned via `spawn_agent()`. Tracks per-call token usage via `CompletionCall` events.
- **`AgentEvent`** (`src/event.rs`) — `Token`, `Reasoning`, `ToolCall`, `ToolResult`, `SubagentToolCall`, `Error`, `Done`, `UsageUpdate`.
- **`Session`** (`src/session/mod.rs`) — serializable state: messages, compactions, costs, permission allowlist, model/provider info.
- **`PermissionChecker`** (`src/permission/checker.rs`) — dual-layer (glob + regex) rules, doom-loop detection, `SecurityMode` dispatch, read-tool classification (ctx_search/ctx_retrieve/ctx_stats are read-only; ctx_execute is not).
- **`ContextStore`** (`src/extras/context/store.rs`) — SQLite store with FTS5 dual tokenizers (Porter + trigram), RRF merge, fuzzy correction, event recording, tool output indexing, compact snapshots.
- **`ReadHistory`** (`src/agent/read_lifecycle.rs`) — process-wide read tracking: stale detection (file edited since read), superseded detection (re-read without edit), `summarize_dropped_content()` for externalization pointers.
- **`TerminalGuard`** (`src/ui/terminal.rs`) — RAII for raw mode, alt screen, mouse capture.
- **`Renderer`** (`src/ui/renderer.rs`) — line-buffered viewport, markdown rendering, scroll/selection.
- **`InputEditor`** (`src/ui/input/mod.rs`) — text buffer, cursor, history, kill-ring, picker integration.
- **`ContextFiles`** (`src/context/mod.rs`) — loaded agents, prompts, themes, architecture docs.

## Control Flow

```
CLI parse (main.rs) → config load → context load → context store init → session load
  │
  ├── --print-config → print and exit
  ├── --acp → extras::acp::serve()
  ├── --print → single agent.run_print() response
  ├── --loop → run_headless_loop() iterative mode
  └── (default) → ui::run_interactive()
```

### Interactive TUI Event Loop (`src/ui/mod.rs`)

Single `tokio::select!` with 4 branches:
1. **`UserEvent` from `user_rx`** — keyboard/mouse/resize/paste from background event thread
2. **`AgentEvent` from `agent_rx`** — streaming LLM tokens, tool calls, errors, usage updates
3. **Permission `AskRequest` from `ask_rx`** — user must approve/reject tool calls
4. **Periodic refresh** (100ms) — spinner animation when agent is running

## Data Flow

```
User input → InputEditor (buffer) → spawn_agent(prompt + history)
  │
  ▼
Agent (rig) → CompletionModel (LLM API)
  │
  ▼ streaming
AgentEvent stream (Token, ToolCall, ToolResult, UsageUpdate, Done)
  │
  ├── handle_agent_event() → Renderer (viewport buffer) → crossterm draw
  ├── ToolCall → PermissionChecker.check() → {Allowed, Ask, Denied}
  │     ├── Ask → permission_handler (user approves/rejects)
  │     └── Allowed → tool execution
  │           ├── bash/read/web_fetch → auto_index_output() if >threshold
  │           │     → ContextStore (SQLite) → pointer (id:N) in context
  │           └── write/edit → mark_file_edited() → read lifecycle tracking
  └── Done → Session.append() → session::storage::save_session()
```

## Context Mode Architecture

The `context` feature (enabled by default) provides a SQLite-backed externalization layer:

```
Tool output (bash/read/web_fetch/subagent)
  │
  ├── Output > 100KB → auto_index_output() → SQLite store
  │     └── Returns pointer: [id:N <bytes> from <cmd>. Use ctx_retrieve("N")]
  │
  ├── Output > 2KB saved → silent indexing (no pointer shown)
  │
  └── Output < 2KB → returned as-is

ctx_search(query) → FTS5 Porter + FTS5 trigram → RRF merge → results
  └── If both empty → fuzzy_correct(query) → retry

ctx_execute(code) → run script → compress output → index → return compressed
ctx_retrieve(id)  → fetch full raw output from SQLite (up to 8KB)
ctx_stats()       → show bytes saved / reduction %
```

### SQLite Schema

- `events` + `events_fts` (Porter) + `events_fts_trigram` — session events (tool calls, failures, compactions)
- `tool_output` + `tool_output_fts` (Porter) + `tool_output_fts_trigram` — externalized outputs
- `compact_snapshots` — priority-bucketed session snapshots
- `search_vocabulary` — vocabulary for fuzzy correction

### Read Lifecycle

```
read(file) → record_read() → check overlap with prior reads
  ├── file in EDITED_FILES → STALE (old data wrong)
  ├── same range read before → SUPERSEDED (redundant)
  └── coaching message prepended to read output

write/edit(file) → mark_file_edited() → untrack_read_path()
  └── next read of this file is fresh (not stale)
```

## Subagent Architecture

```
Main Agent (session_id: S)
  │
  ├── task(prompts: ["q1", "q2"])
  │     ├── Subagent 1 (session_id: S, shared context store)
  │     │     Tools: read, grep, find_files, list_dir, ctx_search, ctx_execute
  │     │     Prompt: Chinese (reasoning) → English (response, caveman style)
  │     │     Max 12 turns, 32KB response
  │     │
  │     └── Subagent 2 (session_id: S, shared context store)
  │           └── runs in parallel via tokio::spawn
  │
  └── Subagent outputs indexed → searchable via ctx_search by main agent
```

## Design Decisions

1. **Custom TUI over crossterm (no ratatui)** — keeps binary size minimal; project has its own line buffer, markdown renderer, scroll/selection.
2. **Type-erased enums, not trait objects** — `AnyAgent` enum wraps each provider variant. Avoids `dyn CompletionModel` lifetime issues; matching on enum is faster than vtable dispatch.
3. **Permission: dual-layer (glob + regex) rules** — glob for fast path, regex for complex patterns. Doom-loop detection tracks repeated identical tool calls.
4. **Session compaction + compact snapshots** — when token count approaches context window, old messages are summarized and dropped. A priority-bucketed snapshot (P1: errors/decisions, P2: file changes, P3: short messages) is saved to SQLite and re-injected on resume.
5. **Context mode: SQLite externalization** — large tool outputs stored in SQLite with FTS5 dual tokenizers. Dual tokenizers (Porter for stemming, trigram for substring) merged via RRF provide both word and identifier search. Fuzzy correction handles typos.
6. **Cache-aware compression** — command output compression is skipped when savings <15% to preserve the provider's prefix cache, avoiding cache busts for negligible token savings.
7. **Chinese reasoning prompts for subagents** — subagent system prompts are written in Chinese to compress token count (Chinese tokens are denser). The model reasons in Chinese but responds in English using a caveman-style format (dropped articles, fragments, no narration) for minimum output tokens.
8. **Feature-gated extras** — `loop`, `mcp`, `acp`, `memory`, `subagents`, `git-worktree`, `archmd`, `context`, `advisor` are all compile-time features.
9. **Single-threaded tokio by default** — `#[tokio::main(flavor = "current_thread")]` unless `multithread` feature enabled.

## Dependencies

| Crate | Use |
|---|---|
| `rig 0.38` | Agent framework: prompt hooks, tool system, streaming, provider clients |
| `clap 4` | Derive-based CLI argument parsing |
| `crossterm 0.29` | Terminal raw mode, color, cursor, mouse, paste events |
| `tokio 1` | Async runtime (current_thread default), channels, process, fs |
| `serde + serde_json + toml` | Config (TOML/JSON), session serialization (JSON) |
| `rusqlite 0.33` | SQLite store for context mode (FTS5, WAL, bundled SQLCipher) |
| `chrono`, `uuid` | Session timestamps and IDs |
| `pulldown-cmark 0.13` | Markdown → styled lines for TUI rendering |
| `ignore 0.4` | `.gitignore`-aware file traversal |
| `regex 1` | Permission pattern matching |
| `reqwest 0.13` | HTTP client |
| `mimalloc` | Global allocator (size + speed) |
| `compact_str`, `smallvec` | Heap-efficient small-string/small-vector types |

Optional: `rmcp 1.7` (MCP), `agent-client-protocol 0.12` (ACP).

## Entry Points

- **`main()`** (`src/main.rs`) — all modes dispatch from here
- **`--print`** / `-p` — `agent.run_print()` → single reply, then exit
- **`--loop`** — `run_headless_loop()` → iterative prompt/validate loop
- **`--acp`** — `extras::acp::serve()` → ACP server mode
- **Default (no flags)** — `ui::run_interactive()` → full TUI
- **`--resume`** / `--continue` / `--session <id>` — loads prior session before entering TUI/print
