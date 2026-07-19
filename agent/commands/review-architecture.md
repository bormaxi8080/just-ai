# Review architecture

Use `get_architecture`, `search_graph`, and `trace_path` for the indexed
project. Verify that the root `just` crate has no dependency on `just-ai`, core
has no adapter dependencies, AI output only reaches proposal validation, and
execution only receives typed recipe names and argv. Run tests and document
every violation with a source location and remediation.
