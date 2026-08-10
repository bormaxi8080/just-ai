# ADR 0004: Native provider transport and strict response contracts

- Status: accepted
- Date: 2026-07-19

## Context

The first implementation delegated HTTP to `curl`. Although request bodies and
credentials were moved out of argv, an external process complicated packaging,
error handling, timeouts, testing, and desktop distribution.

## Decision

Use a native blocking Rust HTTP adapter behind `AiProvider`. The initial
adapter targets OpenAI-compatible `/chat/completions` endpoints and uses rustls
through `ureq`. The existing OpenAI, Ollama, base URL, model, and API-key
environment contract remains stable.

Model message content is parsed as JSON, validated against an operation-specific
JSON Schema, and only then deserialized. Schemas reject unknown fields, invalid
risk values, missing properties, invalid recipe names, and empty recipe bodies.

## Consequences

- The CLI and desktop package no longer require `curl`.
- Provider behavior can be tested against a local mock HTTP server.
- Invalid model structures fail before proposal or presentation logic.
- Responses API or provider-specific transports can be added as separate
  adapters without changing application use cases.
