# just-ai architecture decisions

## Companion and module boundaries
The upstream `just` package remains behaviorally unchanged and has no dependency on just-ai. `just-ai` communicates through the public JSON dump and direct argv-based process execution. `lib.rs` is a small public composition API. Physical modules separate `cli`, `inspection`, `ai_responses`, `proposal`, `provider`, `application`, and `domain`. The Tauri/React GUI is a separate workspace adapter. HTTP daemon support is optional and deferred.

## AI safety, transport, and context
AI output is an untrusted typed proposal, never an executable action. Local deterministic code validates syntax, dependencies, risk, workspace paths, and reviewed content before atomic application. Providers implement `AiProvider`. The initial OpenAI-compatible provider uses native Rust HTTP with rustls and a global 120-second timeout. Message content is parsed to JSON, validated against a strict operation-specific JSON Schema, then deserialized. Project context is allowlist-only, bounded per file and total, excludes dotenv files, and redacts likely credential assignments.

## Execution and history
Execution is two-phase: prepare obtains preview, risk, and policy; execute re-prepares before typed confirmation. Frontends submit recipe names and argv arrays, never shell strings. Core emits typed events and supports cancellation. Tauri permits one active run. History is bounded, atomic, per-project JSONL behind `RunHistory` and retains only redacted 16 KiB output tails. `RunRecord::completed` is the shared CLI/GUI record-construction boundary. Project roots are canonicalized before deriving storage identity so relative and absolute paths share history. Tauri exposes bounded recent-history reads and persists GUI executions through the application history boundary.

## Desktop parameter contract
The desktop adapter renders parameters from `ContextParameter`; it does not parse justfiles. Singular values remain one argv element. Plus/star variadics accept one argv element per line, preserving spaces inside each argument. Defaults are initialized explicitly in the form, and plus/required parameters are validated before prepare.

## Compatibility testing
The JSON dump boundary has immutable, versioned fixtures parsed directly by inspection tests. The basic fixture covers recipes, dependencies, defaults, warnings, and risk derivation. The rich fixture pins the `just 1.54.0` contract for nested modules, structured interpolation bodies, shebangs, flags, and singular/plus/star parameters. Platform-only dump shapes remain a Windows-CI increment. Black-box CLI integration tests verify agent commands work without a justfile and operational commands fail cleanly when project discovery fails. History tests cover retention, missing storage, bounded/redacted output, shared record construction, and canonical project identity.

## Agent workflow
Code discovery and impact analysis use Codebase Memory MCP first. Structural changes require tests, formatting, Clippy, documentation/ADR updates, re-indexing, and graph verification.
