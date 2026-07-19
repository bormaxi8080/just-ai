# Architecture review prompt

Review the proposed change against the ADRs and dependency rules. Use Codebase
Memory MCP to trace inbound and outbound dependencies. Report:

1. whether root `just` behavior changed;
2. whether an adapter leaked into core;
3. whether untrusted AI or UI data can reach execution or filesystem writes;
4. whether path confinement, concurrency, cancellation, and secret redaction
   are covered;
5. missing unit, contract, integration, and negative tests;
6. documentation and graph-index updates required.

Prefer concrete symbol paths and call traces over general advice.
