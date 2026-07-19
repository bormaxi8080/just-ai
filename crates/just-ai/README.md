# just-ai

`just-ai` is an opt-in companion binary for `justfile` analysis and future AI
workflows. It does not replace `just`, does not change how recipes run, and is
not required by projects that only want to use `just`.

The deterministic parts are offline:

- it reads project data from `just --dump --dump-format json`;
- it exports compact context for AI tools;
- it performs local risk scoring for recipe command bodies;
- it validates generated recipes before writing them.

AI commands are opt-in and require provider configuration. They send the compact
project context to the configured OpenAI-compatible chat completions endpoint.

## Relationship to `just`

`just` remains the task runner:

```sh
just test
just build
just --list
```

`just-ai` is a companion tool:

```sh
just-ai doctor
just-ai run test
just-ai run --yes container-build
just-ai run --confirm "run deploy" deploy
just-ai history
just-ai history --json --limit 50
just-ai suggest
just-ai explain test
just-ai add "run tests with coverage"
just-ai export-context --pretty
just-ai agent implement
just-ai agent review-architecture
```

Removing `just-ai` from a machine does not affect a valid `justfile`. Recipes
continue to run through `just` exactly as before.

## Requirements

- Rust 1.89.0 or newer, matching the workspace `rust-version`.
- A `just` binary available in `PATH`, or an explicit path passed with
  `--just-binary`.

On macOS with Homebrew-managed `rustup`, a typical setup is:

```sh
brew install rustup
rustup toolchain install 1.89.0
rustup default 1.89.0
rustup component add rustfmt clippy
```

If Homebrew installed `rustup` as a keg-only formula, add it to your shell path:

```sh
export PATH="/opt/homebrew/opt/rustup/bin:$PATH"
```

## Build

From the repository root:

```sh
cargo build --package just-ai
```

If you want to run `just-ai` against the local development build of `just`, also
build the main binary:

```sh
cargo build --bin just
```

## Architecture

The binary is a thin adapter over the `just-ai` Rust library. Deterministic
domain rules and application use cases are shared with the separate Tauri GUI
in `apps/just-ai-gui`. The upstream `just` package has no dependency on either
layer and remains the source of truth for parsing and execution.

See [`docs/architecture/README.md`](../../docs/architecture/README.md) and the
ADRs in that directory before changing module boundaries.

## Agent commands

Versioned maintenance playbooks are shipped with the binary and can be printed
without discovering a justfile or contacting an AI provider:

```sh
just-ai agent system-prompt
just-ai agent implement
just-ai agent review-architecture
just-ai agent refresh-index
```

Their editable sources live in `agent/`. They require Codebase Memory MCP for
code discovery and architectural impact checks.

The current implementation status and subsequent increments are tracked in
[`docs/architecture/roadmap.md`](../../docs/architecture/roadmap.md).

AI context scanning is allowlist-based and bounded. Dotenv files are never
read by the scanner, likely credential assignments are redacted, and the
exported context reports truncation and redaction counts. Run history is local,
per-project, bounded to 500 records, and stores only redacted output tails.

## Commands

### AI provider configuration

AI commands use native Rust transports selected behind one provider boundary:
OpenAI Responses, Ollama Chat, or generic OpenAI-compatible Chat Completions.
No provider path spawns an external HTTP command.

OpenAI:

```sh
export JUST_AI_PROVIDER=openai
export JUST_AI_MODEL=gpt-5.6-terra
export JUST_AI_API_KEY=...
```

The `openai` provider uses the native Responses API with strict Structured
Outputs. The response contract for each operation is sent as `text.format`
JSON Schema and is validated again locally before deserialization. The default
uses Terra with `reasoning.effort=none` to preserve the former mini model's
balanced cost/latency role; override `JUST_AI_MODEL` for quality-first routing.

Ollama:

```sh
export JUST_AI_PROVIDER=ollama
export JUST_AI_BASE_URL=http://localhost:11434
export JUST_AI_MODEL=llama3.1
```

