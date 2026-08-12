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
exports.HistoryTreeItem = exports.HistoryTreeProvider = void 0;
const vscode = __importStar(require("vscode"));
class HistoryTreeProvider {
    context;
    _onDidChangeTreeData = new vscode.EventEmitter();
    onDidChangeTreeData = this._onDidChangeTreeData.event;
    records = [];
    client = null; // Will be set via setClient
    constructor(context) {
        this.context = context;
    }
    setClient(client) {
        this.client = client;
    }
    refresh() {
        this.refreshHistory().then(() => {
            this._onDidChangeTreeData.fire();
        });
    }
    async refreshHistory() {
        if (!this.client) {
            return;
        }
        try {
            this.records = await this.client.getHistory(50);
        }
        catch (error) {
            console.error('Failed to load history:', error);
            this.records = [];
        }
    }
    getTreeItem(element) {
        return element;
    }
    getChildren(element) {
        if (!element) {
            // Root level - show recent runs grouped by recipe
            const grouped = this.groupByRecipe(this.records);
            return Promise.resolve(Object.entries(grouped).map(([recipe, records]) => {
                const latest = records[0];
                const successCount = records.filter(r => r.success).length;
                const totalCount = records.length;
                return new HistoryTreeItem(`${recipe} (${successCount}/${totalCount} passed)`, vscode.TreeItemCollapsibleState.Collapsed, 'recipe-group', { recipe, records });
            }));
        }
        else if (element.contextValue === 'recipe-group') {
            // Show individual runs for this recipe
            const { records } = element.data;
            return Promise.resolve(records.map((record) => new HistoryTreeItem(this.formatRecord(record), vscode.TreeItemCollapsibleState.None, 'history-record', record)));
        }
        return Promise.resolve([]);
    }
    groupByRecipe(records) {
        const grouped = {};
        for (const record of records) {
            if (!grouped[record.recipe]) {
                grouped[record.recipe] = [];
            }
            grouped[record.recipe].push(record);
        }
        // Sort each group by time (newest first)
        for (const recipe of Object.keys(grouped)) {
            grouped[recipe].sort((a, b) => b.started_at_ms - a.started_at_ms);
        }
        return grouped;
    }
    formatRecord(record) {
        const date = new Date(record.started_at_ms);
        const timeStr = date.toLocaleTimeString();
        const status = record.success ? '$(pass)' : '$(error)';
        const duration = `${record.duration_ms}ms`;
        const exitCode = record.exit_code !== undefined ? ` exit=${record.exit_code}` : '';
        return `${status} ${timeStr} ${duration}${exitCode}`;
    }
}
exports.HistoryTreeProvider = HistoryTreeProvider;
class HistoryTreeItem extends vscode.TreeItem {
    label;
    collapsibleState;
    contextValue;
    data;
    constructor(label, collapsibleState, contextValue, data) {
        super(label, collapsibleState);
        this.label = label;
        this.collapsibleState = collapsibleState;
        this.contextValue = contextValue;
        this.data = data;
        this.contextValue = contextValue;
        if (contextValue === 'recipe-group') {
            this.iconPath = new vscode.ThemeIcon('symbol-method');
            this.tooltip = `Recipe: ${data.recipe} (${data.records.length} runs)`;
        }
        else if (contextValue === 'history-record') {
            const record = data;
            this.iconPath = new vscode.ThemeIcon(record.success ? 'check' : 'error');
            this.tooltip = new vscode.MarkdownString(`**Recipe:** ${record.recipe}\n` +
                `**Status:** ${record.success ? 'Success' : 'Failed'}\n` +
                `**Exit Code:** ${record.exit_code ?? 'N/A'}\n` +
                `**Duration:** ${record.duration_ms}ms\n` +
                `**Started:** ${new Date(record.started_at_ms).toLocaleString()}\n\n` +
                `**Stdout:**\n\`\`\`\n${record.stdout_tail || '(empty)'}\n\`\`\`\n\n` +
                `**Stderr:**\n\`\`\`\n${record.stderr_tail || '(empty)'}\n\`\`\``);
            this.command = {
                command: 'just-ai.showHistoryDetail',
                title: 'Show Details',
                arguments: [record]
            };
        }
    }
}
exports.HistoryTreeItem = HistoryTreeItem;
//# sourceMappingURL=historyProvider.js.map