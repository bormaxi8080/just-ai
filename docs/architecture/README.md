# just-ai architecture

`just-ai` is an optional layer around the unmodified `just` task runner.
The project follows a core-first, ports-and-adapters architecture.

## Product boundaries

```text
justfile -> just CLI -> just-ai-core <- CLI
                                  <- Tauri GUI
                                  <- read-only stdio MCP adapter
```

`just` owns parsing, dependency resolution, and recipe execution. `just-ai`
uses the public JSON dump and process interface; it must not add AI behavior to
the `just` crate or depend on private `just` modules.

The core owns project inspection, deterministic risk analysis, policy,
provider-neutral AI operations, validated change proposals, and run events.
Presentation adapters must never accept or execute arbitrary shell strings.

## Dependency rules

1. The root `just` package does not depend on any `just-ai` package.
2. Core code does not depend on Clap, Tauri, React, HTTP, or terminal output.
3. CLI and GUI depend inward on core contracts.
4. AI produces proposals. Local deterministic code validates and applies them.
5. Recipe execution is always delegated to the configured `just` binary.
   A `--` terminator separates just-ai-controlled options from the recipe name
   and every user-supplied recipe argument.
6. File writes are workspace-confined, hash-guarded, validated, and atomic.
7. Secrets and excluded files never enter remote AI context.
8. Provider transport is native Rust behind `AiProvider`; model responses must
   pass operation-specific JSON Schema validation before deserialization.
9. Streaming cancellation terminates the recipe process tree through Unix
   process groups or Windows Job Objects.
10. MCP clients cannot select the `just` executable; that process boundary is
    controlled by the server environment.
11. Each MCP server is scoped to its process working directory; clients cannot
    redirect inspection or preparation to another project root.
12. MCP tool arguments are runtime-validated against explicit per-tool
    allowlists before any filesystem or process operation.
13. Preparation rejects structured function calls and `dotenv-command` before
    invoking dry-run, since upstream may evaluate them during preview.
14. Every captured `just` subprocess stream is capped at 8 MiB. Dump,
    validation, and dry-run use the shared capture adapter; recipe execution
    additionally uses a bounded event queue and terminates its process tree on
    overflow.

## Packages

The library exposes a small public API and a two-line binary. Adapter and core
modules are physically separate while the CLI contract stays stable.

```text
crates/just-ai/src/
  lib.rs             small public composition API
  bounded_output.rs  bounded concurrent subprocess output capture
  cli.rs             Clap adapter and terminal rendering
  inspection.rs      just JSON dump boundary and project context
  just_dump.rs       shared non-evaluating JSON dump process boundary
  ai_responses.rs    typed model responses and JSON Schemas
  proposal.rs        validation, rendering, diff, guarded application
  provider.rs        native provider adapter
  application/       execution, history, scanner, patch use cases
  domain/            risk and policy rules

apps/just-ai-gui/     separate Tauri/React adapter
apps/just-ai-mcp/     separate read-only adapter with isolated stdio transport,
                     JSON-RPC protocol, catalog, and core tool modules
agent/                canonical prompts and project-management commands
```

The MCP adapter publishes the canonical `agent/` sources through
`prompts/list` and `prompts/get`. Prompt files are embedded at build time, so
the CLI and MCP surfaces cannot drift while the installed binary is running.
The same source backs `just-ai agent verify` and the MCP `verify` prompt.
It also publishes this architecture guide, roadmap, and accepted ADRs through
a fixed `just-ai://docs/*` resource allowlist. No client-supplied path reaches
the filesystem. The protocol layer validates JSON-RPC envelopes before
classifying messages as notifications, so malformed notification-shaped input
receives a standard error while valid notifications remain silent. The stdio
transport bounds each input frame to 1 MiB and recovers at the next newline
after oversized or non-UTF-8 input.

## Verification gates

Every architecture increment must pass:

```sh
cargo fmt --check
cargo clippy --package just-ai --all-targets -- -D warnings
cargo test --package just-ai
cargo test --lib
cargo test --manifest-path apps/just-ai-mcp/Cargo.toml
```

The core suite includes versioned basic, rich, and Windows JSON-dump fixtures.
The Windows CI job additionally verifies native drive-letter path semantics and
Job Object cancellation.

The Codebase Memory MCP index is refreshed after structural changes. Graph
queries are used to verify that adapters depend on core and not vice versa.
