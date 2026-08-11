import * as vscode from 'vscode';
import { HistoryRecord, JustAiClient } from './client';

export class HistoryTreeProvider implements vscode.TreeDataProvider<HistoryTreeItem> {
  private _onDidChangeTreeData: vscode.EventEmitter<HistoryTreeItem | undefined | null | void> = new vscode.EventEmitter<HistoryTreeItem | undefined | null | void>();
  readonly onDidChangeTreeData: vscode.Event<HistoryTreeItem | undefined | null | void> = this._onDidChangeTreeData.event;

  private records: HistoryRecord[] = [];
  private client: JustAiClient | null = null; // Will be set via setClient

  constructor(private context: vscode.ExtensionContext) {}

  setClient(client: JustAiClient) {
    this.client = client;
  }

  refresh(): void {
    this.refreshHistory().then(() => {
      this._onDidChangeTreeData.fire();
    });
  }

  private async refreshHistory(): Promise<void> {
    if (!this.client) { return; }
    try {
      this.records = await this.client.getHistory(50);
    } catch (error) {
      console.error('Failed to load history:', error);
      this.records = [];
    }
  }

  getTreeItem(element: HistoryTreeItem): vscode.TreeItem {
    return element;
  }

  getChildren(element?: HistoryTreeItem): Thenable<HistoryTreeItem[]> {
    if (!element) {
      // Root level - show recent runs grouped by recipe
      const grouped = this.groupByRecipe(this.records);
      return Promise.resolve(
        Object.entries(grouped).map(([recipe, records]) => {
          const latest = records[0];
          const successCount = records.filter(r => r.success).length;
          const totalCount = records.length;
          return new HistoryTreeItem(
            `${recipe} (${successCount}/${totalCount} passed)`,
            vscode.TreeItemCollapsibleState.Collapsed,
            'recipe-group',
            { recipe, records }
          );
        })
      );
    } else if (element.contextValue === 'recipe-group') {
      // Show individual runs for this recipe
      const { records } = element.data as { records: HistoryRecord[] };
      return Promise.resolve(
        records.map((record: HistoryRecord) => new HistoryTreeItem(
          this.formatRecord(record),
          vscode.TreeItemCollapsibleState.None,
          'history-record',
          record
        ))
      );
    }
    return Promise.resolve([]);
  }

  private groupByRecipe(records: HistoryRecord[]): Record<string, HistoryRecord[]> {
    const grouped: Record<string, HistoryRecord[]> = {};
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

  private formatRecord(record: HistoryRecord): string {
    const date = new Date(record.started_at_ms);
    const timeStr = date.toLocaleTimeString();
    const status = record.success ? '$(pass)' : '$(error)';
    const duration = `${record.duration_ms}ms`;
    const exitCode = record.exit_code !== undefined ? ` exit=${record.exit_code}` : '';
    return `${status} ${timeStr} ${duration}${exitCode}`;
  }
}

export class HistoryTreeItem extends vscode.TreeItem {
  constructor(
    public readonly label: string,
    public readonly collapsibleState: vscode.TreeItemCollapsibleState,
    public readonly contextValue: string,
    public readonly data: any
  ) {
    super(label, collapsibleState);
    this.contextValue = contextValue;

    if (contextValue === 'recipe-group') {
      this.iconPath = new vscode.ThemeIcon('symbol-method');
      this.tooltip = `Recipe: ${data.recipe} (${data.records.length} runs)`;
    } else if (contextValue === 'history-record') {
      const record = data as HistoryRecord;
      this.iconPath = new vscode.ThemeIcon(record.success ? 'check' : 'error');
      this.tooltip = new vscode.MarkdownString(
        `**Recipe:** ${record.recipe}\n` +
        `**Status:** ${record.success ? 'Success' : 'Failed'}\n` +
        `**Exit Code:** ${record.exit_code ?? 'N/A'}\n` +
        `**Duration:** ${record.duration_ms}ms\n` +
        `**Started:** ${new Date(record.started_at_ms).toLocaleString()}\n\n` +
        `**Stdout:**\n\`\`\`\n${record.stdout_tail || '(empty)'}\n\`\`\`\n\n` +
        `**Stderr:**\n\`\`\`\n${record.stderr_tail || '(empty)'}\n\`\`\``
      );
      this.command = {
        command: 'just-ai.showHistoryDetail',
        title: 'Show Details',
        arguments: [record]
      };
    }
  }
}