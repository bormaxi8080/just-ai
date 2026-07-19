# just-ai GUI

This is a separate Tauri 2 + React adapter over the `just-ai` Rust library.
It does not execute shell strings and does not import code from the upstream
`just` crate. The initial vertical slice discovers recipes and presents their
local deterministic risk reports.

## Development

```sh
npm install
npm run tauri dev
```

The Rust adapter is in `src-tauri/src/lib.rs`. Keep commands thin: business
rules belong in `crates/just-ai` and frontend code only renders serializable
results.
