import * as vscode from 'vscode';
export interface JustRecipe {
    namepath: string;
    risk: 'low' | 'medium' | 'high' | 'blocked';
    risks: Array<{
        level: string;
        line: string;
        reason: string;
    }>;
    body: string[];
    dependencies: string[];
    parameters: Array<{
        name: string;
        default?: string;
    }>;
    doc?: string;
}
export interface ProjectContext {
    recipes: JustRecipe[];
    modules: Array<{
        name: string;
        path: string;
    }>;
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
        findings: Array<{
            level: string;
            line: string;
            reason: string;
        }>;
    }>;
}
export declare class JustAiClient {
    private justBinary;
    private projectRoot;
    constructor(projectRoot: string, justBinary?: string);
    private runJustAi;
    getProjectContext(): Promise<ProjectContext>;
    runDoctor(json?: boolean): Promise<DoctorReport | string>;
    suggest(): Promise<string>;
    explain(recipe: string): Promise<string>;
    add(request: string, write?: boolean): Promise<string>;
    fix(recipe: string, write?: boolean): Promise<string>;
    workflow(request: string, write?: boolean): Promise<string>;
    fixBatch(write?: boolean): Promise<string>;
    explainBatch(module?: string): Promise<string>;
    exportContext(pretty?: boolean): Promise<string>;
    migrateAnalyze(json?: boolean, similarityThreshold?: number): Promise<string>;
    migrateModularize(write?: boolean, dryRun?: boolean): Promise<string>;
    migrateDeduplicate(write?: boolean, similarityThreshold?: number, interactive?: boolean, merge?: boolean): Promise<string>;
    template(request: string): Promise<string>;
    instantiateTemplate(template: string, values: Record<string, string>, write?: boolean): Promise<string>;
    composeWorkflow(request: string, write?: boolean): Promise<string>;
    getHistory(limit?: number, recipe?: string, success?: boolean): Promise<HistoryRecord[]>;
    private runJustAiCommand;
    private runJustAiCommandWithArgs;
    static getProjectRoot(uri: vscode.Uri): string | undefined;
}
export declare class RiskDiagnostics {
    private context;
    private outputChannel;
    private collection;
    private client;
    private enabled;
    constructor(context: vscode.ExtensionContext, outputChannel: vscode.OutputChannel);
    setClient(client: JustAiClient): void;
    setEnabled(enabled: boolean): void;
    getCollection(): vscode.DiagnosticCollection;
    refresh(document: vscode.TextDocument): Promise<void>;
    private mapRiskToSeverity;
    private findLineInDocument;
    clear(): void;
}
//# sourceMappingURL=client.d.ts.map