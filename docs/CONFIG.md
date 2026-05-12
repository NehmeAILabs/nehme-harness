# Configuration

nehme-harness reads an optional config file. It supports both JSON and TOML
formats. The file is resolved by priority:

- If `NH_CONFIG_DIR` is set: `$NH_CONFIG_DIR/config.toml` or `$NH_CONFIG_DIR/config.json`
- Otherwise: `~/.config/nehme-harness/config.toml` or `~/.config/nehme-harness/config.json`
- Otherwise: `~/.local/share/nehme-harness/config.toml` or `~/.local/share/nehme-harness/config.json`

If a `config.toml` exists at a higher priority, it is used. If neither exists
at any priority, a default `config.toml` is created in the lowest-priority
directory (`~/.local/share/nehme-harness/`). On macOS the XDG config path above
resolves to `~/Library/Application Support/nehme-harness/`.

Prompts and themes are loaded from the same data directory:

- Prompts: `~/.local/share/nehme-harness/prompts/`
- Themes: `~/.local/share/nehme-harness/themes/`

All config keys are optional. CLI flags and their environment-backed values
(such as `NH_PROVIDER` and `NH_MODEL`) take precedence where both exist.

Example (TOML):

```toml
provider = "openrouter"
model = "deepseek/deepseek-v4-flash"
max_tokens = 16384
temperature = 0.7
context_window = 128000
reserve_tokens = 8192
keep_recent_tokens = 10000
compact_enabled = true
edit_system = "similarity"
default_prompt = "code"
default_permission_mode = "standard"
permission-modes = ["guarded", "standard", "yolo"]
show_tool_details = 3
deny_repeated_reads = false

[quick_models.fast]
provider = "openai"
model = "gpt-4o-mini"

[custom_providers.local-vllm]
provider_type = "openai"
base_url = "http://localhost:8000/v1"
api_key_env = "VLLM_API_KEY"

[permission]
"*" = "ask"
read = "allow"

[permission.write]
"**/*.rs" = "allow"
"**" = "ask"

[permission.bash]
"cargo test" = "allow"
"rm **" = "deny"

[permission.external_directory]
"/tmp/**" = "allow"
"/**" = "ask"
```

## Accepted top-level keys

