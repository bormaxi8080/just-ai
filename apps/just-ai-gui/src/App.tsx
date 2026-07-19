import { useEffect, useMemo, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { cancelRun, executeRun, inspectProject, prepareRun, type ProjectContext, type Recipe, type RunConfirmation, type RunResult } from "./api";

export function App() {
  const [root, setRoot] = useState(".");
  const [project, setProject] = useState<ProjectContext | null>(null);
  const [selected, setSelected] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [run, setRun] = useState<RunResult | null>(null);
  const [liveOutput, setLiveOutput] = useState("");
  const [running, setRunning] = useState(false);

  const recipe = useMemo(
    () => project?.recipes.find((item) => item.namepath === selected) ?? null,
    [project, selected],
  );

  async function load() {
    setError(null);
    try {
      const next = await inspectProject(root);
      setProject(next);
      setSelected((current) => current ?? next.recipes[0]?.namepath ?? null);
    } catch (reason) {
      setError(String(reason));
    }
  }

  async function runRecipe(selectedRecipe: Recipe) {
    setError(null); setRun(null); setLiveOutput("");
    try {
      const prepared = await prepareRun({ project_root: root, recipe: selectedRecipe.namepath, arguments: [] });
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
    finally { setRunning(false); }
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
      </nav>
      <article>{recipe ? <RecipeDetail recipe={recipe} onRun={() => void runRecipe(recipe)} /> : <p>Select a recipe.</p>}
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

function RecipeDetail({ recipe, onRun }: { recipe: Recipe; onRun: () => void }) {
  return <>
    <span className="eyebrow">RECIPE</span><h2>{recipe.namepath}</h2>
    <p>{recipe.doc ?? "No description yet."}</p>
    <h3>Command preview</h3><pre>{recipe.body.join("\n")}</pre>
    <h3>Dependencies</h3><p>{recipe.dependencies.join(", ") || "None"}</p>
    <h3>Local risk analysis</h3>
    <div className="risk-card"><strong data-risk={recipe.risk}>{recipe.risk}</strong>
      {recipe.risks.length === 0 ? <p>No deterministic findings.</p> :
        <ul>{recipe.risks.map((finding, index) => <li key={index}>{finding.reason}</li>)}</ul>}
    </div>
    <button className="run-button" onClick={onRun}>Prepare & run</button>
  </>;
}
