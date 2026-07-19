# `just --dump` compatibility fixtures

These versioned fixtures freeze the JSON boundary consumed by `just-ai`.
They follow the upstream `just 1.54.0` JSON contract and are intentionally
parsed without invoking a shell or duplicating the upstream parser.

- `just-dump-basic.json` covers recipes, dependencies, defaults, warnings, and
  deterministic risk derivation.
- `just-dump-rich.json` covers nested modules, structured body interpolation,
  shebang metadata, visibility/quiet flags, and singular/plus/star parameters.

When the bundled upstream `just` JSON schema changes, add a new fixture instead
of silently rewriting old coverage. Keep platform-only shapes in a separate
fixture generated and exercised on that platform.