The `ollama` provider uses Ollama's native `/api/chat` endpoint, disables its
default response streaming, sends the operation JSON Schema through `format`,
and uses temperature zero for deterministic structured output. Local Ollama
requires no API key; `JUST_AI_API_KEY` is forwarded for authenticated remotes.

Generic OpenAI-compatible endpoint:

```sh
export JUST_AI_PROVIDER=openai-compatible
export JUST_AI_BASE_URL=https://api.example.com/v1
export JUST_AI_MODEL=...
export JUST_AI_API_KEY=...
```

`openai-compatible` retains the Chat Completions transport for servers that do
not implement either native Ollama or OpenAI Responses semantics.

`JUST_AI_API_KEY` is required unless `JUST_AI_PROVIDER=ollama` is used.

### `export-context`

Exports a compact JSON document intended for AI tools and editor integrations:

```sh
just-ai export-context
just-ai export-context --pretty
```

Use a specific `just` binary:

```sh
just-ai --just-binary ./target/debug/just export-context --pretty
```

The same path can be configured through the environment:

```sh
JUST_AI_JUST_BINARY=./target/debug/just just-ai export-context --pretty
```

The output contains:

- `modules`: discovered just modules, source path, and recipe count;
- `recipes`: recipe metadata, parameters, dependencies, body lines, and risk
  findings;
- `warnings`: warnings emitted by the `just` analyzer.

Example shape:

```json
{
  "modules": [
    {
      "module_path": "",
      "recipe_count": 2,
      "source": "/repo/justfile"
    }
  ],
  "recipes": [
    {
      "name": "test",
      "namepath": "test",
      "body": ["cargo test"],
      "dependencies": [],
      "parameters": [],
      "risk": "low",
      "risks": []
    }
  ],
  "warnings": []
}
```

Expression fragments from the `just` JSON dump are rendered as placeholders,
for example `{{variable:...}}`. This keeps exported context compact and avoids
pretending that dynamic expressions are static shell text.

### `doctor`

Analyzes recipes and prints a human-readable risk report:

```sh
just-ai doctor
```

Example:

```text
Analyzed 41 recipes: 35 low, 4 medium, 2 high, 0 blocked.

publish [high]
  - recursively removes files: `rm -rf tmp/release`
  - changes remote git state: `git push origin $VERSION`
```

Emit JSON instead:

```sh
just-ai doctor --json
```

`doctor` exits with a non-zero status only when it finds `blocked` risk. Medium
and high findings are warnings because many real project workflows legitimately
install tools, push branches, clean temporary directories, or build containers.

### `suggest`

Asks the configured AI provider to recommend useful missing recipes:

```sh
just-ai suggest
```

The model receives the exported project context and must return strict JSON.
`just-ai` prints the recommendations, proposed command bodies, rationale, and
expected risk. It does not write files.

### `explain`

Asks the configured AI provider to explain one recipe:

```sh
just-ai explain test
just-ai explain module:recipe
```

The output includes a summary, plain-language explanation, parameters,
dependencies, and risk notes.

### `add`

Asks the configured AI provider to propose one new recipe:

```sh
just-ai add "run tests with coverage"
```

By default this is a dry run. `just-ai`:

1. sends compact context and the user request to the provider;
2. parses the provider response as strict JSON;
3. renders one recipe;
4. rejects duplicate recipe names and missing dependency recipes;
5. validates the proposed `justfile` with `just --dump --dump-format json`;
6. runs local risk scoring on the proposed body;
7. prints a diff.

Apply the generated recipe:

```sh
just-ai add "run tests with coverage" --write
```

`add --write` refuses to write recipes with `blocked` risk. Medium and high risk
recipes are still printed with findings so the user can review them before
running anything.

## Risk Scoring

Risk scoring is local and deterministic. It is designed as a guardrail for
future AI-generated recipes and as a quick review aid for existing `justfile`s.

Current levels:

- `low`: no risky patterns detected.
- `medium`: network access, dependency installation, remote git operations, or
  container workflows.
