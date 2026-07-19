# Implement an increment

1. Read `docs/architecture/README.md` and applicable ADRs.
2. Query Codebase Memory MCP for the affected symbols and trace their callers.
3. Declare the dependency boundary and observable behavior being preserved.
4. Add or update tests that express the contract.
5. Implement the smallest complete vertical increment.
6. Run `cargo fmt --check`, package Clippy, and relevant tests.
7. Inspect `git diff` for accidental changes to upstream `just`.
8. Re-index Codebase Memory MCP and verify dependency direction.
9. Update docs and ADRs when contracts or boundaries changed.
