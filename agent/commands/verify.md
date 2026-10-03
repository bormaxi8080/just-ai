# Verify a completed increment

Run checks from the repository root and stop at the first failure.

1. Check formatting and the core companion layer:

   ```sh
   cargo fmt --all -- --check
   cargo clippy --package just-ai --all-targets -- -D warnings
   cargo build --package just --package just-ai --package just-ai-mcp
   cargo test --package just-ai
   cargo test --package just-ai-lsp
   ```

2. Check the independent MCP adapter:

   ```sh
   cargo fmt --manifest-path apps/just-ai-mcp/Cargo.toml -- --check
   cargo clippy --manifest-path apps/just-ai-mcp/Cargo.toml --all-targets --locked -- -D warnings
   cargo test --manifest-path apps/just-ai-mcp/Cargo.toml --locked
   ```

3. Check the independent desktop adapter:

   ```sh
   npm --prefix apps/just-ai-gui run build
   cargo check --manifest-path apps/just-ai-gui/src-tauri/Cargo.toml --locked
   ```

4. Check the editor adapter:

   ```sh
   npm --prefix apps/just-ai-vscode run test:contracts
   ```

5. Run upstream `just` library tests and prove its tracked implementation was
   not changed by the increment:

   ```sh
   cargo test --lib
   git diff --exit-code upstream/master -- src
   git diff --check
   ```

6. Re-index Codebase Memory MCP. Use `get_architecture`, `search_graph`, and
   `trace_path` to confirm dependency direction, then update its ADR when the
   increment changed a contract or boundary.

Document any intentionally skipped platform-specific check and why it could
not run in the current environment.
