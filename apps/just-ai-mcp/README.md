# just-ai MCP adapter

This independent stdio adapter exposes the stable `just-ai` inspection and
preparation contracts to MCP clients. It is deliberately read-only: no tool can
execute a recipe, apply a proposal, or write a project file.

```sh
cargo build --manifest-path apps/just-ai-mcp/Cargo.toml --release
```

Configure the resulting `just-ai-mcp` binary as a stdio MCP server. The adapter
implements newline-delimited JSON-RPC and writes no logs to stdout.
It negotiates the published MCP protocol versions from `2024-11-05` through
`2025-11-25`, falling back to its latest supported version for unknown clients.

Tools:

- `inspect_project` — full serializable recipe/project context;
- `doctor` — deterministic per-recipe risk reports;
- `prepare_run` — `just --dry-run` preview, risk, and confirmation policy.

Prompts:

- `implement` — implement one verified architecture increment;
- `review-architecture` — inspect dependency direction and safety invariants;
- `refresh-index` — rebuild and verify the Codebase Memory MCP graph;
- `system` — apply the project maintainer invariants;
- `verify` — run the layered test, lint, upstream, and graph gates.

The prompt catalog is compiled directly from the canonical files under
`agent/`; the adapter has no second editable copy. Prompts accept no arguments
and are returned as MCP `user` messages because the protocol prompt-message
roles are limited to `user` and `assistant`.

Resources:

- `just-ai://docs/architecture` — boundaries and verification gates;
- `just-ai://docs/roadmap` — completed and deferred increments;
- `just-ai://docs/adr/*` — the five accepted architecture decisions.

Resources are a build-time allowlist of canonical Markdown documents. The
adapter exposes no path template, arbitrary filesystem read, subscription, or
resource-change notification capability.

Black-box integration tests spawn the built binary with piped stdio. They
verify one JSON-RPC object per stdout line, silent notifications, and continued
service after malformed JSON input. Stderr remains available for diagnostics.
