# ARCHITECTURE.md

nehme-harness supports an optional `ARCHITECTURE.md` file that gives both the main
agent and exploration subagents high-level design context about your project.

## What It Does

When `ARCHITECTURE.md` files exist (at the project root and/or in parent
directories), their content is appended to the agent's system prompt preamble
— right after `AGENTS.md` context and before the custom prompt. This means
every LLM call carries awareness of your project's architecture.

**All subagents also receive the same architecture context**, so they can
explore the codebase with an understanding of the overall design.

## Why Use It

### For the Agent

Without `ARCHITECTURE.md`, an agent exploring a large codebase starts with
zero architectural knowledge. It reads `AGENTS.md` for conventions, then
begins probing files one by one. This works but costs tokens and turns as
the agent builds a mental model from scratch.

With `ARCHITECTURE.md`, the agent enters the conversation already knowing:
- How the project is organized (directory layout, module responsibilities)
- Key types, traits, and data structures
- Where control flow and data flow live
- Design decisions and constraints
- External dependencies and their roles
- Entry points and how things boot

This front-loads understanding, reducing the number of read probes needed and
making the agent's first responses more accurate. Combined with context-mode
externalization, this means the agent spends fewer tokens on exploration and
more on the actual task.

### For the User

- **Consistency across sessions** — the agent stays aligned with your design intent across different conversations
- **Better subagent delegation** — when using the `task` tool, subagents understand the architecture without querying the main agent
- **Onboarding** — new contributors (human or AI) get a structured overview
- **Living documentation** — the agent can update `ARCHITECTURE.md` when significant changes are made

## Discovery and Loading

nehme-harness loads `ARCHITECTURE.md` files using the same recursive upward search
as `AGENTS.md`:

1. **Global**: `~/.local/share/nehme-harness/agent/ARCHITECTURE.md` (XDG data dir)
2. **Project**: `ARCHITECTURE.md` in the current working directory and all parent directories up to the filesystem root

Files from all levels are concatenated, with source-path headers indicating
where each block came from. This lets you define organization-wide conventions
in the global file while having project-specific architecture in each repo.

At startup, if no `ARCHITECTURE.md` is found anywhere in the directory tree,
nehme-harness offers to create one:

```
No ARCHITECTURE.md found in /home/you/project. Create one? [y/N]
```

If you answer yes, a template is written to the project root. When the template
is created, nehme-harness automatically injects a system message instructing the
agent to explore the codebase and populate the file with a thorough architectural
overview.

### Template Contents

The generated template contains sections for directory layout, key types/traits,
control flow, data flow, design decisions, dependencies, and entry points.

## Disabling

Pass `--no-context-files` (or `-n`) to suppress loading of both `AGENTS.md`
and `ARCHITECTURE.md`. You can also set `no_context_files = true` in your
config file.

## How It Integrates

| Layer | Behavior |
|---|---|
| **System prompt** | Architecture content appended after `AGENTS.md`, before custom prompt |
| **Subagents** | Each subagent receives the architecture context in its preamble |
| **`task` tool** | Exploration subagents can reference architecture context |
| **TUI status** | Displays `loaded ARCHITECTURE.md` when architecture content exists |
| **Prompts** | Built-in prompts reference architecture-aware workflows |

## Writing a Good ARCHITECTURE.md

A well-written `ARCHITECTURE.md` should be **concise** (aim for 200-500 words
for small projects, 500-2000 for larger ones) and **actionable** — think of it
as a cheat sheet the agent can reference when making decisions. Avoid
reproducing code; focus on structure, relationships, and rationale.

### Recommended Sections

1. **Directory Layout** — one-line summaries of each top-level directory
2. **Key Types/Traits** — the 5-10 most important data structures
3. **Control Flow** — request lifecycle, main loops, async boundaries
4. **Data Flow** — how data enters, transforms, and exits the system
5. **Design Decisions** — "why X instead of Y" for critical choices
6. **Dependencies** — key libraries and what they're used for
7. **Entry Points** — binary entry, API handlers, CLI parsing

## Comparison with AGENTS.md

| Aspect | AGENTS.md | ARCHITECTURE.md |
|---|---|---|
| **Purpose** | Coding conventions, instructions, project-specific procedures | High-level design: structure, relationships, rationale |
| **Scope** | "How to work in this codebase" | "How this codebase is built" |
| **Update frequency** | Rare (conventions change slowly) | With significant refactors or new modules |
| **Typical length** | Short to medium | Medium (200-2000 words) |

Both files complement each other: `AGENTS.md` tells the agent how to operate;
`ARCHITECTURE.md` tells it what it's operating on.
