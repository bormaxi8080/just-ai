# Refresh the code knowledge graph

1. Resolve the actual checkout root; do not reuse another checkout's project ID.
   Call Codebase Memory MCP `index_repository` with this repository root,
   `mode=full`, and persistence enabled.
2. Wait for indexing to finish and check `index_status`.
3. Call `list_projects` and confirm the returned root path matches this checkout.
   Call `get_architecture` and confirm the expected crates and entry points,
   including `just-ai-lsp`. Search a changed symbol and read its source with
   `get_code_snippet`; a ready index with unavailable source is not healthy.
4. Store material architectural decisions using `manage_adr`.
