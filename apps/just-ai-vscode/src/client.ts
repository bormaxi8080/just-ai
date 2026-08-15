import * as vscode from 'vscode';
import { spawn } from 'child_process';
import { join, dirname } from 'path';
import { existsSync } from 'fs';

export interface JustRecipe {
  namepath: string;
  risk: 'low' | 'medium' | 'high' | 'blocked';
  risks: Array<{ level: string; line: string; reason: string }>;
  body: string[];
  dependencies: string[];
  parameters: Array<{ name: string; default?: string }>;
  doc?: string;
}

export interface ProjectContext {
  recipes: JustRecipe[];
  modules: Array<{ name: string; path: string }>;
}

export interface HistoryRecord {
  success: boolean;
  recipe: string;
  exit_code?: number;
  duration_ms: number;
  stdout_tail: string;
  stderr_tail: string;
  started_at_ms: number;
}

export interface DoctorReport {
  total_recipes: number;
  low: number;
  medium: number;
  high: number;
  blocked: number;
  highest_risk: string;
  recipes: Array<{
    namepath: string;
    risk: string;
    findings: Array<{ level: string; line: string; reason: string }>;
  }>;
}

export class JustAiClient {
  private justBinary: string;
  private projectRoot: string;

  constructor(projectRoot: string, justBinary: string = 'just') {
    this.projectRoot = projectRoot;
    this.justBinary = justBinary;
  }

  private runJustAi(args: string[]): Promise<string> {
    return new Promise((resolve, reject) => {
      const env = { ...process.env };
      // Ensure we inherit the user's environment for API keys
      const child = spawn(this.justBinary, ['--dump', '--dump-format', 'json'], {
        cwd: this.projectRoot,
        env,
        stdio: ['ignore', 'pipe', 'pipe']
      });

      let stdout = '';
      let stderr = '';

      child.stdout.on('data', (data) => stdout += data.toString());
      child.stderr.on('data', (data) => stderr += data.toString());

      child.on('close', (code) => {
        if (code === 0) {
          resolve(stdout);
        } else {
          reject(new Error(`just --dump failed: ${stderr}`));
        }
      });
    });
  }

  async getProjectContext(): Promise<ProjectContext> {
    const output = await this.runJustAi([]);
    return JSON.parse(output);
  }

  async runDoctor(json: boolean = false): Promise<DoctorReport | string> {
    return new Promise((resolve, reject) => {
      const args = ['doctor'];
      if (json) { args.push('--json'); }

      const child = spawn(this.justBinary, args, {
        cwd: this.projectRoot,
        env: { ...process.env },
        stdio: ['ignore', 'pipe', 'pipe']
      });

      let stdout = '';
      let stderr = '';

      child.stdout.on('data', (data) => stdout += data.toString());
      child.stderr.on('data', (data) => stderr += data.toString());

      child.on('close', (code) => {
        if (code === 0 || code === 1) { // doctor returns 1 for blocked
          if (json) {
            resolve(JSON.parse(stdout));
          } else {
            resolve(stdout);
          }
        } else {
          reject(new Error(`just-ai doctor failed: ${stderr}`));
        }
      });
    });
  }

  async suggest(): Promise<string> {
    return this.runJustAiCommand('suggest');
  }

  async explain(recipe: string): Promise<string> {
    return this.runJustAiCommand('explain', recipe);
  }

  async add(request: string, write: boolean = false): Promise<string> {
    return this.runJustAiCommand('add', request, write ? '--write' : '');
  }

  async fix(recipe: string, write: boolean = false): Promise<string> {
    return this.runJustAiCommand('fix', recipe, write ? '--write' : '');
  }

  async workflow(request: string, write: boolean = false): Promise<string> {
    return this.runJustAiCommand('workflow', request, write ? '--write' : '');
  }

  async fixBatch(write: boolean = false): Promise<string> {
    return this.runJustAiCommand('fix', '--all-failed', write ? '--write' : '');
  }

  async explainBatch(module?: string): Promise<string> {
    const args = ['explain', '--all'];
    if (module) {
      args.push('--module', module);
    }
    return this.runJustAiCommandWithArgs(args);
  }

  async exportContext(pretty: boolean = false): Promise<string> {
    return this.runJustAiCommand('export-context', pretty ? '--pretty' : '');
  }

  // Migrate commands
  async migrateAnalyze(json: boolean = false, similarityThreshold: number = 0.8): Promise<string> {
    const args = ['migrate', 'analyze'];
    if (json) {
      args.push('--json');
    }
    args.push('--similarity-threshold', similarityThreshold.toString());
    return this.runJustAiCommandWithArgs(args);
  }

  async migrateModularize(write: boolean = false, dryRun: boolean = false): Promise<string> {
    const args = ['migrate', 'modularize'];
    if (write) {
      args.push('--write');
    }
    if (dryRun) {
      args.push('--dry-run');
    }
    return this.runJustAiCommandWithArgs(args);
  }

  async migrateDeduplicate(write: boolean = false, similarityThreshold: number = 0.8, interactive: boolean = false, merge: boolean = false): Promise<string> {
    const args = ['migrate', 'deduplicate'];
    if (write) {
      args.push('--write');
    }
    args.push('--similarity-threshold', similarityThreshold.toString());
    if (interactive) {
      args.push('--interactive');
    }
    if (merge) {
      args.push('--merge');
    }
    return this.runJustAiCommandWithArgs(args);
  }

  async template(request: string): Promise<string> {
    return this.runJustAiCommand('template', request);
  }

