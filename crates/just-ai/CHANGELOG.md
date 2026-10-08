# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).


## [0.1.0] - 2026-10-08


### 🚀 Features

- feat: establish layered just-ai architecture ([`a8bc2ab`](https://github.com/bormaxi8080/just-ai/commit/a8bc2ab32d521aed0818b1ee521f62232490104d))

- feat(gui): add recipe arguments and run history ([`79ff26d`](https://github.com/bormaxi8080/just-ai/commit/79ff26dfd676161e837dac7a503a05addacdcec8))

- feat(execution): cancel Unix process trees ([`7fe656c`](https://github.com/bormaxi8080/just-ai/commit/7fe656c85fc939c4e7f54db04f8e7a95c140fcc7))

- feat(history): expose cancellable run diagnostics ([`a754d98`](https://github.com/bormaxi8080/just-ai/commit/a754d98babf12db3b2602e6e374429ec9fc941a6))

- feat(provider): add OpenAI Responses adapter ([`59ae792`](https://github.com/bormaxi8080/just-ai/commit/59ae792ed6f71d2c67e0e228f5e77f2c50bb48ab))

- feat(provider): add native Ollama adapter ([`1ae4881`](https://github.com/bormaxi8080/just-ai/commit/1ae48810ef613b814008ea78b57cc29f1ff64202))

- feat(mcp): add read-only stdio adapter ([`0bf8b4d`](https://github.com/bormaxi8080/just-ai/commit/0bf8b4d68cb95f718e569d43afbfe4c7c8a4d0ee))

- feat(agent): add layered verification command ([`431e4e6`](https://github.com/bormaxi8080/just-ai/commit/431e4e6e25e7075bc85af9602f4925bc30996cf4))

- feat(execution): cancel Windows process trees ([`2403a33`](https://github.com/bormaxi8080/just-ai/commit/2403a333c919b91e4b44de0ccee516c1f88a1934))

- feat(config): add complete configuration system

- Add Config, HistoryConfig, ExecutionConfig, RiskConfig, PolicyConfig, AiConfig,
  ScannerConfig, BoundedFileConfig, McpConfig structs with TOML loading
- Wire HistoryConfig into history module (JsonLineHistory, RunRecord, project_history_path)
- Wire ExecutionConfig into execution module (RecipeExecutor, stream_queue_capacity,
  cancellation_poll_interval, max_capture_bytes)
- Wire RiskConfig into RiskFinding.scan_line with custom rule support
- Wire PolicyConfig into DefaultPolicy.evaluate with custom decisions
- Wire ScannerConfig into ProjectScanner with file/total budgets and allowlist
- Wire BoundedFileConfig into bounded_file module (max_editable_file_bytes, prefix_limit)
- Update CLI to load config via Config::load() for history path/params
- Add example just-ai.toml with all sections documented
- All 64 tests pass ([`9bf6a4f`](https://github.com/bormaxi8080/just-ai/commit/9bf6a4f9ac5ee2acf9a3c6ab4c1f1bd4ab72fa6a))

- feat(history): migrate JSONL history to SQLite backend

- Added HistoryBackend enum (jsonl|sqlite) with sqlite as default
- Created SqliteHistory implementation with full RunHistory trait
- Added query() method for filtering by recipe/success
- Added create_history() factory function for backend selection
- Updated CLI to use create_history() for backend-agnostic access
- Updated GUI to use create_history() for recent_runs and execute_run
- Added HistoryConfig options: backend, database_file
- Added sqlx, tokio(rt-multi-thread), chrono, uuid dependencies
- Added 2 new tests for JSONL query filtering
- All 66 tests pass ([`4fde9ea`](https://github.com/bormaxi8080/just-ai/commit/4fde9ea5bcfc96456bfe23957b29dd2b1d6b1cf6))

- feat(provider): add streaming support for AI provider responses

- Added AiStream type alias for streaming responses
- Added complete_stream() method to AiProvider trait with default implementation
- Implemented streaming for OpenAI Responses API (SSE format with output_text chunks)
- Implemented streaming for Ollama (native SSE format with message/content)
- Implemented streaming for OpenAI-Compatible (SSE format with choices/0/delta/content)
- Added futures and async-stream dependencies
- All 66 tests pass ([`a21c865`](https://github.com/bormaxi8080/just-ai/commit/a21c865bc5e119f3489523adb17433968fc92781))


### 🐛 Bug Fixes

- fix(execution): terminate just options before recipe argv ([`2a23b1c`](https://github.com/bormaxi8080/just-ai/commit/2a23b1c0977aca082d9db34af1bb53860d8f4c66))

- fix(execution): block evaluative dry-run inputs ([`45c1ef5`](https://github.com/bormaxi8080/just-ai/commit/45c1ef565dbb790c10ac5c2a86ba59c0628a35d4))

- fix(core): bound just subprocess output ([`509bc8e`](https://github.com/bormaxi8080/just-ai/commit/509bc8e8bcc0d46de5e57c2ce59575ed4cb3d4bc))

- fix(execution): bound recipe output collection ([`0342fac`](https://github.com/bormaxi8080/just-ai/commit/0342fac17060dbc0c3a714c594bf74d104b8c88c))

- fix(core): bound project file reads ([`7b8b6e9`](https://github.com/bormaxi8080/just-ai/commit/7b8b6e96186dd997a179252df0d06db2afe2bc25))

- fix(history): bound JSONL reads and retention ([`7013be3`](https://github.com/bormaxi8080/just-ai/commit/7013be3f0eb29b97e15c7f8fbe3064e32719ea11))

- fix: validate modularization plans and prevent destructive file overwrites ([`bae7910`](https://github.com/bormaxi8080/just-ai/commit/bae7910db9c858ee585e5c1747f8b77ac5354b18))

- fix: align editor adapters with core contracts and UTF-16 positions ([`8a24fc2`](https://github.com/bormaxi8080/just-ai/commit/8a24fc2537c1094e11cf86e60deb4db2a6db26ec))

- fix: enforce companion run policy in editor and cover validation rollback ([`2dd06b0`](https://github.com/bormaxi8080/just-ai/commit/2dd06b0535b5ae9c3e836abc225662d08fb4ae8f))

- fix: enforce real runner previews, project policy and redacted AI context ([`f089682`](https://github.com/bormaxi8080/just-ai/commit/f0896828a2f78e987f4291eb8fa6ffbc47325811))

- fix: isolate project history and make synchronous storage async-safe ([`bc87878`](https://github.com/bormaxi8080/just-ai/commit/bc878786d99fc3e5d0d91646d8f1803885b2c7af))

- fix: replace unsafe recipe merging with source-preserving deduplication plans ([`192a5b9`](https://github.com/bormaxi8080/just-ai/commit/192a5b97cec9b974319260f4f77eaf6bb76960c6))

- fix: share stored template plans and validate MCP write contracts ([`1b8a2af`](https://github.com/bormaxi8080/just-ai/commit/1b8a2af6613577841b13f2b41f6e7e8b38c673c3))

- fix: scope editor commands and verify native companion contracts in CI ([`fe175e6`](https://github.com/bormaxi8080/just-ai/commit/fe175e6a2b5f31ca1b97029a04958716b14cb390))

- fix: apply project execution, provider tuning and history redaction settings ([`2eed4ee`](https://github.com/bormaxi8080/just-ai/commit/2eed4eedc954e273f08c5e85df658f9bae647cea))

- fix: upgrade Codebase graph verification and resolve companion boundary calls ([`8601e15`](https://github.com/bormaxi8080/just-ai/commit/8601e1501a59239cf2a923d6e85c29263452aff4))


### ♻️ Refactoring

- refactor: share safe migration logic across CLI, MCP and desktop ([`b58b683`](https://github.com/bormaxi8080/just-ai/commit/b58b683cf2aeef149c1f15a93d139c3cc1c748d3))


### ✅ Tests

- test: expand just dump compatibility coverage ([`4771278`](https://github.com/bormaxi8080/just-ai/commit/4771278ffd9568c29c517b8f0a1c12076326b703))

- test(inspection): add Windows dump fixture ([`6ac1ee2`](https://github.com/bormaxi8080/just-ai/commit/6ac1ee20a28aed2d04d323603d74dce9708ec011))

- test(execution): remove overflow process race ([`02945e0`](https://github.com/bormaxi8080/just-ai/commit/02945e09e2486d2edc8ee6f4ddbb5acda25dcd77))


### 🔀 CI/CD

- ci: fix clippy warnings and flaky test

- apps/just-ai-gui/src-tauri/src/lib.rs: remove unused SqliteHistory import, prefix unused project_root with _
- crates/just-ai/src/config.rs: add #[derive(Default)] to HistoryBackend enum
- src/alias.rs: replace assert!(a == b) with assert_eq!(a, b)
- crates/just-ai/src/application/execution.rs: add small sleep to avoid 'Text file busy' flaky test on CI

All tests pass locally (1594 workspace + 66 just-ai), clippy clean, fmt clean. ([`670518a`](https://github.com/bormaxi8080/just-ai/commit/670518aa2a5b0372d54e76c0d02c8a977c1b1e78))

- ci: adjust sqlx features to reduce transitive deps

- Switch from 'runtime-tokio-native-tls' to 'runtime-tokio' + 'tls-rustls'
- This avoids pulling in rsa (via native-tls -> ring -> rsa) and others
- Note: sqlx 0.9 requires Rust 1.94, staying on 0.8 for Rust 1.89 compat

All tests pass, clippy clean. ([`ccf4aa6`](https://github.com/bormaxi8080/just-ai/commit/ccf4aa62ee644294ad0abf7d2dab6e4ef228b540))


### Other

- Add just-ai companion prototype ([`0aaff24`](https://github.com/bormaxi8080/just-ai/commit/0aaff24b9e7ffabd6c29c4bd100d60a582c29447))

- Update specific README blocks ([`0a9e634`](https://github.com/bormaxi8080/just-ai/commit/0a9e634bf2cdd1d928175263db72a377b36b3591))

- P4: Complete Dockerfiles and release artifacts

- Dockerfile.cli: Multi-stage CLI build (working, tested)
- Dockerfile.mcp: Multi-stage MCP build (working, tested with ping/tools)
- Dockerfile.gui: Multi-stage Tauri GUI build (built successfully)
- Dockerfile.dev: Development container with all tooling
- docker-compose.yml: Service orchestration for CLI, MCP, dev
- .dockerignore: Optimized build context
- .github/workflows/ci.yml: Complete CI pipeline (check, test, build, docker, audit, release)
- apps/just-ai-gui/src-tauri/src/lib.rs: Fix private field import

All 3 Docker images build and run correctly.
All cargo tests pass (1594 in workspace + 66 in just-ai). ([`f233b80`](https://github.com/bormaxi8080/just-ai/commit/f233b80ad74fa80475d79a3bf005d6c628122281))

- P1: Implement P1 improvements

- P1.1: Add `just-ai fix <recipe>` command for AI-assisted recipe fixes
- P1.2: Add `--interactive` flag to `run` command with TUI prompts (dialoguer)
- P1.3: Grouped recipe insertion by dependencies/naming patterns
- P1.4: VSCode extension with risk diagnostics, history panel, commands
- P1.5: MCP tools: run_recipe, get_history, add_recipe, fix_recipe
- P1.6: `just-ai history migrate` for JSONL -> SQLite conversion
- P1.7: Provider adapters: Anthropic, Azure OpenAI, Google Gemini
- P1.8: Config commands: validate, schema generation with schemars ([`3b15d6f`](https://github.com/bormaxi8080/just-ai/commit/3b15d6f50f94101a2bd165666afa003971151b23))

- P2.3 + P4: Multi-recipe AI workflows, batch operations, templates, and composition ([`ef3834d`](https://github.com/bormaxi8080/just-ai/commit/ef3834d4ee88c33055e4cb2115d2390d777ff635))

- P5.1: Implement migrate/refactor command with dead code analysis and dependency graph

- Add migrate CLI command with three subcommands:
  - analyze - Detect unreferenced/isolated recipes, dependency cycles, depths, and similar recipes
  - modularize - Group recipes by common prefix for module organization (analysis only)
  - deduplicate - Find and optionally merge similar/duplicate recipes (interactive or auto)

- Add dead code detection functions in inspection.rs:
  - find_unreferenced_recipes() - recipes never used as dependencies
  - find_isolated_recipes() - recipes with no deps and no dependents

- Add dependency graph analysis functions in inspection.rs:
  - detect_cycles() - Find circular dependencies using DFS
  - calculate_dependency_depths() - Longest path from root for each recipe

- Add similarity detection:
  - find_similar_recipes() - Use TextDiff to find potential duplicates

All tests pass (61), cargo fmt clean, clippy clean. ([`54b072b`](https://github.com/bormaxi8080/just-ai/commit/54b072bbff435f35d8c818a978d7b83fdbff378b))

- P5.1: Add tests for migrate functionality

- Add test fixtures for migration analysis:
  - just-dump-migrate.json - full project with dependencies for testing analysis
  - just-dump-cycle.json - circular dependency detection
  - just-dump-similar.json - duplicate recipe detection

- Add unit tests in inspection.rs:
  - parses_versioned_migrate_fixture - tests unreferenced/isolated recipes, cycles, depths, similarity
  - detects_cycles - tests circular dependency detection with normalization
  - finds_similar_recipes - tests text similarity detection (100% for identical, <80% for different)

- Fix calculate_dependency_depths() to use BFS from roots (corrected depth calculation)

All 64 tests pass, cargo fmt clean, clippy clean. ([`0db76fd`](https://github.com/bormaxi8080/just-ai/commit/0db76fd6acc94d4fd874bab7ae845a17ce45d58c))

- P5.2: Implement modularize action with module file creation

- Enhanced modularize_project in cli.rs to:
  - Group recipes by common prefix (e.g., test-, build-, deploy-)
  - Create .just module files for each group with 2+ recipes
  - Extract recipes from root justfile and move to module files
  - Add import statements at top of root justfile
  - Validate final justfile with just --dump

- Added new helper functions:
  - extract_recipe() - extracts a recipe definition with its body
  - add_imports_at_top() - inserts import statements after existing imports

- Added test fixture just-dump-modularize.json with 8 recipes for testing
- Added test parses_versioned_modularize_fixture() for grouping logic

All 65 tests pass, cargo fmt clean, clippy clean. ([`234730f`](https://github.com/bormaxi8080/just-ai/commit/234730f4c069aedf3a2177705e11fe0986bcd3aa))

- P5.3: Improve deduplicate action with smart merge

- Enhanced deduplicate_project in cli.rs:
  - Added --merge flag for smart merging instead of removing one recipe
  - Added interactive option 'm' for smart merge (combines unique parts of both recipes)
  - Smart auto-merge when --write --merge used together

- Added smart_merge_recipes() function that:
  - Uses shorter name (more generic)
  - Merges docs (prefers longer)
  - Unions parameters (prefers ones with defaults)
  - Unions dependencies
  - Combines unique body lines from both recipes (deduplicates identical lines)

- Added test smart_merge_recipes_preserves_unique_body_lines for similarity detection

All 66 tests pass, cargo fmt clean, clippy clean. ([`a6c53fc`](https://github.com/bormaxi8080/just-ai/commit/a6c53fcf41c589f006e54ce5e3a82930583059fe))

- Stage 4 implemented ([`8f08daf`](https://github.com/bormaxi8080/just-ai/commit/8f08daf16625e886599295a7898dbff2adc0f62c))

- P2.4: Tauri GUI Config Validation & Schema commands

- Added ai_config_validate command - validates just-ai.toml configuration
- Added ai_config_schema command - outputs JSON Schema for just-ai.toml
- Made find_config public in config module for Tauri backend access
- Added schemars dependency to Tauri Cargo.toml
- Added TypeScript types and API functions: ConfigValidationResult, ConfigSchemaResult, aiConfigValidate, aiConfigSchema
- Added UI buttons for Validate Config and Config Schema in Utility panel
- Added result display components: ConfigValidationResult (shows valid/invalid with errors), ConfigSchemaResult (pretty-prints JSON schema)
- All tests pass, cargo fmt and clippy clean ([`1d4dac3`](https://github.com/bormaxi8080/just-ai/commit/1d4dac3fbba50085df23128013c8d6e409ad8326))

- P3.1: Template Persistence - Store templates in .just-ai/templates/

- Added TemplateProposal serialization to JSON files in .just-ai/templates/
- Added save_template(), load_template(), list_templates(), delete_template() functions
- Modified handle_template() to persist templates to disk
- Modified handle_instantiate_template() to load templates from disk
- Added CLI commands: template-list, template-delete
- Updated instantiate-template to use stored templates instead of re-generating
- All tests pass, cargo fmt and clippy clean ([`d3ea85d`](https://github.com/bormaxi8080/just-ai/commit/d3ea85d31181571dbcc3d0b5391dec9c56e56673))

- Phase 3.2 + P2.3: Built-in Template Library and Multi-Recipe AI Workflows ([`ffc64fb`](https://github.com/bormaxi8080/just-ai/commit/ffc64fbca7fd31e749be8aafdbc7e67eafb015f9))

