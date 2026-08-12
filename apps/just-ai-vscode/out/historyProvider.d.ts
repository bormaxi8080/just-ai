import * as vscode from 'vscode';
import { JustAiClient } from './client';
export declare class HistoryTreeProvider implements vscode.TreeDataProvider<HistoryTreeItem> {
    private context;
    private _onDidChangeTreeData;
    readonly onDidChangeTreeData: vscode.Event<HistoryTreeItem | undefined | null | void>;
    private records;
    private client;
    constructor(context: vscode.ExtensionContext);
    setClient(client: JustAiClient): void;
    refresh(): void;
    private refreshHistory;
    getTreeItem(element: HistoryTreeItem): vscode.TreeItem;
    getChildren(element?: HistoryTreeItem): Thenable<HistoryTreeItem[]>;
    private groupByRecipe;
    private formatRecord;
}
export declare class HistoryTreeItem extends vscode.TreeItem {
    readonly label: string;
    readonly collapsibleState: vscode.TreeItemCollapsibleState;
    readonly contextValue: string;
    readonly data: any;
    constructor(label: string, collapsibleState: vscode.TreeItemCollapsibleState, contextValue: string, data: any);
}
//# sourceMappingURL=historyProvider.d.ts.map