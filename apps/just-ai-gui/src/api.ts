import { invoke } from "@tauri-apps/api/core";

export type RiskLevel = "low" | "medium" | "high" | "blocked";

export interface RiskFinding {
  level: RiskLevel;
  line: string;
  reason: string;
}

export interface Recipe {
  name: string;
  namepath: string;
  doc: string | null;
  body: string[];
  dependencies: string[];
  parameters: ContextParameter[];
  risk: RiskLevel;
  risks: RiskFinding[];
  private: boolean;
}

export interface ContextParameter {
  name: string;
  kind: string;
  default: string | null;
}

export interface ProjectContext {
  recipes: Recipe[];
  warnings: string[];
}

export type PolicyDecision =
  | { decision: "allow" }
  | { decision: "confirm" }
  | { decision: "confirm_typed"; phrase: string }
  | { decision: "deny"; reason: string };

export interface RunRequest {
  project_root: string;
  recipe: string;
  arguments: string[];
}

export interface PreparedRun {
  request: RunRequest;
  preview: string[];
  risk: RiskLevel;
  findings: RiskFinding[];
  policy: PolicyDecision;
}

export type RunConfirmation =
  | { confirmation: "none" }
  | { confirmation: "confirmed" }
  | { confirmation: "typed"; phrase: string };

export interface RunResult {
  success: boolean;
  exit_code: number | null;
  stdout: string;
  stderr: string;
}

export interface RunRecord {
  id: string;
  recipe: string;
  arguments: string[];
  started_at_ms: number;
  duration_ms: number;
  exit_code: number | null;
  success: boolean;
  cancelled: boolean;
  stdout_tail: string;
  stderr_tail: string;
}

// AI response types
export interface SuggestRecommendation {
  name: string;
  body: string[];
  rationale: string;
  risk: RiskLevel;
}

export interface SuggestResponse {
  summary: string;
  recommendations: SuggestRecommendation[];
}

export interface ExplainResponse {
  summary: string;
  explanation: string;
  parameters: string[];
  dependencies: string[];
  risks: string[];
}

export interface AddRecipeResponse {
  summary: string;
  recipe: {
    name: string;
    doc: string | null;
    parameters: { name: string; default: string | null }[];
    dependencies: string[];
    body: string[];
  };
  rationale: string[];
}

export interface FixRecipeResponse {
  summary: string;
  recipe: {
    name: string;
    doc: string | null;
    parameters: { name: string; default: string | null }[];
    dependencies: string[];
    body: string[];
  };
  rationale: string[];
}

// Workflow and batch types
export interface RecipeProposal {
  name: string;
  doc: string | null;
  parameters: { name: string; default: string | null }[];
  dependencies: string[];
  body: string[];
}

export interface WorkflowResponse {
  summary: string;
  recipes: RecipeProposal[];
  rationale: string[];
  execution_order: string[];
}

export interface AiWorkflowResult {
  success: boolean;
  message: string;
  diff?: string;
  recipes: string[];
  summary?: string;
  execution_order?: string[];
  workflow?: WorkflowResponse;
}

export interface AiFixBatchResult {
  success: boolean;
  message: string;
  diff?: string;
  fixed_recipes: string[];
}

export interface AiExplainBatchResult {
  success: boolean;
  explanations: ExplainResponse[];
}

// Wrapper responses from Tauri commands
export interface AiAddRecipeResult {
  success: boolean;
  message: string;
  diff?: string;
  recipe_name?: string;
  summary?: string;
  recipe?: AddRecipeResponse;
}

export interface AiFixRecipeResult {
  success: boolean;
  message: string;
  diff?: string;
  recipe_name?: string;
  summary?: string;
  recipe?: FixRecipeResponse;
}

export function inspectProject(projectRoot: string): Promise<ProjectContext> {
  return invoke("inspect_project", { projectRoot });
}

export function prepareRun(request: RunRequest): Promise<PreparedRun> {
  return invoke("prepare_run", { request });
}

export function executeRun(prepared: PreparedRun, confirmation: RunConfirmation): Promise<RunResult> {
  return invoke("execute_run", { prepared, confirmation });
}

export function recentRuns(projectRoot: string, limit = 20): Promise<RunRecord[]> {
  return invoke("recent_runs", { projectRoot, limit });
}

export function cancelRun(): Promise<boolean> {
  return invoke("cancel_run");
}

export function aiSuggest(projectRoot: string): Promise<SuggestResponse> {
  return invoke("ai_suggest", { projectRoot });
}

export function aiExplain(projectRoot: string, recipeName: string): Promise<ExplainResponse> {
  return invoke("ai_explain", { projectRoot, recipeName });
}

export function aiAddRecipe(projectRoot: string, request: string, write: boolean): Promise<AiAddRecipeResult> {
  return invoke("ai_add_recipe", { projectRoot, request: { request, write } });
}

export function aiFixRecipe(projectRoot: string, recipeName: string, write: boolean): Promise<AiFixRecipeResult> {
  return invoke("ai_fix_recipe", { projectRoot, request: { recipe_name: recipeName, write } });
}

export function aiWorkflow(projectRoot: string, request: string, write: boolean): Promise<AiWorkflowResult> {
  return invoke("ai_workflow", { projectRoot, request: { request, write } });
}

export function aiFixBatch(projectRoot: string, write: boolean): Promise<AiFixBatchResult> {
  return invoke("ai_fix_batch", { projectRoot, request: { write } });
}

export function aiExplainBatch(projectRoot: string, recipes?: string[], module?: string): Promise<AiExplainBatchResult> {
  return invoke("ai_explain_batch", { projectRoot, request: { recipes, module } });
}
