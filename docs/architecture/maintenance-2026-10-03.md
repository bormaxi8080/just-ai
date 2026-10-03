# Maintenance verification — 2026-10-03

The fork was synchronized with casey/just master
`602ee328c9b21361121b498674334012175d4d81` in merge commit `10445d25`.
The latest published upstream release at verification time was 1.58.0.
`git diff --exit-code upstream/master -- src` passes. Companion changes are
kept outside the upstream implementation.

## Changes

- Corrected the Codebase Memory root and maintainer prompts; persisted the
  shared graph and verified source snippets from the current checkout.
- Replaced destructive modularization with staged validation, conflict
  refusal, cooperative writer locking and rollback on returned write errors.
  CLI, MCP and desktop now share the migration implementation.
- Corrected editor CLI/JSON contracts, process error handling, workspace trust
  and recipe execution through the companion risk policy. LSP uses UTF-16
  positions and refreshes analysis when source files change on disk.
- Repaired locked Docker builds, desktop entry point and release packaging;
  removed unreachable duplicate release handling and tracked dependencies.
- Updated architecture and roadmap documents to describe actual behavior.

## Verification

- Full locked workspace tests on macOS ARM64: 2,543 passed, 19 ignored.
  The built upstream `just` was on PATH during integration tests.
- Workspace Clippy with warnings denied, Rust formatting and `bin/forbid` pass.
- Desktop Rust targets compile on macOS; frontend production build passes.
- VS Code TypeScript compile and all three process/contract tests pass,
  including literal argv transport and rejection in untrusted workspaces.
- Linux ARM64 CLI, MCP and desktop release Docker images build. Desktop runtime
  dynamic libraries resolve without missing dependencies. CLI help and bundled
  `just --version` work; MCP initialize/tools-list exchange returns 16 tools.
- Linux and Windows release archive commands were checked with dummy binaries;
  this verifies filenames and layout, not native executable behavior.

## Limits and follow-up

- Native Windows and Linux AMD64 execution were not tested in this session.
  Packaged desktop windows and interactive UI behavior still need OS testing.
- Modularization refuses projects already containing imports/modules. The
  writer lock coordinates companion processes; it does not coordinate other
  editors or provide crash-atomic recovery. See ADR 0006.
- LSP semantic analysis uses saved files; unsaved text and richer import/variable
  semantics remain product work.
- The imported upstream `actions.lock` describes upstream workflows, not all
  fork actions. No workflow currently consumes it. Refresh it with the upstream
  `gh actions-lock` tooling when that tooling is available; GitHub CLI 2.102.0
  does not provide this command and the referenced documentation returns 404.
- Commits and container tags are local; nothing was pushed or published.
