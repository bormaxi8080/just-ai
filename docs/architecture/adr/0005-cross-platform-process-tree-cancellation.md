# ADR 0005: Cancel the complete recipe process tree on every desktop platform

- Status: accepted
- Date: 2026-07-19

## Decision

Streaming execution owns a platform-specific `ProcessTree` for the lifetime of
the child process. Unix starts `just` in an isolated process group and signals
that group. Windows creates a private Job Object with
`JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE`, assigns the spawned `just` process, and
uses `TerminateJobObject` when cancellation is requested. Descendants inherit
the process group or Job Object by default.

If Windows job assignment fails, `just-ai` kills and waits for the direct child
before returning the error. This prevents a partially supervised recipe from
continuing after the application reports that startup failed.

## Consequences

Cancellation closes stdout and stderr inherited by descendant processes, so
the streaming loop cannot hang waiting for orphaned pipe handles. Windows CI
executes a descendant-cancellation integration test. Other non-Unix platforms
retain direct-child cancellation until they receive a native tree primitive.
