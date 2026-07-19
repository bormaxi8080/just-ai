# Refresh the code knowledge graph

1. Call Codebase Memory MCP `index_repository` with this repository root,
   `mode=full`, and persistence enabled.
2. Wait for indexing to finish and check `index_status`.
3. Call `get_architecture` and confirm the expected crates and entry points.
4. Store material architectural decisions using `manage_adr`.
