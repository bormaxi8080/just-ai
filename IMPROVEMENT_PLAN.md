# just-ai maintenance plan

Updated 2026-10-03. This replaces the earlier speculative feature matrix with
verified maintenance status. See `docs/architecture/roadmap.md` for product work.

## Completed maintenance

1. Restore Codebase Memory MCP at the current checkout root; unify maintainer
   prompts and Cursor rules through `AGENTS.md` and verify source availability.
2. Validate modularization in staging, refuse existing module files, preserve
   comments/attributes, coordinate companion writers and roll back new files
   on returned errors. Reuse this plan in CLI, MCP and desktop.
3. Align VS Code with companion CLI and JSON contracts, handle spawn errors,
   honor workspace trust, fix UTF-16 LSP positions and refresh disk analysis.
4. Correct workspace artifact paths, release archive commands, locked Docker
   builds and desktop executable entry point. Keep upstream out of companion
   release-plz processing; desktop remains an independent workspace.
5. Synchronize with casey/just master `602ee328` in a dedicated merge commit.
   Root `src/` is identical to upstream; the published release is 1.58.0.
6. Verify Rust tests/Clippy, editor contracts, desktop targets, frontend,
   packaging scripts and container smoke tests. Platform evidence is recorded
   in the maintenance report; checks are not implied for untested platforms.
7. Stop tracking node_modules/editor build output, retain lockfiles, share
   migration logic and refresh documentation and the Codebase graph.

## Remaining product work

- Import-aware multi-file migration with durable crash recovery. Current
  modularization refuses existing imports/modules without changes; its writer
  lock coordinates companion processes, not arbitrary editors.
- Analyze unsaved LSP buffers and implement variable/import semantics.
- Extend regression coverage for template persistence, provider streaming and
  independently confirmed platform-specific behavior.
- Test packaged desktop windows and user interactions on each supported OS.
- Revisit tool schema validation and side-effect annotations as MCP tools grow.

Use `agent/commands/implement.md` for increments and
`agent/commands/verify.md` for checks. Do not mark planned features completed
without tests or a recorded verification limitation.
