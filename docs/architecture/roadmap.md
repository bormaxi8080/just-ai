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
- dedicated layered CI workflow;
- local ADRs and Codebase Memory MCP ADR/index.

## Next increments

1. Add provider-specific Responses API and local-model adapters where their
   semantics differ from OpenAI-compatible chat completions.
2. Add process-group cancellation on Unix and job-object cancellation on
   Windows; migrate JSONL history to SQLite if querying requirements justify it.
3. Add parameter forms and persisted history to the desktop UI.
4. Expand JSON dump fixtures for modules, interpolations, shebangs, and
   platform-specific parameter kinds.
5. Add an optional daemon/MCP adapter only after core contracts stabilize.

Every increment follows `agent/commands/implement.md` and ends with a graph
refresh and architecture review.
