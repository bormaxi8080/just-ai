"use strict";
var __createBinding = (this && this.__createBinding) || (Object.create ? (function(o, m, k, k2) {
    if (k2 === undefined) k2 = k;
    var desc = Object.getOwnPropertyDescriptor(m, k);
    if (!desc || ("get" in desc ? !m.__esModule : desc.writable || desc.configurable)) {
      desc = { enumerable: true, get: function() { return m[k]; } };
    }
    Object.defineProperty(o, k2, desc);
}) : (function(o, m, k, k2) {
    if (k2 === undefined) k2 = k;
    o[k2] = m[k];
}));
var __setModuleDefault = (this && this.__setModuleDefault) || (Object.create ? (function(o, v) {
    Object.defineProperty(o, "default", { enumerable: true, value: v });
}) : function(o, v) {
    o["default"] = v;
});
var __importStar = (this && this.__importStar) || (function () {
    var ownKeys = function(o) {
        ownKeys = Object.getOwnPropertyNames || function (o) {
            var ar = [];
            for (var k in o) if (Object.prototype.hasOwnProperty.call(o, k)) ar[ar.length] = k;
            return ar;
        };
        return ownKeys(o);
    };
    return function (mod) {
        if (mod && mod.__esModule) return mod;
        var result = {};
        if (mod != null) for (var k = ownKeys(mod), i = 0; i < k.length; i++) if (k[i] !== "default") __createBinding(result, mod, k[i]);
        __setModuleDefault(result, mod);
        return result;
    };
})();
Object.defineProperty(exports, "__esModule", { value: true });
exports.RiskDiagnostics = exports.JustAiClient = void 0;
const vscode = __importStar(require("vscode"));
const child_process_1 = require("child_process");
const path_1 = require("path");
const fs_1 = require("fs");
class JustAiClient {
    justBinary;
    projectRoot;
    constructor(projectRoot, justBinary = 'just') {
        this.projectRoot = projectRoot;
        this.justBinary = justBinary;
    }
    runJustAi(args) {
        return new Promise((resolve, reject) => {
            const env = { ...process.env };
            // Ensure we inherit the user's environment for API keys
            const child = (0, child_process_1.spawn)(this.justBinary, ['--dump', '--dump-format', 'json'], {
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
                }
                else {
                    reject(new Error(`just --dump failed: ${stderr}`));
                }
            });
        });
    }
    async getProjectContext() {
        const output = await this.runJustAi([]);
        return JSON.parse(output);
    }
    async runDoctor(json = false) {
        return new Promise((resolve, reject) => {
            const args = ['doctor'];
            if (json) {
                args.push('--json');
            }
            const child = (0, child_process_1.spawn)(this.justBinary, args, {
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
                    }
                    else {
                        resolve(stdout);
                    }
                }
                else {
                    reject(new Error(`just-ai doctor failed: ${stderr}`));
                }
            });
        });
    }
    async suggest() {
        return this.runJustAiCommand('suggest');
    }
    async explain(recipe) {
        return this.runJustAiCommand('explain', recipe);
    }
    async add(request, write = false) {
        return this.runJustAiCommand('add', request, write ? '--write' : '');
    }
    async fix(recipe, write = false) {
        return this.runJustAiCommand('fix', recipe, write ? '--write' : '');
    }
    async workflow(request, write = false) {
        return this.runJustAiCommand('workflow', request, write ? '--write' : '');
    }
    async fixBatch(write = false) {
        return this.runJustAiCommand('fix', '--all-failed', write ? '--write' : '');
    }
    async explainBatch(module) {
        const args = ['explain', '--all'];
        if (module) {
            args.push('--module', module);
        }
        return this.runJustAiCommandWithArgs(args);
    }
    async exportContext(pretty = false) {
        return this.runJustAiCommand('export-context', pretty ? '--pretty' : '');
    }
    // Migrate commands
    async migrateAnalyze(json = false, similarityThreshold = 0.8) {
        const args = ['migrate', 'analyze'];
        if (json) {
            args.push('--json');
        }
        args.push('--similarity-threshold', similarityThreshold.toString());
        return this.runJustAiCommandWithArgs(args);
    }
    async migrateModularize(write = false, dryRun = false) {
        const args = ['migrate', 'modularize'];
        if (write) {
            args.push('--write');
        }
        if (dryRun) {
            args.push('--dry-run');
        }
        return this.runJustAiCommandWithArgs(args);
    }
    async migrateDeduplicate(write = false, similarityThreshold = 0.8, interactive = false, merge = false) {
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
    async template(request) {
        return this.runJustAiCommand('template', request);
    }
    async instantiateTemplate(template, values, write = false) {
        const args = ['instantiate-template', template];
        for (const [key, value] of Object.entries(values)) {
            args.push(`${key}=${value}`);
        }
        if (write) {
            args.push('--write');
        }
        return this.runJustAiCommandWithArgs(args);
    }
    async composeWorkflow(request, write = false) {
        return this.runJustAiCommand('compose-workflow', request, write ? '--write' : '');
    }
    async getHistory(limit = 20, recipe, success) {
        const args = ['history', 'recent', '--limit', limit.toString()];
        if (recipe) {
            args.push('--recipe', recipe);
        }
        if (success !== undefined) {
            args.push('--success', success.toString());
        }
        args.push('--json');
        const result = await this.runJustAiCommandWithArgs(args);
        return JSON.parse(result);
    }
    async runJustAiCommand(...args) {
        const child = (0, child_process_1.spawn)('just-ai', args, {
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
                }
                else {
                    reject(new Error(`just-ai ${args.join(' ')} failed: ${stderr}`));
                }
            });
        });
    }
    async runJustAiCommandWithArgs(args) {
        const child = (0, child_process_1.spawn)('just-ai', args, {
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
                }
                else {
                    reject(new Error(`just-ai ${args.join(' ')} failed: ${stderr}`));
                }
            });
        });
    }
    static getProjectRoot(uri) {
        const workspaceFolder = vscode.workspace.getWorkspaceFolder(uri);
        if (workspaceFolder) {
            return workspaceFolder.uri.fsPath;
        }
        // Try to find justfile by walking up
        let currentDir = (0, path_1.dirname)(uri.fsPath);
        while (currentDir !== (0, path_1.dirname)(currentDir)) {
            if ((0, fs_1.existsSync)((0, path_1.join)(currentDir, 'justfile')) ||
                (0, fs_1.existsSync)((0, path_1.join)(currentDir, 'Justfile')) ||
                (0, fs_1.existsSync)((0, path_1.join)(currentDir, '.just'))) {
                return currentDir;
            }
            currentDir = (0, path_1.dirname)(currentDir);
        }
        return undefined;
    }
}
exports.JustAiClient = JustAiClient;
class RiskDiagnostics {
    context;
    outputChannel;
    collection;
    client = null;
    enabled = true;
    constructor(context, outputChannel) {
        this.context = context;
        this.outputChannel = outputChannel;
        this.collection = vscode.languages.createDiagnosticCollection('just-ai');
        context.subscriptions.push(this.collection);
    }
    setClient(client) {
        this.client = client;
    }
    setEnabled(enabled) {
        this.enabled = enabled;
        if (!enabled) {
            this.clear();
        }
    }
    getCollection() {
        return this.collection;
    }
    async refresh(document) {
        if (!this.enabled || !this.client) {
            return;
        }
        try {
            const report = await this.client.runDoctor(true);
            const diagnostics = [];
            for (const recipe of report.recipes) {
                if (recipe.risk === 'low')
                    continue;
                const severity = this.mapRiskToSeverity(recipe.risk);
                for (const finding of recipe.findings) {
                    // Try to find the line in the document
                    const lineIndex = this.findLineInDocument(document, finding.line);
                    if (lineIndex !== -1) {
                        const range = new vscode.Range(lineIndex, 0, lineIndex, 1000);
                        const diagnostic = new vscode.Diagnostic(range, `[just-ai] ${finding.reason}: \`${finding.line.trim()}\``, severity);
                        diagnostic.code = `just-ai-${recipe.risk.toLowerCase()}`;
                        diagnostic.source = 'just-ai';
                        diagnostics.push(diagnostic);
                    }
                }
            }
            this.collection.set(document.uri, diagnostics);
        }
        catch (error) {
            this.outputChannel.appendLine(`Error refreshing diagnostics: ${error}`);
        }
    }
    mapRiskToSeverity(risk) {
        switch (risk) {
            case 'blocked': return vscode.DiagnosticSeverity.Error;
            case 'high': return vscode.DiagnosticSeverity.Warning;
            case 'medium': return vscode.DiagnosticSeverity.Information;
            default: return vscode.DiagnosticSeverity.Hint;
        }
    }
    findLineInDocument(document, searchLine) {
        const normalizedSearch = searchLine.trim().toLowerCase();
        for (let i = 0; i < document.lineCount; i++) {
            const line = document.lineAt(i).text.trim().toLowerCase();
            if (line.includes(normalizedSearch) || normalizedSearch.includes(line)) {
                return i;
            }
        }
        return -1;
    }
    clear() {
        this.collection.clear();
    }
}
exports.RiskDiagnostics = RiskDiagnostics;
//# sourceMappingURL=client.js.map