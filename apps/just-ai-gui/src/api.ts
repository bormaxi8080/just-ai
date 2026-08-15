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

// Template types
export interface TemplateParameterInfo {
  name: string;
  description: string;
  required: boolean;
  default: string | null;
}

export interface AiTemplateResult {
  success: boolean;
  template_name: string;
  template_description: string;
  template_category: string;
  template_parameters: TemplateParameterInfo[];
  template_body: string[];
  summary: string;
}

export interface AiInstantiateTemplateResult {
  success: boolean;
  message: string;
  diff?: string;
  recipe_name?: string;
  summary?: string;
  recipe?: AddRecipeResponse;
}

export interface AiComposeWorkflowResult {
  success: boolean;
  message: string;
  diff?: string;
  recipes: string[];
  summary?: string;
  execution_order?: string[];
  workflow?: ComposeWorkflowResponse;
}

export interface ComposeWorkflowResponse {
  summary: string;
  rationale: string[];
  execution_order: string[];
  recipes: ComposeRecipe[];
}

export interface ComposeRecipe {
  name: string;
  source: "existing" | "new" | "modified";
  doc: string | null;
  parameters: { name: string; default: string | null }[];
  dependencies: string[];
  body: string[];
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

// Template commands
export function aiTemplate(projectRoot: string, request: string): Promise<AiTemplateResult> {
  return invoke("ai_template", { projectRoot, request: { request } });
}

export function aiInstantiateTemplate(
  projectRoot: string,
  template: string,
  values: Record<string, string>,
  write: boolean
): Promise<AiInstantiateTemplateResult> {
  return invoke("ai_instantiate_template", { projectRoot, request: { template, values, write } });
}

export function aiComposeWorkflow(projectRoot: string, request: string, write: boolean): Promise<AiComposeWorkflowResult> {
  return invoke("ai_compose_workflow", { projectRoot, request: { request, write } });
}

// Export Context types
export interface ExportContextResult {
  success: boolean;
  context: ProjectContext;
}

export function aiExportContext(projectRoot: string): Promise<ExportContextResult> {
  return invoke("ai_export_context", { projectRoot });
}

// Doctor types
export interface DoctorRecipeGui {
  namepath: string;
  risk: RiskLevel;
  risks: RiskFinding[];
}

export interface DoctorResult {
  success: boolean;
  total_recipes: number;
  low: number;
  medium: number;
  high: number;
  blocked: number;
  highest_risk: RiskLevel;
  recipes: DoctorRecipeGui[];
}

export function aiDoctor(projectRoot: string): Promise<DoctorResult> {
  return invoke("ai_doctor", { projectRoot });
}

// Migrate types
export interface MigrateAnalyzeResult {
  success: boolean;
  total_recipes: number;
  unreferenced_recipes: string[];
  isolated_recipes: string[];
  cycles: string[][];
  dependency_depths: Record<string, number>;
  similar_recipes: [string, string, number][];
}

export interface MigrateModularizeResult {
  success: boolean;
  message: string;
  modules: string[];
  imports: string[];
  moved_recipes: string[];
  diff?: string;
}

export interface MigrateDeduplicateResult {
  success: boolean;
  message: string;
  similar_pairs: [string, string, number][];
  removed: string[];
  merged: string[];
  diff?: string;
}

export function aiMigrateAnalyze(projectRoot: string, json?: boolean): Promise<MigrateAnalyzeResult> {
  return invoke("ai_migrate_analyze", { projectRoot, request: { json } });
}

export function aiMigrateModularize(projectRoot: string, write: boolean): Promise<MigrateModularizeResult> {
  return invoke("ai_migrate_modularize", { projectRoot, request: { write } });
}

export function aiMigrateDeduplicate(
  projectRoot: string,
  write: boolean,
  merge?: boolean,
  similarityThreshold?: number
): Promise<MigrateDeduplicateResult> {
  return invoke("ai_migrate_deduplicate", {
    projectRoot,
    request: { write, merge: merge ?? false, similarity_threshold: similarityThreshold },
  });
}
