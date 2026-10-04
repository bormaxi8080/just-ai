#!/usr/bin/env python3
"""Check Codebase MCP source identity, exclusions and adapter call coverage.

Use --refresh to build a full persistent index. Never treats ready as sufficient.
Only the current checkout is indexed; other project caches are not modified.
"""
import argparse
import hashlib
import json
from pathlib import Path
import re
import subprocess


REVIEWED_EXCLUSIONS = {
    "examples/rule124.just": "3e63117acec5a0b1cb4f7033662b3bd5d01d4ed1ce44465d171c2bbbeede44cb",
}


def unpack(envelope):
    if envelope.get("isError"):
        raise RuntimeError(str(envelope.get("content")))
    if "structuredContent" in envelope:
        return envelope["structuredContent"]
    return json.loads(envelope["content"][0]["text"])


def rows(table):
    """Expand the v0.11 lossless grouped-table response."""
    for group in table.get("groups", []):
        for values in group["rows"]:
            row = dict(zip(table["cols"], values))
            prefix = group.get("qn_prefix", "")
            row["qualified_name"] = prefix + "." + row["name"] if prefix else row["name"]
            row["file_path"] = group.get("file", "")
            yield row


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--binary", default="codebase-memory-mcp")
    parser.add_argument("--root", type=Path, default=Path(__file__).resolve().parents[1])
    parser.add_argument("--refresh", action="store_true")
    args = parser.parse_args()
    root = args.root.resolve(strict=True)
    for path, fingerprint in REVIEWED_EXCLUSIONS.items():
        if hashlib.sha256((root / path).read_bytes()).hexdigest() != fingerprint:
            raise RuntimeError(f"Excluded upstream source changed; review before updating fingerprint: {path}")
    version = subprocess.check_output([args.binary, "--version"], text=True)
    match = re.search(r"(\d+)\.(\d+)\.(\d+)", version)
    if not match or tuple(map(int, match.groups())) < (0, 11, 0):
        raise RuntimeError("Codebase MCP >= 0.11.0 required for ignore and Rust call resolution")

    def call(tool, **arguments):
        output = subprocess.run([args.binary, "cli", "--json", tool],
                                input=json.dumps(arguments), capture_output=True,
                                text=True, check=True, timeout=180)
        return unpack(json.loads(output.stdout))

    refreshed = None
    if args.refresh:
        refreshed = call("index_repository", repo_path=str(root), mode="full", persistence=True)
    projects = call("list_projects", format="json", limit=1000)
    candidates = projects.get("projects", [])
    if isinstance(candidates, dict):
        candidates = list(rows(candidates))
    matching = [project for project in candidates if Path(project["root_path"]).resolve() == root]
    if len(matching) != 1:
        raise RuntimeError("Expected exactly one indexed project for the current checkout root")
    project = matching[0]["name"]
    status = call("index_status", project=project, format="json", diagnostics="full")
    if status.get("status") != "ready":
        raise RuntimeError(f"Index is not ready: {status}")
    diagnostics = refreshed or status
    for category in ("parse_partial", "parse_unusable", "skipped"):
        coverage = diagnostics.get(category, {})
        if coverage.get("truncated"):
            raise RuntimeError(f"Truncated {category} coverage; inspect full index diagnostics")
        for entry in coverage.get("files", []):
            path = entry.get("path", entry.get("file", ""))
            raise RuntimeError(f"Unreviewed source coverage failure: {category}: {path}")
    generated = call("search_graph", project=project, format="json",
                     file_pattern="apps/just-ai-vscode/out/*", limit=1)
    if generated.get("total", 0) != 0:
        raise RuntimeError("Generated editor output contaminates the graph")
    for path in REVIEWED_EXCLUSIONS:
        excluded = call("search_graph", project=project, format="json", file_pattern=path, limit=1)
        if excluded.get("total", 0) != 0:
            raise RuntimeError(f"Reviewed parser exception still indexed: {path}")

    symbol = project + ".crates.just-ai.src.application.templates.TemplatePlan.prepare"
    discovered = call("search_graph", project=project, format="json", qn_pattern=re.escape(symbol) + "$", limit=5)
    if discovered.get("total") != 1:
        raise RuntimeError("Expected current TemplatePlan.prepare symbol")
    snippet = call("get_code_snippet", project=project, qualified_name=symbol, source_mode="full", format="json")
    source_path = Path(snippet["file_path"]).resolve()
    if source_path != root / "crates/just-ai/src/application/templates.rs":
        raise RuntimeError("Snippet comes from a different checkout")
    actual = source_path.read_text().splitlines(keepends=True)
    expected = "".join(actual[snippet["start_line"] - 1:snippet["end_line"]])
    if snippet.get("source") != expected:
        raise RuntimeError("Indexed symbol source differs from the current checkout")

    trace = call("trace_path", project=project, function_name=symbol,
                 direction="inbound", depth=1, format="json", limit=100)
    callers = trace.get("callers", [])
    if isinstance(callers, dict):
        callers = list(rows(callers))
    observed = {caller["qualified_name"] for caller in callers}
    expected = {project + "." + path for path in (
        "crates.just-ai.src.proposal.handle_instantiate_template",
        "apps.just-ai-mcp.src.tools.call_tool_at",
        "apps.just-ai-gui.src-tauri.src.lib.ai_instantiate_template_blocking",
    )}
    if observed != expected:
        raise RuntimeError(f"TemplatePlan adapter call coverage mismatch: missing={expected-observed}, unexpected={observed-expected}")
    execution_symbol = project + ".crates.just-ai.src.application.execution.RecipeExecutor.prepare"
    execution_discovered = call("search_graph", project=project, format="json",
                                qn_pattern=re.escape(execution_symbol) + "$", limit=5)
    if execution_discovered.get("total") != 1:
        raise RuntimeError("Expected current RecipeExecutor.prepare symbol")
    execution_trace = call("trace_path", project=project, function_name=execution_symbol,
                           direction="inbound", depth=1, format="json", limit=100)
    execution_callers = execution_trace.get("callers", [])
    if isinstance(execution_callers, dict):
        execution_callers = list(rows(execution_callers))
    # File containers may aggregate cfg/test call sites; check actual function boundaries.
    execution_observed = {row["qualified_name"] for row in execution_callers
                          if not row["qualified_name"].endswith(".__file__")}
    execution_expected = {project + "." + path for path in (
        "crates.just-ai.src.application.execution.RecipeExecutor.execute_streaming_with_limit",
        "crates.just-ai.src.cli.try_main",
        "apps.just-ai-mcp.src.tools.call_tool_at",
        "apps.just-ai-gui.src-tauri.src.lib.prepare_run",
    )}
    if execution_observed != execution_expected:
        raise RuntimeError(f"Execution preparation call coverage mismatch: missing={execution_expected-execution_observed}, unexpected={execution_observed-execution_expected}")
    print(f"Codebase source/exclusion/CLI-MCP-GUI call checks passed: {project}")
    print(f"Index: {status.get('nodes', '?')} nodes, {status.get('edges', '?')} edges")


if __name__ == "__main__":
    main()
