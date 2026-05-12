When compiling nehme-harness:
- Never run `cargo build`
- Don't use `--release` during development
- Never run `cargo check` (instead use `cargo test`)
- Always run `cargo fmt`
- Always run `cargo install --path . --debug`
- Run `cargo test` if you want to check all unit tests

Important notes:
- Always write tests when writing new non-TUI code.
- Always update docs/ files when needed.
- If adding or editing slash commands, edit the slash commands `/` picker in the TUI.
- The binary is `nh` (not `nehme-harness`). The package name is `nehme-harness`.
- Context mode is enabled by default. All context-mode code paths are `#[cfg(feature = "context")]`.
- Subagent and BTW prompts are written in Chinese for token efficiency. Keep them in Chinese.
- `ctx_search`, `ctx_retrieve`, `ctx_stats` are read-only/always-allowed. `ctx_execute` requires permission (treated like bash).
