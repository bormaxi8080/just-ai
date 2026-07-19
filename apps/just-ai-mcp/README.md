# just-ai MCP adapter

This independent stdio adapter exposes the stable `just-ai` inspection and
preparation contracts to MCP clients. It is deliberately read-only: no tool can
execute a recipe, apply a proposal, or write a project file.

```sh
cargo build --manifest-path apps/just-ai-mcp/Cargo.toml --release
```

Configure the resulting `just-ai-mcp` binary as a stdio MCP server. The adapter
implements newline-delimited JSON-RPC and writes no logs to stdout.

Tools:

- `inspect_project` — full serializable recipe/project context;
- `doctor` — deterministic per-recipe risk reports;
- `prepare_run` — `just --dry-run` preview, risk, and confirmation policy.