| Key                       | Type    | Description                                                                                                                                                                 |
| ------------------------- | ------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `provider`                | string  | Provider name. Built-ins: `openrouter`, `openai`, `anthropic`, `gemini`/`google`, `ollama`, `deepseek`. Default: `openrouter`.        |
| `model`                   | string  | Model name. Default: `deepseek/deepseek-v4-flash`.                                                                                                                          |
| `max_tokens`              | integer | Maximum response tokens. Default: `16384`.                                                                                                                                  |
| `max_agent_turns`         | integer | Maximum agent turns per response. Default: `200`.                                                                                                                           |
| `temperature`             | number  | Model temperature (0.0–2.0). Only applied via `--temperature` CLI flag.                                                                                                    |
| `no_tools`                | boolean | Disable all tools. Default: `false`.                                                                                                                                        |
| `no_context_files`        | boolean | Disable loading `AGENTS.md`, `CLAUDE.md`, and `ARCHITECTURE.md`. Default: `false`.                               |
| `context_window`          | integer | Session context-window size for status and auto-compaction. Auto-detected from model catalog; falls back to `128000`. `0` disables auto-compaction. |
| `reserve_tokens`          | integer | Tokens to reserve before compaction triggers. Falls back to quick model's `reserve_tokens`, then `8192`.                                                                                                         |
| `keep_recent_tokens`      | integer | Approximate recent-token budget kept verbatim during compaction. Default: `10000`.                                                                                          |
| `max_text_file_size`      | integer | Maximum allowed file size in bytes for read/write operations. Default: `1048576` (1 MB).                                                                               |
| `max_read_lines`          | integer | Maximum lines returned by the read tool. |
| `max_bash_output_lines`   | integer | Maximum lines returned by the bash tool. |
| `max_grep_results`        | integer | Maximum results from grep. |
| `max_find_results`        | integer | Maximum results from find_files. |
| `max_list_dir_entries`    | integer | Maximum entries from list_dir. |
| `max_web_search_results`  | integer | Maximum web search results. |
| `max_web_fetch_length`    | integer | Maximum bytes fetched by web_fetch. |
| `deny_repeated_reads`     | boolean | Block repeated reads of the same file section until the file is edited or written. Default: `true`. |
| `compact_enabled`         | boolean | Enable automatic conversation compaction. Default: `true`. Also enables compact snapshots (context feature).                                                  |
| `always_show_welcome`     | boolean | Always show the welcome banner. Default: `false`.                                                                               |
| `auto_update_prompts`     | boolean | `true` = always regenerate prompts on version change. `false` = never. Unset = ask interactively.                                         |
| `auto_update_themes`      | boolean | Same as above, for themes.                                         |
| `edit_system`             | string  | `"similarity"` (SEARCH/REPLACE, default) or `"hashedit"` (CRC-32 tag-based CAS edits). See [HASHEDIT.md](HASHEDIT.md).                     |
| `custom_providers`        | object  | Map of provider aliases to `{ "provider_type", "base_url", "api_key_env", "api_style", "headers", "danger_accept_invalid_certs", "timeout_secs" }`. |
| `permission`              | object  | Permission rules using glob patterns.                       |
| `permission-regex`        | object  | Same structure as `permission` but patterns are regex.                       |
| `permission-allow`        | object  | Map of tool names to lists of glob patterns to allow.    |
| `permission-ask`          | object  | Map of tool names to lists of glob patterns to prompt on.  |
| `permission-deny`         | object  | Map of tool names to lists of glob patterns to deny.     |
| `restrictive`             | boolean | Select restrictive permission mode. Overridden by `accept_all`/`yolo`.                                                     |
| `accept_all`              | boolean | Select standard permission mode with auto-allow within CWD. Overridden by `yolo`.                            |
| `yolo`                    | boolean | Select yolo mode (allow all, ask for destructive bash).                                                                                                            |
| `permission-modes`        | array   | List of mode names that apply config-based rules. Default: `["guarded", "standard", "yolo"]`.             |
| `sandbox`                 | boolean | Run bash commands in the sandbox. Default: `false`.                                                                                                              |
| `sandbox_backend`         | string  | Sandbox backend: `"bwrap"` or `"zerobox"`.                                                                                                              |
| `shell`                   | string  | Shell to use for bash tool (default: system shell).                                                                                                              |
| `default_permission_mode` | string  | Permission mode when no mode boolean/CLI flag is set. Accepts: `standard` (default), `restrictive`, `readonly`, `guarded`, `yolo`.                                          |
| `show_tool_details`       | boolean or integer | Show tool-result previews in the TUI. `false` hides, `true` shows all, integer limits lines. Default: `3`. |
| `default_prompt`          | string  | Prompt name to activate on startup. Default: `code`. Applies `%%mode=` directive if present. |
| `editor`                  | string  | Editor command for `Ctrl+G` (default: `$EDITOR`, then `editor`, then `nano`).                                                                                        |
| `api_keys`                | object  | Map of provider names to API keys. Used as fallback when the env var is not set.                                                   |
| `quick_models`            | object  | Map of quick-model names to `{ "provider", "model", "reserve_tokens"?, "input_token_cost"?, "output_token_cost"?, "cached_input_token_cost"? }`.                                                      |
| `mcp_servers`             | object  | MCP server map when compiled with the `mcp` feature. When omitted, recommended MCPs are auto-configured.                                                   |
| `enable-exa-mcp`          | boolean | Auto-configure the Exa Web Search MCP server. Default: `true`.                                                                                                         |
| `enable-context7-mcp`     | boolean | Auto-configure the Context7 MCP server. Default: `false`.                                                                                                              |
| `enable-grepapp-mcp`      | boolean | Auto-configure the Grep.app MCP server. Default: `false`.                                                                                                              |
| `allow_all_mcp_calls`     | boolean | Skip permission checks for all MCP tool calls. Default: `false`.                                                                                   |
| `acp_servers`             | object  | ACP server config map (requires `acp` feature).                                                                                       |
| `acp_host`                | string  | TCP bind host for ACP server mode.                                                                                                              |
| `acp_port`                | integer | TCP bind port for ACP server mode (default: 7243).                                                                                               |
| `task_max_turns`          | integer | Max agent turns per subagent. Default: `12`. |
| `task_enabled`            | boolean | Whether the `task` tool is registered. Default: `true`. |
| `subagent_model`          | string  | Model name or quick-model alias for subagents. Default: uses main model. |
| `subagent_provider`       | string  | Provider for subagents. Default: same as main. |
| `subagent_max_read_lines`  | integer | Max lines subagent read tool returns. |
| `subagent_max_grep_results`| integer | Max grep results for subagents. |
| `subagent_max_find_results`| integer | Max find_files results for subagents. |
| `subagent_max_list_dir_entries` | integer | Max list_dir entries for subagents. |
| `colors`                  | object  | Background color overrides for the TUI. |
| `chain`                   | object  | Chain-of-prompts configuration. See below. |
| `advisor`                 | object  | Advisor configuration. See below. |
| `wt_auto_merge`           | boolean | Auto-merge worktree branch on exit. |
| `wt_base_dir`             | string  | Base directory for worktrees (default: parent of repo). |
| `wt_force`                | boolean | Force worktree remove and branch delete (`-D`) even if dirty. |

