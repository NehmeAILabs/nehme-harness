# Subagents (read-only codebase exploration)

## Overview

Subagents let the main agent delegate **precise read-only investigations** to a
**read-only child agent**. Each subagent receives a specific technical question
(e.g. "Where is MCP support implemented?") and returns a focused answer.
This keeps the main agent's context clean while enabling thorough lookups.

Subagents are designed for **highly specific questions**, not wide exploration.
Avoid broad instructions like "check all documentation" — instead ask precise
questions that can be answered with a few file reads and searches.

When the main agent calls the `task` tool, one subagent is spawned per prompt.
If multiple prompts are given, they run in **parallel** via `tokio::spawn`.

## Feature Gate

Subagents are enabled by the `subagents` Cargo feature (included in the default
feature set).

## The `task` Tool

The main agent has a tool called `task`. It accepts:

```json
{
  "prompts": ["explore the auth module", "find all API route definitions"]
}
```

- **Single prompt**: one subagent explores, returns findings.
- **Multiple prompts**: subagents run concurrently. Each result appears under a `## Task N:` heading.

## What the Subagent Can Do

### Read tools (always available)

| Tool       | Purpose                       |
|------------|-------------------------------|
| `read`     | Read file contents            |
| `grep`     | Regex search in files         |
| `find_files` | Find files by glob pattern |
| `list_dir` | List directory contents       |

### Context tools (when `context` feature is enabled)

| Tool           | Purpose                                |
|----------------|----------------------------------------|
| `ctx_search`   | Full-text search of indexed tool outputs from the session's context store |
| `ctx_execute`  | Run Python/Node/Bash/Ruby/Go scripts with auto-compression and indexing |

Subagents share the **same session_id** as the main agent, so any output they
index via `ctx_execute` or that gets auto-externalized is searchable by the
main agent via `ctx_search`.

### Memory tools (when `memory` feature is enabled)

| Tool            | Purpose                                |
|-----------------|----------------------------------------|
| `memory_read`   | Read memory files (long-term, notes…)  |
| `memory_search` | Keyword search across all memory       |

### Explicitly excluded

| Tool           | Reason                                  |
|----------------|-----------------------------------------|
| `write`        | Subagent is read-only by design         |
| `edit`         | Subagent is read-only by design         |
| `bash`         | Not needed — read tools cover exploration |
| `ctx_retrieve` | Not exposed to subagents (only main + BTW agents) |
| `ctx_stats`    | Not exposed to subagents |
| `memory_write` | Subagent should not persist memory      |
| `mcp_tool`     | External, unpredictable — out of scope  |

## Subagent System Prompt

The subagent system prompt is written **in Chinese** for token efficiency.
Chinese text compresses more tightly into tokens than English, reducing the
system prompt overhead. The model reasons in Chinese but responds in English.

Key instructions in the prompt:
- **Role**: Code investigation agent, skilled at searching and reading codebases
- **Tools**: `find_files`, `grep`, `read`, `list_dir`, `ctx_search`, `ctx_execute`
- **Constraints**: Return absolute paths; read-only, no file modification, no shell commands
- **Language**: Final answer in English; reasoning in Chinese (中文). Code, file paths, variable names, and error messages kept verbatim (not translated)
- **Response style (caveman)**: Minimum tokens. Drop articles (a/an/the), fillers (just/really/basically), politeness. Fragments OK. Don't narrate tool calls. No decorative formatting. Quote only shortest key error lines. Pattern: `[discovery] [evidence] [answer]`. Expand only when a fragment could be misread, for safety warnings, or for irreversible operations.

## Security & Permissions

The subagent is built with **no permission system** (`permission: None` on all
its tools). This is safe because it only has read and context-search tools:

- Read tools with `None` permission will read any path without checks, but they cannot write, edit, or execute commands.
- `ctx_execute` goes through the subagent's tool set but runs in a subprocess with no write access to the project.

The main agent's `task` tool itself goes through the normal permission check
(`check_perm("task", …)`), so users can allow/ask/deny it via their permission
rules.

## Configuration

