# ADR 0001: Keep just, just-ai, and GUI as separate layers

- Status: accepted
- Date: 2026-07-19

## Context

The repository began as a fork of `just` with a companion binary implemented
in one Rust file. AI analysis and a desktop interface require more state and
security policy than the task runner itself should own.

## Decision

The original `just` package remains behaviorally unchanged. `just-ai` talks to
it through `just --dump --dump-format json` and explicit subprocess arguments.
Reusable behavior lives in a transport-independent Rust library. The CLI and a
separate Tauri/React GUI are adapters. An HTTP daemon is optional and will only
be introduced for headless editor or agent integrations.

## Consequences

- A valid justfile remains usable without `just-ai`.
- There is one deterministic policy and validation implementation for all UIs.
- Desktop IPC is sufficient for the initial GUI; no localhost server is needed.
- The JSON dump is a compatibility boundary and needs fixture tests.
- Some information absent from the dump requires a small source metadata
  reader, but `just-ai` will not implement another just parser.
