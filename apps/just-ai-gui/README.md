# just-ai GUI

This is a separate Tauri 2 + React adapter over the `just-ai` Rust library.
It does not execute shell strings and does not import code from the upstream
`just` crate. The current vertical slice discovers recipes, renders positional
and variadic parameter forms, presents local deterministic risk reports,
streams output, supports cancellation, and displays persisted run history.
History rows expose argv, cancellation status, timestamps, and redacted bounded
stdout/stderr tails for local diagnostics.

Variadic parameters use one argument per line so whitespace inside an argument
is preserved. History is owned by the Rust application layer; the GUI reads it
through a bounded IPC command and never accesses JSONL storage directly.

## Development

```sh
npm install
npm run tauri dev
```

The Rust adapter is in `src-tauri/src/lib.rs`. Keep commands thin: business
rules belong in `crates/just-ai` and frontend code only renders serializable
results.
