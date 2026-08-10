import { useEffect, useMemo, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { cancelRun, executeRun, inspectProject, prepareRun, recentRuns, type ContextParameter, type ProjectContext, type Recipe, type RunConfirmation, type RunRecord, type RunResult } from "./api";

export function App() {
  const [root, setRoot] = useState(".");
  const [project, setProject] = useState<ProjectContext | null>(null);
  const [selected, setSelected] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [run, setRun] = useState<RunResult | null>(null);
  const [liveOutput, setLiveOutput] = useState("");
  const [running, setRunning] = useState(false);
  const [history, setHistory] = useState<RunRecord[]>([]);

  const recipe = useMemo(
    () => project?.recipes.find((item) => item.namepath === selected) ?? null,
    [project, selected],
  );

  async function load() {
    setError(null);
    try {
      const next = await inspectProject(root);
      setProject(next);
      setSelected((current) => next.recipes.some((item) => item.namepath === current)
        ? current : next.recipes[0]?.namepath ?? null);
      setHistory(await recentRuns(root).catch(() => []));
    } catch (reason) {
      setError(String(reason));
    }
  }

  async function runRecipe(selectedRecipe: Recipe, arguments_: string[]) {
    setError(null); setRun(null); setLiveOutput("");
    try {
      const prepared = await prepareRun({ project_root: root, recipe: selectedRecipe.namepath, arguments: arguments_ });
      let confirmation: RunConfirmation = { confirmation: "none" };
      if (prepared.policy.decision === "deny") throw new Error(prepared.policy.reason);
      if (prepared.policy.decision === "confirm" && !window.confirm(`Run ${selectedRecipe.namepath}?`)) return;
      if (prepared.policy.decision === "confirm") confirmation = { confirmation: "confirmed" };
      if (prepared.policy.decision === "confirm_typed") {
        const phrase = window.prompt(`Type “${prepared.policy.phrase}” to continue:`);
        if (phrase === null) return;
        confirmation = { confirmation: "typed", phrase };
      }
      setRunning(true);
      setRun(await executeRun(prepared, confirmation));
    } catch (reason) { setError(String(reason)); }
    finally {
      setRunning(false);
      try { setHistory(await recentRuns(root)); } catch { /* The run result remains primary. */ }
    }
  }

  useEffect(() => { void load(); }, []);
  useEffect(() => {
    const unlisten = listen<{ event: string; text?: string }>("run-event", ({ payload }) => {
      if ((payload.event === "stdout" || payload.event === "stderr") && payload.text) {
        setLiveOutput((current) => current + payload.text);
      }
    });
    return () => { void unlisten.then((dispose) => dispose()); };
  }, []);

  return <main>
    <header>
      <div><span className="eyebrow">LOCAL WORKFLOW CONTROL</span><h1>just-ai</h1></div>
      <form onSubmit={(event) => { event.preventDefault(); void load(); }}>
        <input aria-label="Project root" value={root} onChange={(e) => setRoot(e.target.value)} />
        <button>Inspect project</button>
      </form>
    </header>
    {error && <p className="error">{error}</p>}
    <section className="layout">
      <nav aria-label="Recipes">
        <h2>Recipes <small>{project?.recipes.length ?? 0}</small></h2>
        {project?.recipes.filter((item) => !item.private).map((item) =>
          <RecipeRow key={item.namepath} recipe={item} selected={item.namepath === selected}
            onSelect={() => setSelected(item.namepath)} />)}
        <RunHistory records={history} />
      </nav>
      <article>{recipe ? <RecipeDetail key={recipe.namepath} recipe={recipe}
        onRun={(arguments_) => void runRecipe(recipe, arguments_)} /> : <p>Select a recipe.</p>}
        {running && <button className="cancel-button" onClick={() => void cancelRun()}>Cancel run</button>}
        {run && <section className="run-output"><h3>Run output · {run.success ? "success" : `exit ${run.exit_code}`}</h3>
          <pre>{liveOutput || `${run.stdout}${run.stderr && `\n${run.stderr}`}`}</pre></section>}
      </article>
    </section>
  </main>;
}

function RecipeRow({ recipe, selected, onSelect }: { recipe: Recipe; selected: boolean; onSelect: () => void }) {
  return <button className={`recipe ${selected ? "selected" : ""}`} onClick={onSelect}>
    <span>{recipe.namepath}</span><i data-risk={recipe.risk}>{recipe.risk}</i>
  </button>;
}

function RecipeDetail({ recipe, onRun }: { recipe: Recipe; onRun: (arguments_: string[]) => void }) {
  const [values, setValues] = useState<Record<string, string>>(() => initialParameterValues(recipe.parameters));
  const [parameterError, setParameterError] = useState<string | null>(null);

  function submit(event: React.FormEvent) {
    event.preventDefault();
    try {
      onRun(buildArguments(recipe.parameters, values));
      setParameterError(null);
    } catch (reason) {
      setParameterError(String(reason));
    }
  }

  return <>
    <span className="eyebrow">RECIPE</span><h2>{recipe.namepath}</h2>
    <p>{recipe.doc ?? "No description yet."}</p>
    <h3>Command preview</h3><pre>{recipe.body.join("\n")}</pre>
    <h3>Dependencies</h3><p>{recipe.dependencies.join(", ") || "None"}</p>
    <h3>Parameters</h3>
    <form className="parameter-form" onSubmit={submit}>
      {recipe.parameters.length === 0 ? <p>None</p> : recipe.parameters.map((parameter) =>
        <label key={parameter.name}>
          <span>{parameter.name} <small>{parameterLabel(parameter)}</small></span>
          {isVariadic(parameter) ?
            <textarea value={values[parameter.name] ?? ""} rows={3}
              placeholder="One argument per line"
              onChange={(event) => setValues({ ...values, [parameter.name]: event.target.value })} /> :
            <input value={values[parameter.name] ?? ""}
              onChange={(event) => setValues({ ...values, [parameter.name]: event.target.value })} />}
        </label>)}
      {parameterError && <p className="error">{parameterError}</p>}
      <button className="run-button">Prepare & run</button>
    </form>
    <h3>Local risk analysis</h3>
    <div className="risk-card"><strong data-risk={recipe.risk}>{recipe.risk}</strong>
      {recipe.risks.length === 0 ? <p>No deterministic findings.</p> :
        <ul>{recipe.risks.map((finding, index) => <li key={index}>{finding.reason}</li>)}</ul>}
    </div>
  </>;
}

function RunHistory({ records }: { records: RunRecord[] }) {
  return <section className="history">
    <h3>Recent runs</h3>
    {records.length === 0 ? <p>No runs yet.</p> : records.map((record) =>
      <details className="history-row" key={record.id}>
        <summary>
          <span>{record.recipe}</span>
          <small className={record.success ? "success" : "failure"}>
            {record.cancelled ? "cancelled" : record.success ? "success" : `exit ${record.exit_code ?? "?"}`} · {record.duration_ms} ms
          </small>
        </summary>
        <small>{new Date(record.started_at_ms).toLocaleString()}</small>
        <code>{[record.recipe, ...record.arguments].join(" ")}</code>
        {record.stdout_tail && <pre>{record.stdout_tail}</pre>}
        {record.stderr_tail && <pre className="history-stderr">{record.stderr_tail}</pre>}
      </details>)}
  </section>;
}

function initialParameterValues(parameters: ContextParameter[]): Record<string, string> {
  return Object.fromEntries(parameters.map((parameter) => [parameter.name, parameter.default ?? ""]));
}

function isVariadic(parameter: ContextParameter): boolean {
  return parameter.kind === "plus" || parameter.kind === "star";
}

function parameterLabel(parameter: ContextParameter): string {
  if (parameter.kind === "plus") return "one or more";
  if (parameter.kind === "star") return "zero or more";
  return parameter.default === null ? "required" : `default: ${parameter.default}`;
}

function buildArguments(parameters: ContextParameter[], values: Record<string, string>): string[] {
  return parameters.flatMap((parameter) => {
    const value = values[parameter.name] ?? "";
    if (isVariadic(parameter)) {
      const items = value.split("\n").map((item) => item.trim()).filter(Boolean);
      if (parameter.kind === "plus" && items.length === 0) {
        throw new Error(`${parameter.name} requires at least one argument`);
      }
      return items;
    }
    if (value === "" && parameter.default === null) {
      throw new Error(`${parameter.name} is required`);
    }
    return [value];
  });
}
