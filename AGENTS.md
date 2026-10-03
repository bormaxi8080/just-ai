# Maintaining just-ai

Read `agent/prompts/system.md`, `docs/architecture/README.md`, and the relevant
ADRs before changing behavior. Follow `agent/commands/verify.md` for checks.

Use Codebase Memory MCP for symbol discovery and call traces. Before trusting
the graph, call `list_projects` and check that the project's `root_path` matches
the current checkout. The project for this checkout is
`Volumes-Transcend-workspace-projects-just-ai`. Check `index_status`, search a
recently changed symbol, and confirm `get_code_snippet` returns its source.
If the path or source is stale, refresh this checkout with `index_repository`
(`mode=full`, `persistence=true`). Never trust a ready status alone. If MCP is
unavailable, state that limitation and inspect the checkout directly.

Preserve the upstream just implementation. Keep companion changes in their
own commits and upstream synchronization in a separate merge commit. Do not
publish releases or push unless requested. Refresh and verify the graph after
structural changes and before reporting completion.