## OpenAI API styles and custom headers

The `openai` provider can talk to either of rig's two OpenAI transports:

- **`responses`** — the Responses API (`/responses`). Default for `api.openai.com`. Required for GPT-5-series models.
- **`completions`** — the Chat Completions API (`/chat/completions`). Default when a custom `base_url` is set.

Set `api_style` to override the auto-detected default.

Custom providers may send arbitrary HTTP headers. Header values support
`${ENV_VAR}` expansion:

```json
{
  "custom_providers": {
    "company-gateway": {
      "provider_type": "openai",
      "base_url": "https://gateway.example.com/v1",
      "api_key_env": "GATEWAY_API_KEY",
      "headers": {
        "cf-access-client-id": "${CF_ACCESS_CLIENT_ID}",
        "cf-access-client-secret": "${CF_ACCESS_CLIENT_SECRET}"
      }
    }
  }
}
```

The optional `timeout_secs` field overrides the default HTTP timeout.
`"danger_accept_invalid_certs": true` disables TLS verification.

## Colors

The `colors` object accepts three optional string fields, each of which can be a
named color or hex color (e.g. `"#1e1e2e"`).

- `chat_background` — background color for the main conversation buffer.
- `input_background` — background color for the text input area.
- `status_background` — background color for the status bar.

Supported named colors: `reset`, `black`, `red`, `green`, `yellow`, `blue`,
`magenta`, `cyan`, `white`, `grey`, `dark_grey`, `dark_red`, `dark_green`,
`dark_yellow`, `dark_blue`, `dark_magenta`, `dark_cyan`.

## Permission configuration

Permission actions are lowercase strings: `allow`, `ask`, or `deny`. Each tool
rule can be a single action or an object mapping patterns to actions. Supported
permission tool keys are `bash`, `read`, `write`, `edit`, `grep`, `find_files`,
`list_dir`, and `write_todo_list`. MCP-backed tools are checked under
`mcp_tool:{server_name}:{tool_name}`. Context tools (`ctx_search`, `ctx_retrieve`,
`ctx_stats`) are always allowed (read-only). `ctx_execute` is treated like `bash`.

Use `"*"` for the default action, `external_directory` for absolute-path rules
outside the working directory, and `doom_loop` for repeated identical tool calls
(default: `ask`).

Two config fields control permissions by pattern:

- **`permission`** — patterns are treated as globs (e.g. `**/*.rs`, `src/**`).
- **`permission-regex`** — same structure, but patterns are regex.

Both fields can be used together; rules from both are merged.

As a TOML-friendly alternative, use `permission-allow`, `permission-ask`, and
`permission-deny` at the top level:

```toml
permission-allow = { read = ["src/**", "tests/**"] }
permission-ask = { bash = ["rm **"] }
permission-deny = { write = ["/etc/**", "/usr/**"] }
```

## MCP server configuration

When compiled with MCP support, `mcp_servers` accepts command-based and URL-based
servers:

```json
{
  "mcp_servers": {
    "filesystem": {
      "command": "npx",
      "args": ["-y", "@modelcontextprotocol/server-filesystem", "."],
      "env": {}
    },
    "remote-search": {
      "url": "https://example.com/mcp",
      "headers": {
        "authorization": "Bearer token"
      }
    }
  }
}
```

### Recommended MCP servers

When `mcp_servers` is not explicitly set, three recommended MCP servers are
available:

