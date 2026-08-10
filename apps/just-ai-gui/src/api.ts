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
