# just-ai

`just-ai` is an opt-in companion binary for `justfile` analysis and future AI
workflows. It does not replace `just`, does not change how recipes run, and is
not required by projects that only want to use `just`.

The current implementation is intentionally offline:

- it reads project data from `just --dump --dump-format json`;
- it exports compact context for AI tools;
- it performs local risk scoring for recipe command bodies;
- it does not send project data to an LLM provider.

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
just-ai export-context --pretty
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

## Commands

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

The first implementation keeps `just-ai` outside the `just` runtime path:

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
   `-- future AI workflows
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

## Current Limitations

- There is no LLM provider integration yet.
- `just-ai` does not edit `justfile`s yet.
- Dynamic `just` expressions are summarized as placeholders in command text.
- Risk scoring is pattern-based and does not parse shell syntax deeply.
- The JSON context format is new and should be treated as experimental.

## Roadmap

Likely next steps:

1. Add `just-ai suggest` to recommend missing project recipes.
2. Add `just-ai add "<task>"` to generate a patch for a new recipe.
3. Add `just-ai fix <recipe>` to propose changes after a failed run.
4. Add provider abstraction for OpenAI, Anthropic, Ollama, and OpenAI-compatible
   endpoints.
5. Add `--write` and interactive confirmation flows for applying generated
   patches.
6. Expose the context/risk protocol to VSCode or an LSP-adjacent extension.

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