| Key                    | Default | Description                                     | Env var              |
| ---------------------- | ------- | ----------------------------------------------- | -------------------- |
| `enable-exa-mcp`       | `true`  | Exa web search (mcp.exa.ai)                     | `EXA_API_KEY`        |
| `enable-context7-mcp`  | `false` | Context7 documentation lookup (mcp.context7.com) | `CONTEXT7_API_KEY`   |
| `enable-grepapp-mcp`   | `false` | Grep.app semantic code search (mcp.grep.app)     | `GREP_APP_API_KEY`   |

## ACP configuration

When compiled with the `acp` feature, nehme-harness can act as an ACP agent server.

| Key           | Type    | Description                                            |
| ------------- | ------- | ------------------------------------------------------ |
| `acp_servers` | object  | Named ACP server configurations. |
| `acp_host`    | string  | TCP bind host (default: stdio mode). |
| `acp_port`    | integer | TCP bind port (default: 7243). |

## Edit System Modes

nehme-harness supports two edit systems, selectable via `edit_system` config key,
`--edit-system` CLI flag, or `/editsys` slash command:

### `similarity` (default)

Aider-style SEARCH/REPLACE format with fuzzy matching fallback.

### `hashedit`

Tag-based edits using CRC-32 line hashes and file-level CAS (check-and-set)
tokens. Token-efficient (no old-text reproduction) and CAS-guarded. See
[HASHEDIT.md](HASHEDIT.md).

## Prompt directives

Custom prompt `.md` files may include a `%%mode=<mode>` directive on the
**first line** to automatically switch the security mode when the prompt
is activated.

Valid modes: `standard`, `restrictive`, `readonly`, `guarded`, `yolo`, `planwrite`.

Use `%%mode=last_user_mode` to keep (or restore) the mode the user last
set explicitly via `/mode` or startup config.

The directive line is stripped from the prompt content before it reaches
the agent.

## Chain-of-Prompts

When enabled, after the agent finishes responding with a `brainstorm`, `plan`,
or `code` prompt, the status bar shows `Continue to <next>? [Yes/But/No]`.

- **Yes** (`y`/`yes`) — switch to the next prompt and auto-submit a transition message.
- **But** (`but <msg>` / `b <msg>` / `yes but <msg>`) — same as yes, but prepend `<msg>`.
- **No** (`n`/`no`) — decline the chain, continue normally.

| Transition | Default | Description |
|-----------|---------|-------------|
| `brainstorm-to-plan` | `true` | After brainstorming, prompt to move to planning |
| `plan-to-code` | `true` | After planning, prompt to start coding |
| `code-to-review` | `false` | After coding, prompt to run a review |

```toml
[chain]
brainstorm-to-plan = true
plan-to-code = true
code-to-review = false
```

## Advisor

The advisor tool lets the agent consult a stronger reviewer model (or the
user, in human-handoff mode) for strategic guidance before making important
decisions.

```toml
[advisor]
enabled = true
model = "deepseek/deepseek-v4-pro"
# provider = "openrouter"         # defaults to main provider
# max_uses = 3                    # max advisor calls per request (nil = unlimited)
# human_handoff = false           # route advisor calls to the user instead
# advisor_kilobytes_limit = 256   # max KB of conversation context (split half head / half tail)
```

| CLI flag | Description |
|------|-------------|
| `--advisor` | Enable the advisor tool |
| `--advisor-model <name>` | Advisor model name |
| `--advisor-provider <name>` | Provider for the advisor model |
| `--advisor-max-uses <n>` | Max advisor calls per request |
| `--advisor-human-handoff` | Route advisor calls to the user |
| `--advisor-kilobytes-limit <n>` | Max KB of conversation context sent to advisor (default: 256) |

### Human handoff mode

When `human_handoff = true`, the agent's advisor calls are redirected to the
user instead of a second model. The agent pauses, shows its question, and the
user types a response.

Runtime control via `/advisor`:
```
/advisor                    Show current advisor status
/advisor on|off             Enable or disable the advisor
/advisor handoff [on|off]   Toggle human handoff mode
/advisor model <name>       Change the advisor model
/advisor max-uses <n>       Set max advisor calls per request (0 = unlimited)
/advisor context-limit <n>  Set max kilobytes of conversation context
```