| Config field           | Type      | Default             | Description                           |
|------------------------|-----------|---------------------|---------------------------------------|
| `task_max_turns`       | `usize`   | `12`                | Max agent turns per subagent          |
| `task_enabled`         | `bool`    | `true`              | Whether the `task` tool is registered |
| `subagent_model`       | `string`  | (uses main model)   | Model name or quick-model alias       |
| `subagent_provider`    | `string`  | (same as main)      | Provider for the subagent (optional)  |

### Model resolution (in order of precedence)

1. `subagent_model` matches a **quick model name** → uses that quick model's provider + model.
2. `subagent_model` is set but does **not** match a quick model → uses the raw model string with `subagent_provider` (or main provider as fallback).
3. `subagent_model` is **not** set but `subagent_provider` is → uses the main model with the specified provider.
4. Neither is set → falls back to the main agent's model.

Example config:

```json
{
  "task_max_turns": 12,
  "task_enabled": true,
  "subagent_model": "deepseek-v4-flash",
  "subagent_provider": "openrouter"
}
```

## Slash Commands

| Command                            | Description                                |
|------------------------------------|--------------------------------------------|
| `/model-subagent [name]`           | Show or switch the subagent's model        |
| `/models-subagent [name]`          | List quick models or switch subagent to one|

## Architecture

```
Main Agent (session_id: S)
┌──────────────┐                         ┌─────────────────────┐
│ read/write   │                         │ read                │
│ edit/bash    │  calls "task" tool      │ grep                │
│ grep/find    │ ──────────────────────→│ find_files          │
│ ctx_search   │   with prompt(s)        │ list_dir            │
│ ctx_execute  │   spawns parallel       │ ctx_search          │
│ ctx_retrieve │   subagents via         │ ctx_execute         │
│ ctx_stats    │   tokio::spawn          │ memory_read (opt)   │
│ task  ───────┤                         │ memory_search (opt) │
│              │   ──────────────        │                     │
│              │   returns findings ────→│ runs ≤ 12 turns     │
│              │                         │ ≤ 32KB response     │
│              │                         │ returns summary     │
└──────────────┘                         └─────────────────────┘
                                               │
                                               │ auto_index_output()
                                               │ (outputs >100KB)
                                               ▼
                                         ContextStore (SQLite)
                                               │
                                               │ searchable via
                                               │ ctx_search by
                                               │ main agent
                                               ▼
                                         [id:N pointer in
                                          main agent context]
```

Key files:

| File                                         | Role                                  |
|----------------------------------------------|---------------------------------------|
| `src/extras/subagents/mod.rs`                | Module root, static config, `SubagentConfig` with `session_id` |
| `src/extras/subagents/task_tool.rs`          | `TaskTool` implementation, accepts `session_id` |
| `src/extras/subagents/builder.rs`            | Subagent construction (`build_explore_agent`), threads `session_id` to all tools |
| `src/extras/subagents/prompt.rs`             | Chinese system prompt with caveman response style |
| `src/agent/runner.rs` (`run_subagent`)       | Silent agent execution |
| `src/agent/builder.rs`                       | Wires `TaskTool` into main agent with session_id |
| `src/main.rs`                                | Initializes `SubagentConfig` with session ID |

## Session ID Sharing

The subagent shares the **same `session_id`** as the main agent. This is critical
for context-mode integration:

1. When a subagent runs `ctx_execute` or produces a large output, it is auto-indexed into the context store under the shared session_id.
2. The main agent can then search that output via `ctx_search` without the output ever appearing in the main context window.
3. The main agent can retrieve the full output via `ctx_retrieve("id:N")` if needed.

## Parallel Execution

When multiple prompts are supplied, each runs in its own `tokio::spawn` task.
`futures::future::join_all` gathers the results. A failed subagent (panic or
error) does not cancel the others — its output shows the error while the rest
complete normally. Results are ordered by the original prompt index.

## Limits

- **Max turns**: 12 per subagent (configurable via `task_max_turns`)
- **Max response**: 32KB (truncated with externalization pointer if exceeded)
- **No write/edit/bash**: subagents are strictly read-only investigators