  async instantiateTemplate(template: string, values: Record<string, string>, write: boolean = false): Promise<string> {
    const args = ['instantiate-template', template];
    for (const [key, value] of Object.entries(values)) {
      args.push(`${key}=${value}`);
    }
    if (write) {
      args.push('--write');
    }
    return this.runJustAiCommandWithArgs(args);
  }

  async composeWorkflow(request: string, write: boolean = false): Promise<string> {
    return this.runJustAiCommand('compose-workflow', request, write ? '--write' : '');
  }

  async getHistory(limit: number = 20, recipe?: string, success?: boolean): Promise<HistoryRecord[]> {
    const args = ['history', 'recent', '--limit', limit.toString()];
    if (recipe) { args.push('--recipe', recipe); }
    if (success !== undefined) { args.push('--success', success.toString()); }
    args.push('--json');

    const result = await this.runJustAiCommandWithArgs(args);
    return JSON.parse(result);
  }

  private async runJustAiCommand(...args: string[]): Promise<string> {
    const child = spawn('just-ai', args, {
      cwd: this.projectRoot,
      env: { ...process.env },
      stdio: ['ignore', 'pipe', 'pipe']
    });

    let stdout = '';
    let stderr = '';

    child.stdout.on('data', (data) => stdout += data.toString());
    child.stderr.on('data', (data) => stderr += data.toString());

    return new Promise((resolve, reject) => {
      child.on('close', (code) => {
        if (code === 0) {
          resolve(stdout);
        } else {
          reject(new Error(`just-ai ${args.join(' ')} failed: ${stderr}`));
        }
      });
    });
  }

  private async runJustAiCommandWithArgs(args: string[]): Promise<string> {
    const child = spawn('just-ai', args, {
      cwd: this.projectRoot,
      env: { ...process.env },
      stdio: ['ignore', 'pipe', 'pipe']
    });

    let stdout = '';
    let stderr = '';

    child.stdout.on('data', (data) => stdout += data.toString());
    child.stderr.on('data', (data) => stderr += data.toString());

    return new Promise((resolve, reject) => {
      child.on('close', (code) => {
        if (code === 0) {
          resolve(stdout);
        } else {
          reject(new Error(`just-ai ${args.join(' ')} failed: ${stderr}`));
        }
      });
    });
  }

  static getProjectRoot(uri: vscode.Uri): string | undefined {
    const workspaceFolder = vscode.workspace.getWorkspaceFolder(uri);
    if (workspaceFolder) {
      return workspaceFolder.uri.fsPath;
    }

    // Try to find justfile by walking up
    let currentDir = dirname(uri.fsPath);
    while (currentDir !== dirname(currentDir)) {
      if (existsSync(join(currentDir, 'justfile')) ||
          existsSync(join(currentDir, 'Justfile')) ||
          existsSync(join(currentDir, '.just'))) {
        return currentDir;
      }
      currentDir = dirname(currentDir);
    }
    return undefined;
  }
}

export class RiskDiagnostics {
  private collection: vscode.DiagnosticCollection;
  private client: JustAiClient | null = null;
  private enabled: boolean = true;

  constructor(
    private context: vscode.ExtensionContext,
    private outputChannel: vscode.OutputChannel
  ) {
    this.collection = vscode.languages.createDiagnosticCollection('just-ai');
    context.subscriptions.push(this.collection);
  }

  setClient(client: JustAiClient) {
    this.client = client;
  }

  setEnabled(enabled: boolean) {
    this.enabled = enabled;
    if (!enabled) {
      this.clear();
    }
  }

  getCollection(): vscode.DiagnosticCollection {
    return this.collection;
  }

  async refresh(document: vscode.TextDocument) {
    if (!this.enabled || !this.client) {
      return;
    }

    try {
      const report = await this.client.runDoctor(true) as DoctorReport;
      const diagnostics: vscode.Diagnostic[] = [];

      for (const recipe of report.recipes) {
        if (recipe.risk === 'low') continue;

        const severity = this.mapRiskToSeverity(recipe.risk);

        for (const finding of recipe.findings) {
          // Try to find the line in the document
          const lineIndex = this.findLineInDocument(document, finding.line);
          if (lineIndex !== -1) {
            const range = new vscode.Range(lineIndex, 0, lineIndex, 1000);
            const diagnostic = new vscode.Diagnostic(
              range,
              `[just-ai] ${finding.reason}: \`${finding.line.trim()}\``,
              severity
            );
            diagnostic.code = `just-ai-${recipe.risk.toLowerCase()}`;
            diagnostic.source = 'just-ai';
            diagnostics.push(diagnostic);
          }
        }
      }

      this.collection.set(document.uri, diagnostics);
    } catch (error) {
      this.outputChannel.appendLine(`Error refreshing diagnostics: ${error}`);
    }
  }

  private mapRiskToSeverity(risk: string): vscode.DiagnosticSeverity {
    switch (risk) {
      case 'blocked': return vscode.DiagnosticSeverity.Error;
      case 'high': return vscode.DiagnosticSeverity.Warning;
      case 'medium': return vscode.DiagnosticSeverity.Information;
      default: return vscode.DiagnosticSeverity.Hint;
    }
  }

  private findLineInDocument(document: vscode.TextDocument, searchLine: string): number {
    const normalizedSearch = searchLine.trim().toLowerCase();
    for (let i = 0; i < document.lineCount; i++) {
      const line = document.lineAt(i).text.trim().toLowerCase();
      if (line.includes(normalizedSearch) || normalizedSearch.includes(line)) {
        return i;
      }
    }
    return -1;
  }

  clear(): void {
    this.collection.clear();
  }
}