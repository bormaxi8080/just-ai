# Agent layer

This directory contains reusable prompts and command playbooks for maintaining
`just-ai`. They are intentionally documentation-first and vendor-neutral.

Agents must consult Codebase Memory MCP before code discovery, preserve the
upstream `just` implementation, work in small verified increments, and update
architecture records when a dependency boundary changes.

Commands are in `agent/commands/`; prompt fragments are in `agent/prompts/`.
`verify.md` is the canonical layered verification contract exposed by both the
`just-ai agent verify` CLI command and the MCP prompt catalog.
