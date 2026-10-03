# ADR 0003: Prepare and execute recipes in two phases

- Status: accepted
- Date: 2026-07-19

## Decision

GUI execution is split into `prepare_run` and `execute_prepared_run`.
Preparation resolves a recipe, validates arguments, obtains a command preview,
calculates risk from the real dry-run stderr/stdout, evaluates project policy,
and returns a prepared snapshot. Execution prepares again and compares that
snapshot before accepting the required confirmation; it has no expiry token.
Execution streams typed events from a direct `just`
child process. The frontend cannot submit a shell command.

## Consequences

Policy cannot be bypassed by changing UI state after preview. CLI, GUI, and
future daemon integrations share the same execution semantics.
