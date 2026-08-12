import { useEffect, useMemo, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import {
  aiAddRecipe,
  aiExplain,
  aiFixRecipe,
  aiSuggest,
  aiWorkflow,
  aiFixBatch,
  aiExplainBatch,
  cancelRun,
  executeRun,
  inspectProject,
  prepareRun,
  recentRuns,
  type ContextParameter,
  type ProjectContext,
  type Recipe,
  type RunConfirmation,
  type RunRecord,
  type RunResult,
  type SuggestResponse,
  type ExplainResponse,
  type AddRecipeResponse,
  type FixRecipeResponse,
  type AiWorkflowResult,
  type AiFixBatchResult,
  type AiExplainBatchResult,
} from "./api";

export function App() {
  const [root, setRoot] = useState(".");
  const [project, setProject] = useState<ProjectContext | null>(null);
  const [selected, setSelected] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [run, setRun] = useState<RunResult | null>(null);
  const [liveOutput, setLiveOutput] = useState("");
  const [running, setRunning] = useState(false);
  const [history, setHistory] = useState<RunRecord[]>([]);

  // AI feature states
  const [suggestResult, setSuggestResult] = useState<SuggestResponse | null>(null);
  const [explainResult, setExplainResult] = useState<ExplainResponse | null>(null);
  const [addRecipeRequest, setAddRecipeRequest] = useState("");
  const [addRecipeLoading, setAddRecipeLoading] = useState(false);
  const [addRecipeResult, setAddRecipeResult] = useState<string | null>(null);
  const [fixRecipeResult, setFixRecipeResult] = useState<string | null>(null);
  const [fixRecipeLoading, setFixRecipeLoading] = useState(false);
  const [showAiPanel, setShowAiPanel] = useState(false);

  // Workflow and batch states
  const [workflowRequest, setWorkflowRequest] = useState("");
  const [workflowLoading, setWorkflowLoading] = useState(false);
  const [workflowResult, setWorkflowResult] = useState<AiWorkflowResult | null>(null);
  const [fixBatchLoading, setFixBatchLoading] = useState(false);
  const [fixBatchResult, setFixBatchResult] = useState<AiFixBatchResult | null>(null);
  const [explainBatchLoading, setExplainBatchLoading] = useState(false);
  const [explainBatchResult, setExplainBatchResult] = useState<AiExplainBatchResult | null>(null);

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

  async function handleSuggest() {
    setError(null);
    try {
      const result = await aiSuggest(root);
      setSuggestResult(result);
      setShowAiPanel(true);
    } catch (reason) {
      setError(String(reason));
    }
  }

  async function handleExplain() {
    if (!recipe) return;
    setError(null);
    setExplainResult(null);
    try {
      const result = await aiExplain(root, recipe.namepath);
      setExplainResult(result);
      setShowAiPanel(true);
    } catch (reason) {
      setError(String(reason));
    }
  }

  async function handleAddRecipe() {
    if (!addRecipeRequest.trim()) return;
    setError(null);
    setAddRecipeLoading(true);
    setAddRecipeResult(null);
    try {
      const result = await aiAddRecipe(root, addRecipeRequest, false); // dry-run first
      setAddRecipeResult(result.message);
      setShowAiPanel(true);
    } catch (reason) {
      setError(String(reason));
    } finally {
      setAddRecipeLoading(false);
    }
  }

  async function handleAddRecipeWrite() {
    if (!addRecipeRequest.trim()) return;
    setError(null);
    setAddRecipeLoading(true);
    setAddRecipeResult(null);
    try {
      const result = await aiAddRecipe(root, addRecipeRequest, true); // write
      setAddRecipeResult(result.message);
      await load(); // reload project to show new recipe
      setShowAiPanel(true);
    } catch (reason) {
      setError(String(reason));
    } finally {
      setAddRecipeLoading(false);
    }
  }

  async function handleFixRecipe() {
    if (!recipe) return;
    setError(null);
    setFixRecipeLoading(true);
    setFixRecipeResult(null);
    try {
      const result = await aiFixRecipe(root, recipe.namepath, false);
      setFixRecipeResult(result.message);
      setShowAiPanel(true);
    } catch (reason) {
      setError(String(reason));
    } finally {
      setFixRecipeLoading(false);
    }
  }

  async function handleFixRecipeWrite() {
    if (!recipe) return;
    setError(null);
    setFixRecipeLoading(true);
    setFixRecipeResult(null);
    try {
      const result = await aiFixRecipe(root, recipe.namepath, true);
      setFixRecipeResult(result.message);
      await load(); // reload project to show fixed recipe
      setShowAiPanel(true);
    } catch (reason) {
      setError(String(reason));
    } finally {
      setFixRecipeLoading(false);
    }
  }

  async function handleWorkflow() {
    if (!workflowRequest.trim()) return;
    setError(null);
    setWorkflowLoading(true);
    setWorkflowResult(null);
    try {
      const result = await aiWorkflow(root, workflowRequest, false); // dry-run first
      setWorkflowResult(result);
      setShowAiPanel(true);
    } catch (reason) {
      setError(String(reason));
    } finally {
      setWorkflowLoading(false);
    }
  }

  async function handleWorkflowWrite() {
    if (!workflowRequest.trim()) return;
    setError(null);
    setWorkflowLoading(true);
    setWorkflowResult(null);
    try {
      const result = await aiWorkflow(root, workflowRequest, true); // write
      setWorkflowResult(result);
      await load(); // reload project to show new recipes
      setShowAiPanel(true);
    } catch (reason) {
      setError(String(reason));
    } finally {
      setWorkflowLoading(false);
    }
  }

  async function handleFixBatch() {
    setError(null);
    setFixBatchLoading(true);
    setFixBatchResult(null);
    try {
      const result = await aiFixBatch(root, false); // dry-run first
      setFixBatchResult(result);
      setShowAiPanel(true);
    } catch (reason) {
      setError(String(reason));
    } finally {
      setFixBatchLoading(false);
    }
  }

  async function handleFixBatchWrite() {
    setError(null);
    setFixBatchLoading(true);
    setFixBatchResult(null);
    try {
      const result = await aiFixBatch(root, true); // write
      setFixBatchResult(result);
      await load(); // reload project
      setShowAiPanel(true);
    } catch (reason) {
      setError(String(reason));
    } finally {
      setFixBatchLoading(false);
    }
  }

  async function handleExplainBatch() {
    setError(null);
    setExplainBatchLoading(true);
    setExplainBatchResult(null);
    try {
      // Get all recipe names from project
      const recipes = project?.recipes.map(r => r.namepath) ?? [];
      const result = await aiExplainBatch(root, recipes, undefined);
      setExplainBatchResult(result);
      setShowAiPanel(true);
    } catch (reason) {
      setError(String(reason));
    } finally {
      setExplainBatchLoading(false);
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

        {/* AI Features Panel */}
        <section className="ai-panel">
          <h3>AI Assistant</h3>
          <div className="ai-buttons">
            <button onClick={handleSuggest} disabled={running}>Suggest Recipes</button>
            {recipe && <button onClick={handleExplain} disabled={running}>Explain Recipe</button>}
          </div>
          {recipe && (
            <div className="ai-buttons">
              <button onClick={handleFixRecipe} disabled={running || fixRecipeLoading}>
                {fixRecipeLoading ? "Fixing..." : "Fix Recipe"}
              </button>
              <button onClick={handleFixRecipeWrite} disabled={running || fixRecipeLoading}>
                Fix & Write
              </button>
            </div>
          )}
          <div className="ai-buttons">
            <input
              type="text"
              value={addRecipeRequest}
              onChange={(e) => setAddRecipeRequest(e.target.value)}
              placeholder="Describe recipe to add..."
              disabled={addRecipeLoading}
            />
            <button onClick={handleAddRecipe} disabled={running || addRecipeLoading || !addRecipeRequest.trim()}>
              {addRecipeLoading ? "Adding..." : "Add Recipe"}
            </button>
            <button onClick={handleAddRecipeWrite} disabled={running || addRecipeLoading || !addRecipeRequest.trim()}>
              Add & Write
            </button>
          </div>
          <div className="ai-buttons">
            <input
              type="text"
              value={workflowRequest}
              onChange={(e) => setWorkflowRequest(e.target.value)}
              placeholder="Describe multi-recipe workflow..."
              disabled={workflowLoading}
            />
            <button onClick={handleWorkflow} disabled={running || workflowLoading || !workflowRequest.trim()}>
              {workflowLoading ? "Generating..." : "Create Workflow"}
            </button>
            <button onClick={handleWorkflowWrite} disabled={running || workflowLoading || !workflowRequest.trim()}>
              Workflow & Write
            </button>
          </div>
          <div className="ai-buttons">
            <button onClick={handleFixBatch} disabled={running || fixBatchLoading}>
              {fixBatchLoading ? "Analyzing..." : "Fix All Failed"}
            </button>
            <button onClick={handleFixBatchWrite} disabled={running || fixBatchLoading}>
              Fix All & Write
            </button>
          </div>
          <div className="ai-buttons">
            <button onClick={handleExplainBatch} disabled={running || explainBatchLoading}>
              {explainBatchLoading ? "Explaining..." : "Explain All Recipes"}
            </button>
          </div>
        </section>
      </nav>
      <article>
        {recipe ? <RecipeDetail key={recipe.namepath} recipe={recipe}
          onRun={(arguments_) => void runRecipe(recipe, arguments_)} /> : <p>Select a recipe.</p>}
        {running && <button className="cancel-button" onClick={() => void cancelRun()}>Cancel run</button>}
        {run && <section className="run-output"><h3>Run output · {run.success ? "success" : `exit ${run.exit_code}`}</h3>
          <pre>{liveOutput || `${run.stdout}${run.stderr && `\n${run.stderr}`}`}</pre></section>}

        {/* AI Results Panel */}
        {showAiPanel && (
          <aside className="ai-results">
            <div className="ai-results-header">
              <h3>AI Response</h3>
              <button onClick={() => setShowAiPanel(false)}>Close</button>
            </div>
            {suggestResult && (
              <SuggestResult result={suggestResult} onClose={() => setShowAiPanel(false)} />
            )}
            {explainResult && (
              <ExplainResult result={explainResult} onClose={() => setShowAiPanel(false)} />
            )}
            {addRecipeResult && (
              <AddFixResult message={addRecipeResult} onClose={() => setShowAiPanel(false)} />
            )}
            {fixRecipeResult && (
              <AddFixResult message={fixRecipeResult} onClose={() => setShowAiPanel(false)} />
            )}
            {workflowResult && (
              <WorkflowResult result={workflowResult} onClose={() => setShowAiPanel(false)} />
            )}
            {fixBatchResult && (
              <FixBatchResult result={fixBatchResult} onClose={() => setShowAiPanel(false)} />
            )}
            {explainBatchResult && (
              <ExplainBatchResult result={explainBatchResult} onClose={() => setShowAiPanel(false)} />
            )}
          </aside>
        )}
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

function SuggestResult({ result, onClose }: { result: SuggestResponse; onClose: () => void }) {
  return (
    <div className="ai-result">
      <h4>{result.summary}</h4>
      {result.recommendations.map((rec, idx) => (
        <details key={idx} className="ai-recommendation">
          <summary>
            <strong>{rec.name}</strong> <span className="risk-badge" data-risk={rec.risk}>{rec.risk}</span>
          </summary>
          <p>{rec.rationale}</p>
          <pre>{rec.body.join("\n")}</pre>
        </details>
      ))}
      <button onClick={onClose}>Close</button>
    </div>
  );
}

function ExplainResult({ result, onClose }: { result: ExplainResponse; onClose: () => void }) {
  return (
    <div className="ai-result">
      <h4>{result.summary}</h4>
      <p>{result.explanation}</p>
      {result.parameters.length > 0 && (
        <section>
          <h5>Parameters</h5>
          <ul>{result.parameters.map((p, i) => <li key={i}>{p}</li>)}</ul>
        </section>
      )}
      {result.dependencies.length > 0 && (
        <section>
          <h5>Dependencies</h5>
          <ul>{result.dependencies.map((d, i) => <li key={i}>{d}</li>)}</ul>
        </section>
      )}
      {result.risks.length > 0 && (
        <section>
          <h5>Risks</h5>
          <ul>{result.risks.map((r, i) => <li key={i}>{r}</li>)}</ul>
        </section>
      )}
      <button onClick={onClose}>Close</button>
    </div>
  );
}

function AddFixResult({ message, onClose }: { message: string; onClose: () => void }) {
  return (
    <div className="ai-result">
      <p>{message}</p>
      <button onClick={onClose}>Close</button>
    </div>
  );
}

function WorkflowResult({ result, onClose }: { result: AiWorkflowResult; onClose: () => void }) {
  return (
    <div className="ai-result">
      <h4>{result.summary || "Workflow Generated"}</h4>
      <p>{result.message}</p>
      {result.execution_order && result.execution_order.length > 0 && (
        <section>
          <h5>Execution Order</h5>
          <ol>
            {result.execution_order.map((name, i) => (
              <li key={i}>{name}</li>
            ))}
          </ol>
        </section>
      )}
      {result.recipes && result.recipes.length > 0 && (
        <section>
          <h5>Recipes ({result.recipes.length})</h5>
          <ul>
            {result.recipes.map((name, i) => (
              <li key={i}>{name}</li>
            ))}
          </ul>
        </section>
      )}
      {result.workflow && (
        <section>
          <h5>Workflow Details</h5>
          {result.workflow.rationale.length > 0 && (
            <section>
              <h6>Rationale</h6>
              <ul>{result.workflow.rationale.map((r, i) => <li key={i}>{r}</li>)}</ul>
            </section>
          )}
          {result.workflow.recipes.map((recipe, idx) => (
            <details key={idx} className="ai-recommendation">
              <summary>
                <strong>{recipe.name}</strong>
                {recipe.doc && <span className="doc-badge">{recipe.doc}</span>}
              </summary>
              {recipe.dependencies.length > 0 && (
                <p><strong>Dependencies:</strong> {recipe.dependencies.join(", ")}</p>
              )}
              <pre>{recipe.body.join("\n")}</pre>
            </details>
          ))}
        </section>
      )}
      {result.diff && (
        <section>
          <h5>Diff</h5>
          <pre className="diff">{result.diff}</pre>
        </section>
      )}
      <button onClick={onClose}>Close</button>
    </div>
  );
}

function FixBatchResult({ result, onClose }: { result: AiFixBatchResult; onClose: () => void }) {
  return (
    <div className="ai-result">
      <h4>{result.success ? "Batch Fix Complete" : "Batch Fix Failed"}</h4>
      <p>{result.message}</p>
      {result.fixed_recipes.length > 0 && (
        <section>
          <h5>Fixed Recipes ({result.fixed_recipes.length})</h5>
          <ul>
            {result.fixed_recipes.map((name, i) => (
              <li key={i}>{name}</li>
            ))}
          </ul>
        </section>
      )}
      {result.diff && (
        <section>
          <h5>Diff</h5>
          <pre className="diff">{result.diff}</pre>
        </section>
      )}
      <button onClick={onClose}>Close</button>
    </div>
  );
}

function ExplainBatchResult({ result, onClose }: { result: AiExplainBatchResult; onClose: () => void }) {
  return (
    <div className="ai-result">
      <h4>Batch Explanations ({result.explanations.length} recipes)</h4>
      {result.explanations.map((explain, idx) => (
        <details key={idx} className="ai-recommendation">
          <summary>
            <strong>Recipe {idx + 1}</strong>
          </summary>
          <p>{explain.explanation}</p>
          {explain.parameters.length > 0 && (
            <section>
              <h5>Parameters</h5>
              <ul>{explain.parameters.map((p, i) => <li key={i}>{p}</li>)}</ul>
            </section>
          )}
          {explain.dependencies.length > 0 && (
            <section>
              <h5>Dependencies</h5>
              <ul>{explain.dependencies.map((d, i) => <li key={i}>{d}</li>)}</ul>
            </section>
          )}
          {explain.risks.length > 0 && (
            <section>
              <h5>Risks</h5>
              <ul>{explain.risks.map((r, i) => <li key={i}>{r}</li>)}</ul>
            </section>
          )}
        </details>
      ))}
      <button onClick={onClose}>Close</button>
    </div>
  );
}
