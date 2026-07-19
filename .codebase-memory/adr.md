# just-ai architecture decisions

## Companion and module boundaries
The upstream `just` package remains behaviorally unchanged and has no dependency on just-ai. `just-ai` communicates through the public JSON dump and direct argv-based process execution. Physical modules separate CLI, inspection, AI contracts, proposals, providers, application, and domain. The Tauri/React GUI and JSON-RPC/stdio MCP server are independent workspace adapters that depend inward on core.

## AI safety, transport, and context
AI output is an untrusted typed proposal, never an executable action. Local deterministic code validates syntax, dependencies, risk, workspace paths, and reviewed content before atomic application. Providers implement `AiProvider`. OpenAI uses Responses with strict Structured Outputs; the default is `gpt-5.6-terra` with `reasoning.effort=none`. Ollama uses native `/api/chat` with schema format, non-streaming output, and temperature zero. Generic OpenAI-compatible servers retain Chat Completions. Returned text is validated locally against the operation schema. Project context is allowlist-only, bounded, excludes dotenv files, and redacts likely credentials.

## Execution and history
Execution is two-phase: prepare obtains preview, risk, and policy; execute re-prepares before confirmation. Frontends submit recipe names and argv arrays, never shell strings. Streaming children use an isolated Unix process group so cancellation terminates descendants. Windows Job Object support remains pending. History is bounded, atomic, per-project JSONL with redacted 16 KiB tails. Records include argv and cancellation state with serde defaults for older JSONL. Canonical project roots unify CLI and GUI history.

## Desktop contract
The desktop adapter renders `ContextParameter` and never parses justfiles. Singular values remain one argv element; plus/star values use one element per line. History renders timestamps, argv, cancellation state, and expandable redacted tails through bounded Tauri IPC.

## MCP contract
`apps/just-ai-mcp` is a separate newline-delimited JSON-RPC/stdio adapter for MCP protocol version 2025-11-25. Stdout is protocol-only. It exposes only read-only `inspect_project`, `doctor`, and `prepare_run`; preparation delegates to `just --dry-run`. There is deliberately no execution or write tool. Tool results include both `structuredContent` and serialized text content for client compatibility, and annotations declare read-only/non-destructive behavior.

## Compatibility testing
Versioned JSON dump fixtures cover basic and rich `just 1.54.0` shapes. Provider mocks assert transport-specific request and response contracts. History tests cover migration defaults and storage invariants. Unix cancellation tests prove descendant termination. MCP tests cover initialization, notification silence, read-only discovery, and actual dry-run preparation. Layered CI verifies core, desktop, and MCP workspaces.

## Agent workflow
Code discovery and impact analysis use Codebase Memory MCP first. Structural changes require tests, formatting, Clippy, documentation/ADR updates, re-indexing, and graph verification.
