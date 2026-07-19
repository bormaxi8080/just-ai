# ADR 0002: AI is a proposal generator, never an executor

- Status: accepted
- Date: 2026-07-19

## Decision

Model output is parsed into typed proposal objects. Deterministic local code
validates names and dependencies, renders syntax, validates it with `just`,
performs local risk analysis, and shows a diff. Applying and running are
separate user-authorized operations.

Provider calls receive allowlisted, size-bounded, redacted project context.
Provider credentials must not be placed in subprocess arguments.

## Consequences

Prompt injection cannot directly trigger execution. Invalid or blocked output
fails closed. Provider implementations can be replaced without changing the
application use cases.
