# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).


## [0.1.0] - 2026-10-08


### 🚀 Features

- feat(mcp): add read-only stdio adapter ([`0bf8b4d`](https://github.com/bormaxi8080/just-ai/commit/0bf8b4d68cb95f718e569d43afbfe4c7c8a4d0ee))

- feat(mcp): expose project agent prompts ([`65e0b1f`](https://github.com/bormaxi8080/just-ai/commit/65e0b1f7fb16956d344273b326718aea7b815dd7))

- feat(mcp): expose architecture resources ([`5b96dad`](https://github.com/bormaxi8080/just-ai/commit/5b96dad4914e5f27b88c108ca79bb2240bc53619))

- feat(agent): add layered verification command ([`431e4e6`](https://github.com/bormaxi8080/just-ai/commit/431e4e6e25e7075bc85af9602f4925bc30996cf4))

- feat(execution): cancel Windows process trees ([`2403a33`](https://github.com/bormaxi8080/just-ai/commit/2403a333c919b91e4b44de0ccee516c1f88a1934))

- feat(mcp): bound stdio input frames ([`87f6d0e`](https://github.com/bormaxi8080/just-ai/commit/87f6d0e38baaca897ab5c46513b468a1e59b67cd))


### 🐛 Bug Fixes

- fix(mcp): negotiate protocol versions ([`4e2ab81`](https://github.com/bormaxi8080/just-ai/commit/4e2ab81ff9aed237a769cde949ed26ebece6cd23))

- fix(mcp): validate JSON-RPC request envelopes ([`e1d1447`](https://github.com/bormaxi8080/just-ai/commit/e1d1447101364f5e2f1b763724c1ba940ce21dad))

- fix(mcp): prevent client-selected executables ([`9ffbcd7`](https://github.com/bormaxi8080/just-ai/commit/9ffbcd7b8f574d21ca167860934acab3919a0b22))

- fix(mcp): confine tools to server workspace ([`7b617b6`](https://github.com/bormaxi8080/just-ai/commit/7b617b6cb5f4a9f6f22af43422dbb8768a777028))

- fix(mcp): enforce tool argument allowlists ([`b2edf95`](https://github.com/bormaxi8080/just-ai/commit/b2edf95c16a0695699bd94874d80dbb31cc576e4))

- fix(execution): block evaluative dry-run inputs ([`45c1ef5`](https://github.com/bormaxi8080/just-ai/commit/45c1ef565dbb790c10ac5c2a86ba59c0628a35d4))

- fix(core): bound just subprocess output ([`509bc8e`](https://github.com/bormaxi8080/just-ai/commit/509bc8e8bcc0d46de5e57c2ce59575ed4cb3d4bc))

- fix(core): bound project file reads ([`7b8b6e9`](https://github.com/bormaxi8080/just-ai/commit/7b8b6e96186dd997a179252df0d06db2afe2bc25))

- fix: enforce real runner previews, project policy and redacted AI context ([`f089682`](https://github.com/bormaxi8080/just-ai/commit/f0896828a2f78e987f4291eb8fa6ffbc47325811))

- fix: isolate project history and make synchronous storage async-safe ([`bc87878`](https://github.com/bormaxi8080/just-ai/commit/bc878786d99fc3e5d0d91646d8f1803885b2c7af))

- fix: replace unsafe recipe merging with source-preserving deduplication plans ([`192a5b9`](https://github.com/bormaxi8080/just-ai/commit/192a5b97cec9b974319260f4f77eaf6bb76960c6))

- fix: share stored template plans and validate MCP write contracts ([`1b8a2af`](https://github.com/bormaxi8080/just-ai/commit/1b8a2af6613577841b13f2b41f6e7e8b38c673c3))

- fix: upgrade Codebase graph verification and resolve companion boundary calls ([`8601e15`](https://github.com/bormaxi8080/just-ai/commit/8601e1501a59239cf2a923d6e85c29263452aff4))

- fix: verify execution preparation paths across all companion adapters ([`2e26ab1`](https://github.com/bormaxi8080/just-ai/commit/2e26ab13d2eceffd941f77a7f63f1d24bc872f37))


### ♻️ Refactoring

- refactor(mcp): isolate prompt resource catalog ([`f71e473`](https://github.com/bormaxi8080/just-ai/commit/f71e47340a92bbd636035722af180394719e99df))

- refactor(mcp): isolate read-only tool adapter ([`dfbb49d`](https://github.com/bormaxi8080/just-ai/commit/dfbb49db75787f6ff5911af1ad4618cab7504b22))

- refactor(mcp): separate protocol dispatch from transport ([`56018b8`](https://github.com/bormaxi8080/just-ai/commit/56018b8c902a6c72abf237d94c2b325d5990e37a))

- refactor: share safe migration logic across CLI, MCP and desktop ([`b58b683`](https://github.com/bormaxi8080/just-ai/commit/b58b683cf2aeef149c1f15a93d139c3cc1c748d3))


### ✅ Tests

- test(mcp): verify stdio transport ([`dbb1fd7`](https://github.com/bormaxi8080/just-ai/commit/dbb1fd7f66262632e03049518a9444f6a8f66b5e))


### 🔀 CI/CD

- ci: update Cargo.lock for GUI and MCP crates

Fixes CI failure: 'the lock file needs to be updated but --locked was passed'

Both crates had outdated Cargo.lock files that didn't match Cargo.toml after dependency updates. ([`21e6547`](https://github.com/bormaxi8080/just-ai/commit/21e6547a4e8aa78f6e8e688c2580192f3a6ef583))


### Other

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

- Stage 4 implemented ([`8f08daf`](https://github.com/bormaxi8080/just-ai/commit/8f08daf16625e886599295a7898dbff2adc0f62c))