- `high`: recursive deletion, elevated privileges, destructive Docker cleanup,
  filesystem formatting, raw disk copying, or recursive permission changes.
- `blocked`: downloaded content piped to a shell, or recursive removal from the
  filesystem root.

Examples of detected patterns:

| Pattern | Level | Reason |
| --- | --- | --- |
| `cargo install ...` | `medium` | Installs executable dependencies |
| `curl ...` | `medium` | Downloads content from the network |
| `git push ...` | `medium` | Changes remote git state |
| `rm -rf tmp/release` | `high` | Recursively removes files |
| `sudo ...` | `high` | Requires elevated privileges |
| `curl ... \| sh` | `blocked` | Pipes downloaded content to a shell |
| `rm -rf /` | `blocked` | Recursively removes from the filesystem root |

Risk scoring is intentionally conservative and text-based. It should be treated
as a first pass, not as a complete shell safety proof.

## Development

Format and test the crate:

```sh
cargo fmt --package just-ai
cargo check --package just-ai
cargo test --package just-ai
```

Run it against the local development build of `just`:

```sh
cargo build --bin just
cargo run --package just-ai -- --just-binary ./target/debug/just doctor
cargo run --package just-ai -- --just-binary ./target/debug/just export-context --pretty
```

Run it against a globally installed `just`:

```sh
cargo run --package just-ai -- doctor
```

## Architecture

`just-ai` stays outside the `just` runtime path:

```text
justfile
   |
   v
just --dump --dump-format json
   |
   v
just-ai
   |-- context export
   |-- local risk scoring
   |-- AI provider request
   |-- proposed patch
   `-- local validation before write
```

This design keeps the core `just` binary deterministic and avoids adding LLM,
HTTP, authentication, or provider dependencies to the task runner.

Important implementation pieces:

- `DumpModule`, `DumpRecipe`, `DumpParameter`: partial serde model for the
  existing `just` JSON dump.
- `ProjectContext`: compact output model intended for AI tools and editor
  integrations.
- `RiskFinding` and `RiskLevel`: deterministic local risk analysis.
- `DoctorReport`: summary view used by `doctor`.
- `AiProvider`: provider-neutral request boundary with a native
  OpenAI-compatible implementation.
- `RecipeProposal`: structured model output for `add`.

## Current Limitations

- Only OpenAI-compatible chat completions are supported.
- AI responses are constrained by prompts and JSON parsing, but model quality
  still depends on the selected provider and model.
- `just-ai add --write` appends a recipe to the root `justfile`; it does not yet
  insert into submodules or preserve custom grouping conventions.
- Dynamic `just` expressions are summarized as placeholders in command text.
- Risk scoring is pattern-based and does not parse shell syntax deeply.
- The JSON context format is new and should be treated as experimental.

## Roadmap

Likely next steps:

1. Add `just-ai fix <recipe>` to propose changes after a failed run.
2. Add interactive confirmation flows for applying generated patches.
3. Add provider-specific adapters for Anthropic and non-OpenAI Ollama APIs.
4. Add grouped insertion so generated recipes can land near related recipes.
5. Expose the context/risk protocol to VSCode or an LSP-adjacent extension.

The invariant should stay the same: AI can propose and explain changes, but
`just` remains the deterministic runtime for recipes.

## Troubleshooting

### `cargo` is not found

Open a new terminal or source your shell configuration:

```sh
source ~/.zshrc
```

If Homebrew installed `rustup` as keg-only, make sure this path is present:

```sh
export PATH="/opt/homebrew/opt/rustup/bin:$PATH"
```

### `just dump failed`

Make sure `just` can parse the current `justfile`:

```sh
just --dump --dump-format json
```

If you are testing against the development binary:

```sh
cargo build --bin just
just-ai --just-binary ./target/debug/just doctor
```

### Network errors during `cargo check`

The first build may need to download crates from `crates.io`. Retry with working
network access after the dependency cache is populated.
