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

## Review remediation completed 2026-10-04

Five separate implementation commits address the follow-up review:

- `f0896828`: real runner preview, project policy/provider configuration and
  redacted AI context across adapters.
- `bc878786`: project-scoped SQLite history, async-safe synchronous storage and
  recorded MCP executions. Existing shared SQLite data is left untouched because
  its records cannot safely be attributed to a project.
- `192a5b97`: source-preserving deduplication plans instead of semantic unions.
- `1b8a2af6`: shared stored-template instantiation, template-name validation,
  generated-write checks and runtime MCP argument schemas.
- `fe175e6a`: active/nested editor project selection, exact diagnostics, CLI
  preparation and editor policy confirmation, desktop worker isolation,
  portable CLI/MCP smoke tests and native desktop CI matrix.

Verification after these changes: 2,551 Rust tests passed, 19 ignored; workspace
Clippy with warnings denied passed; desktop Rust check, production frontend build
and five editor contract tests passed. The portable local-provider smoke test
verified CLI configuration, persisted MCP templates, history, blocked writes,
argument schemas and prompt redaction. A macOS debug app bundle was built and
its native window opened. This does not establish interactive desktop correctness
on all platforms. Linux and Windows checks are configured in CI but were not
executed natively during this remediation. Earlier container results above refer
to the preceding maintenance increment. No push or release was performed.

Remaining work includes effective wiring of all optional execution limits and
AI tuning settings, custom history redaction patterns, import-aware transformations
and unsaved editor analysis. Native GUI packaging still requires an available
upstream `just` executable; signing and release distribution were not tested.

The final full persistent Codebase index contains 7,060 nodes and 20,040 edges
and resolves the current TemplatePlan source. The installed server still indexes
generated editor `out` files despite `.cbmignore`; restrict symbol discovery to
source paths until server ignore support is reconciled. A ready index alone
therefore remains insufficient evidence of source freshness.
