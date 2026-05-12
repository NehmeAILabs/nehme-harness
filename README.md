# nehme-harness

Heavy fork of [zerostack](https://github.com/gi-dellav/zerostack), optimized for
token efficiency and memory footprint. All credit for the original codebase goes
to the zerostack authors.

## What's different from zerostack

- **Context mode** (default): SQLite-backed externalization layer that keeps large tool outputs out of the context window while making them searchable on demand. Dual FTS5 tokenizers (Porter + trigram), RRF merge, fuzzy correction, auto-externalization of outputs >100KB, compact snapshots that survive compaction
- **Read lifecycle tracking**: detects stale reads (file was edited since last read) and superseded reads (re-read without edit), coaching the agent to avoid wasting context on redundant data
- **Cache-aware compression**: bash output compression is skipped when savings <15% to preserve the provider's prefix cache
- **Subagent prompt optimization**: parallel read-only codebase exploration with Chinese reasoning prompts and caveman-style response format for token efficiency. Subagents share the session's context store
- **Per-call token usage tracking**: accurate per-CompletionCall usage instead of aggregated cumulative totals

## Features

- **Multi-provider**: OpenRouter, OpenAI, Anthropic, Gemini, Ollama, DeepSeek, plus custom OpenAI-compatible providers
- **Standard tools**: read, write, edit, bash, grep, find_files, list_dir, web_search, web_fetch, code_graph, write_todo_list, task
- **Context tools**: `ctx_search` (full-text search indexed outputs), `ctx_execute` (run scripts with auto-compression), `ctx_retrieve` (fetch externalized output by ID), `ctx_stats` (savings statistics)
- **Permission system**: five configurable modes with per-tool patterns, session allowlists, and doom-loop detection
- **Session management**: save/load/resume sessions, auto-compaction to stay within context windows
- **Terminal UI**: crossterm-based, markdown rendering, mouse selection/copy, scrollback, reasoning visibility toggle
- **Prompts system**: switch between system prompt modes at runtime (`code`, `plan`, `review`, `debug`, etc.)
- **MCP support**: connect MCP servers for extended tooling
- **ACP support** (gated): Agent Communication Protocol server for editor integration (Zed, etc.)
- **Persistent memory** (gated): plain-Markdown memory across sessions
- **ARCHITECTURE.md**: companion to AGENTS.md, provides shared architectural context to all agents
- **Loop system**: iterative coding for long-horizon tasks
- **Git worktrees**: branch-per-task workflow with `/worktree`, `/wt-merge`, `/wt-exit`
- **Advisor** (gated): consult a stronger reviewer model for strategic guidance

## Performance

- Binary size: ~26MB
- RAM footprint: ~16MB average, ~24MB peak
- CPU: 0.0% idle, ~1.5% when using tools
- Token efficiency: context-mode externalization + read lifecycle tracking reduces context window consumption by ~28% vs baseline on benchmark tasks

## Installation

### Basic installation (recommended)

```bash
curl -fsSL https://raw.githubusercontent.com/NehmeAILabs/nehme-harness/main/install.sh | bash
```

Or pick a tarball from [GitHub Releases](https://github.com/NehmeAILabs/nehme-harness/releases).

### Nix

```bash
nix run github:NehmeAILabs/nehme-harness
nix profile install github:NehmeAILabs/nehme-harness
```

### Cargo

```bash
# Default features: loop, git-worktree, mcp, subagents, archmd, status-signals, context
cargo install nehme-harness

# All features
cargo install nehme-harness --all-features

# Specific features
cargo install nehme-harness --features acp,memory,advisor
```

Once installed, run `/prompt autoconfig` inside `nh` to explore the documentation and configure interactively.

### Optional: sandbox mode

Install [bubblewrap](https://github.com/containers/bubblewrap) for `--sandbox`, which runs every bash command inside an isolated environment.

## Quick start

```bash
# Set your API key (OpenRouter is default)
export OPENROUTER_API_KEY="[api_key]"

# Interactive session (default prompt: code)
nh

# Monochrome TUI
nh --no-color

# One-shot mode
nh -p "Explain this project"

# Continue last session
nh -c

# Explicit provider/model
nh --provider openrouter --model deepseek/deepseek-v4-flash
```

## Configuration

See [docs/CONFIG.md](docs/CONFIG.md) for config file location, accepted keys, provider
aliases, permission rules, and MCP server configuration.

Run `/prompt autoconfig` to use a specialized agent that navigates the documentation and customizes your setup.

## Context mode

Context mode is enabled by default (the `context` Cargo feature). It provides:

- **Auto-externalization**: tool outputs >100KB (bash, read, web_fetch, subagent responses) are stored in SQLite and replaced with a compact pointer (`id:N`). Use `ctx_retrieve("N")` to fetch the full content later.
- **Full-text search**: `ctx_search` queries all indexed outputs using dual FTS5 tokenizers (Porter stemming + trigram substring matching), merged via Reciprocal Rank Fusion. Fuzzy correction kicks in when both tokenizers return zero hits.
- **Code execution**: `ctx_execute` runs Python/Node/Bash/Ruby/Go scripts, compresses output, and indexes it automatically.
- **Compact snapshots**: on `/compact`, a priority-bucketed snapshot (P1: errors/decisions, P2: file changes, P3: short messages) is saved to SQLite and re-injected on session resume.
- **Read lifecycle**: tracks every file read per session. Stale reads (file edited since read) and superseded reads (re-read without edit) generate coaching messages to prevent context waste.
- **Cache-aware gating**: command output compression is skipped when it would save <15%, preserving the provider's prefix cache.

See [docs/CONTEXT.md](docs/CONTEXT.md) for full details.

## Prompts system

Built-in prompts that change the agent's behavior and tone:

| Prompt                | Description                                                              |
| --------------------- | ------------------------------------------------------------------------ |
| **`code`** (default)  | Coding mode with full file and bash tool access, TDD workflow            |
| **`plan`**            | Planning-only mode — explores and produces a plan without writing code   |
| **`review`**          | Code review mode — reviews for correctness, design, testing, and impact  |
| **`debug`**           | Debug mode — finds root cause before proposing fixes                     |
| **`ask`**             | Read-only mode — only read/grep/find_files permitted                     |
| **`brainstorm`**      | Design-only mode — explores ideas and presents designs without code      |
| **`frontend-design`** | Frontend design mode — distinctive, production-grade UI                  |
| **`review-security`** | Security review mode — finds exploitable vulnerabilities                 |
| **`simplify`**        | Code simplification mode — refines for clarity without changing behavior |
| **`write-prompt`**    | Prompt writing mode — creates and optimizes agent prompts                |
| **`write-text`**      | Prose writing mode — emails, blog posts, docs                            |
| **`autoconfig`**      | Configuration helper — navigates docs and edits config interactively     |

Custom prompts go in `$XDG_CONFIG_HOME/nehme-harness/prompts/`.

The agent automatically loads `AGENTS.md` or `CLAUDE.md` from the project root or ancestor directories. When the `archmd` feature is enabled, `ARCHITECTURE.md` is also loaded. Use `-n` / `--no-context-files` to disable all context file loading.

## Permission system

| Mode | CLI flag | Behavior |
|------|----------|----------|
| **restrictive** | `-R` / `--restrictive` | Ask for every operation. Config rules ignored by default. |
| **readonly** | `--read-only` | Allow read/grep/find_files/list_dir. Deny writes, edits, bash. |
| **guarded** | `--guarded` | Allow read tools. Ask for writes, edits, bash. Config rules apply. |
| **standard** | (default) | Allow path tools within CWD. Safe bash auto-allowed. Config rules apply. |
| **yolo** | `--yolo` | Allow everything, prompt for destructive bash. Config rules apply. |

The `--dangerously-skip-permissions` flag bypasses all checks.

Context tools permission: `ctx_search`, `ctx_retrieve`, `ctx_stats` are read-only and always allowed. `ctx_execute` is treated like bash (requires permission in guarded/standard modes).

## Slash commands

Key commands:

- `/model` — Switch model
- `/thinking` — Set thinking level
- `/clear` — Clear conversation
- `/session` — List/save/load sessions
- `/loop` — Schedule recurring prompts
- `/prompt` — List or change the agent's prompt
- `/mode` — Set the permission mode
- `/queue` — Manage input queued while the agent is busy
- `/btw` — Ask a quick side question in parallel
- `/compress` — Compress conversation history (also saves a compact snapshot)

Use `/help` for the full list. See [docs/COMMANDS.md](docs/COMMANDS.md).

## Subagents

The `task` tool spawns read-only subagents for codebase exploration. Subagents:
- Run in parallel (multiple prompts = concurrent subagents)
- Use Chinese reasoning prompts with caveman-style English responses for token efficiency
- Share the session's context store (indexed outputs are searchable by the main agent via `ctx_search`)
- Have access to: `read`, `grep`, `find_files`, `list_dir`, `ctx_search`, `ctx_execute`, and memory tools (if enabled)
- Max 12 turns per subagent, 32KB max response

See [docs/SUBAGENTS.md](docs/SUBAGENTS.md).

## Session management

Sessions are saved to `$XDG_DATA_HOME/nehme-harness/sessions/`. Use `-c` to resume the most recent, `-r` to browse, or `--session <id>` to load a specific one.

## Memory

Gated behind the `memory` feature. Keeps plain-Markdown notes on disk and injects relevant ones into the system prompt at the start of every session.

```bash
cargo install nehme-harness --features memory
```

See [docs/MEMORY.md](docs/MEMORY.md).

## Loop system

Iterative coding loop for long-horizon tasks. The agent reads the task, picks an item from the plan, works on it, runs tests, updates the plan, and loops.

```
/loop Implement the user authentication system
/loop stop
/loop status
```

Headless: `nh --loop --loop-prompt "Refactor the API" --loop-max 10 --loop-run "cargo test"`

## Git worktrees

Branch-per-task workflow using git worktrees:

| Command | Description |
|---------|-------------|
| `/worktree <name>` | Create a worktree on branch `<name>` and move into it |
| `/wt-merge [branch]` | Merge the worktree branch, push, clean up, return |
| `/wt-exit` | Return to the main repo without merging |

`--parallel` creates a timestamped worktree with auto-merge on exit.

## ACP support

Gated behind the `acp` feature. nehme-harness acts as an ACP Agent server for editors like Zed.

```bash
nh --acp                              # stdio mode
nh --acp --acp-host 0.0.0.0 --acp-port 7243  # TCP mode
```

## Supported providers

- OpenRouter (default)
- OpenAI (Responses API + Chat Completions API)
- Anthropic (with prompt caching)
- Gemini
- Ollama
- DeepSeek

Custom providers can be configured with any base URL and API key environment variable. See [docs/PROVIDERS.md](docs/PROVIDERS.md).

## License

GPL-3.0-only
