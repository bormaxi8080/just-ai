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

Execution loads project capture limits, queue capacity and cancellation polling.
An output idle timeout terminates the isolated process tree; zero disables it
for intentionally quiet long-running recipes. Capture limits cannot exceed the
8 MiB hard ceiling. CLI flags/environment override the configured runner; MCP
continues to use its server-controlled binary.

The main desktop window has explicit Tauri capabilities for event listen and
unlisten. Run stdout/stderr arrives through that subscription while the process
is active. Custom command IPC alone does not grant core event permissions.
Packaged Linux smoke exercises this real event path, unlike renderer mocks.
Desktop Docker builds enable `tauri/custom-protocol` to embed frontend assets.
