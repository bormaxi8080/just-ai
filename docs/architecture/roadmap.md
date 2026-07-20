# Implementation roadmap

## Completed foundation

- `just-ai` library with a two-line CLI binary adapter;
- independent domain risk and policy modules;
- two-phase prepare/execute API with preview revalidation and explicit
  confirmation types;
- direct argv execution without shell construction;
- atomic, optimistic-concurrency-protected proposal writes;
- provider subprocess arguments no longer contain credentials or prompt bodies;
- versioned product prompts and project-agent commands;
- separate Tauri 2 + React GUI with inspect, risk display, prepare, policy
  confirmation, live output events, execution cancellation, and output display;
- provider-neutral `AiProvider` boundary and native OpenAI-compatible adapter;
- strict JSON Schema validation for all current AI response contracts;
- physical separation of CLI, inspection, response contracts, proposal,
  provider, application, and domain modules;
- versioned JSON dump fixture and black-box CLI integration tests;
- allowlisted project scanner with per-file/total budgets and secret redaction;
- typed streaming run events and shared cancellation tokens in core;
- bounded, atomic, per-project JSONL run history with redacted output tails;
- recipe parameter forms and persisted run history in the desktop UI;
- versioned rich JSON dump coverage for nested modules, interpolations,
  shebangs, and singular/plus/star parameter kinds;
- versioned Windows JSON dump coverage for drive-letter paths, PowerShell/cmd
  bodies, interpolation, and nested modules;
- Unix process-group isolation and whole-tree cancellation for streaming runs;
- Windows Job Object ownership and whole-tree cancellation for streaming runs;
- backward-compatible history observability for argv, cancellation, timestamps,
  and expandable redacted output in the desktop UI;
- dedicated OpenAI Responses API adapter with strict Structured Outputs while
  retaining Chat Completions for Ollama and OpenAI-compatible servers;
- native Ollama `/api/chat` adapter with schema-format output, deterministic
  temperature, and explicit non-streaming transport;
- independent read-only MCP/stdio adapter for inspection, risk reports, and
  dry-run preparation, with no execution or write tools;
- MCP prompt discovery backed directly by the canonical project-agent files;
- allowlisted MCP resources for canonical architecture documentation and ADRs;
- black-box MCP stdio tests for framing, notification silence, and parse-error
  recovery;
- shared CLI/MCP `verify` agent command for layered quality gates;
- MCP prompt/resource catalog isolated from protocol transport and tool code;
- MCP read-only core tools isolated from protocol transport and dispatch;
- MCP newline transport isolated from JSON-RPC parsing and dispatch;
- JSON-RPC envelope validation that distinguishes notifications from malformed
  requests and preserves valid request identifiers in errors;
- bounded MCP stdio frames with recovery after oversized and non-UTF-8 input;
- server-controlled MCP `just` executable with no client-provided binary path;
- working-directory-confined MCP tools with no client-provided project root;
- dedicated layered CI workflow;
- local ADRs and Codebase Memory MCP ADR/index.

## Next increments

1. Add adapters for additional local-model runtimes only when their semantics
   differ materially from Ollama and OpenAI-compatible chat completions.
2. Migrate JSONL history to SQLite if querying requirements justify it.
3. Add platform fixtures only when upstream emits a materially different JSON
   shape on that platform.

Every increment follows `agent/commands/implement.md` and ends with a graph
refresh and architecture review.
