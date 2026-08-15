import * as vscode from 'vscode';
import { JustAiClient, RiskDiagnostics } from './client';
import { HistoryTreeProvider, HistoryTreeItem } from './historyProvider';

let riskDiagnostics: RiskDiagnostics;
let historyProvider: HistoryTreeProvider;
let justAiClient: JustAiClient;
let outputChannel: vscode.OutputChannel;

export function activate(context: vscode.ExtensionContext): void {
  console.log('just-ai extension is now active!');

  outputChannel = vscode.window.createOutputChannel('just-ai');
  context.subscriptions.push(outputChannel);

  // Create the just-ai client
  const projectRoot = getProjectRoot();
  if (!projectRoot) {
    vscode.window.showWarningMessage('just-ai: No justfile found in workspace. Some features will be limited.');
  }

  const config = vscode.workspace.getConfiguration('just-ai');
  const justBinary = config.get<string>('justBinary', 'just');
  justAiClient = new JustAiClient(projectRoot || '', justBinary);

  // Initialize risk diagnostics provider
  riskDiagnostics = new RiskDiagnostics(context, outputChannel);
  riskDiagnostics.setClient(justAiClient);
  riskDiagnostics.setEnabled(config.get<boolean>('enableRiskDiagnostics', true));

  // Initialize history tree provider
  historyProvider = new HistoryTreeProvider(context);
  historyProvider.setClient(justAiClient);

  // Register tree view
  const historyTreeView = vscode.window.createTreeView('just-ai.history', {
    treeDataProvider: historyProvider,
    showCollapseAll: true
  });
  context.subscriptions.push(historyTreeView);

  // Register commands
  registerCommands(context);

  // Register document change listeners for diagnostics
  context.subscriptions.push(
    vscode.workspace.onDidChangeTextDocument(event => {
      if (event.document.languageId === 'just') {
        riskDiagnostics.refresh(event.document);
      }
    })
  );

  context.subscriptions.push(
    vscode.workspace.onDidOpenTextDocument(document => {
      if (document.languageId === 'just') {
        riskDiagnostics.refresh(document);
      }
    })
  );

  context.subscriptions.push(
    vscode.workspace.onDidSaveTextDocument(document => {
      if (document.languageId === 'just') {
        riskDiagnostics.refresh(document);
      }
    })
  );

  // Refresh diagnostics on config change
  context.subscriptions.push(
    vscode.workspace.onDidChangeConfiguration(event => {
      if (event.affectsConfiguration('just-ai.enableRiskDiagnostics')) {
        const enabled = config.get<boolean>('enableRiskDiagnostics', true);
        riskDiagnostics.setEnabled(enabled);
        if (enabled && vscode.window.activeTextEditor?.document.languageId === 'just') {
          riskDiagnostics.refresh(vscode.window.activeTextEditor.document);
        }
      }
      if (event.affectsConfiguration('just-ai.justBinary')) {
        const newBinary = config.get<string>('justBinary', 'just');
        justAiClient = new JustAiClient(projectRoot || '', newBinary);
        riskDiagnostics.setClient(justAiClient);
        historyProvider.setClient(justAiClient);
      }
    })
  );

  // Initial diagnostics for open justfiles
  for (const document of vscode.workspace.textDocuments) {
    if (document.languageId === 'just') {
      riskDiagnostics.refresh(document);
    }
  }
}

function registerCommands(context: vscode.ExtensionContext): void {
  // Doctor command
  context.subscriptions.push(
    vscode.commands.registerCommand('just-ai.doctor', async () => {
      await runDoctor();
    })
  );

  // Suggest command
  context.subscriptions.push(
    vscode.commands.registerCommand('just-ai.suggest', async () => {
      await runSuggest();
    })
  );

  // Explain command
  context.subscriptions.push(
    vscode.commands.registerCommand('just-ai.explain', async (uri?: vscode.Uri) => {
      await runExplain(uri);
    })
  );

  // Add command
  context.subscriptions.push(
    vscode.commands.registerCommand('just-ai.add', async () => {
      await runAdd();
    })
  );

  // Fix command
  context.subscriptions.push(
    vscode.commands.registerCommand('just-ai.fix', async () => {
      await runFix();
    })
  );

  // Workflow command
  context.subscriptions.push(
    vscode.commands.registerCommand('just-ai.workflow', async () => {
      await runWorkflow();
    })
  );

  // Fix Batch command
  context.subscriptions.push(
    vscode.commands.registerCommand('just-ai.fixBatch', async () => {
      await runFixBatch();
    })
  );

  // Explain Batch command
  context.subscriptions.push(
    vscode.commands.registerCommand('just-ai.explainBatch', async () => {
      await runExplainBatch();
    })
  );

  // Migrate Analyze command
  context.subscriptions.push(
    vscode.commands.registerCommand('just-ai.migrateAnalyze', async () => {
      await runMigrateAnalyze();
    })
  );

  // Migrate Modularize command
  context.subscriptions.push(
    vscode.commands.registerCommand('just-ai.migrateModularize', async () => {
      await runMigrateModularize();
    })
  );

  // Migrate Deduplicate command
  context.subscriptions.push(
    vscode.commands.registerCommand('just-ai.migrateDeduplicate', async () => {
      await runMigrateDeduplicate();
    })
  );

  // Export context command
  context.subscriptions.push(
    vscode.commands.registerCommand('just-ai.exportContext', async () => {
      await runExportContext();
    })
  );

  // Open history panel
  context.subscriptions.push(
    vscode.commands.registerCommand('just-ai.openHistory', () => {
      vscode.commands.executeCommand('workbench.view.extension.just-ai');
    })
  );

  // Refresh history
  context.subscriptions.push(
    vscode.commands.registerCommand('just-ai.refreshHistory', () => {
      historyProvider.refresh();
    })
  );

  // Show history detail
  context.subscriptions.push(
    vscode.commands.registerCommand('just-ai.showHistoryDetail', (record: any) => {
      showHistoryDetail(record);
    })
  );

  // Configure settings
  context.subscriptions.push(
    vscode.commands.registerCommand('just-ai.configure', () => {
      vscode.commands.executeCommand('workbench.action.openSettings', 'just-ai');
    })
  );

  // Quick pick for recipe commands
  context.subscriptions.push(
    vscode.commands.registerCommand('just-ai.runRecipe', async () => {
      await runRecipeCommand();
    })
  );
}

async function runDoctor(): Promise<void> {
  outputChannel.clear();
  outputChannel.show();
  outputChannel.appendLine('Running just-ai doctor...');

  try {
    const result = await justAiClient.runDoctor(false) as string;
    outputChannel.appendLine(result);

    if (result.includes('blocked')) {
      vscode.window.showWarningMessage('just-ai: Blocked-risk recipes found. Check Output panel for details.');
    } else if (result.includes('high') || result.includes('medium')) {
      vscode.window.showInformationMessage('just-ai: Doctor completed with findings. Check Output panel for details.');
    } else {
      vscode.window.showInformationMessage('just-ai: Doctor completed. All recipes are low risk.');
    }
  } catch (error) {
    const msg = `Doctor failed: ${error}`;
    outputChannel.appendLine(msg);
    vscode.window.showErrorMessage(msg);
  }
}

async function runSuggest(): Promise<void> {
  outputChannel.clear();
  outputChannel.show();
  outputChannel.appendLine('Asking AI for recipe suggestions...');

  try {
    const result = await justAiClient.suggest();
    outputChannel.appendLine(result);

    // Show in a new document for better readability
    const doc = await vscode.workspace.openTextDocument({
      content: result,
      language: 'markdown'
    });
    await vscode.window.showTextDocument(doc);
  } catch (error) {
    const msg = `Suggest failed: ${error}`;
    outputChannel.appendLine(msg);
    vscode.window.showErrorMessage(msg);
  }
}

async function runExplain(uri?: vscode.Uri): Promise<void> {
  let recipe: string | undefined;

  if (uri) {
    // Try to extract recipe from context or selection
    const document = await vscode.workspace.openTextDocument(uri);
    const selection = vscode.window.activeTextEditor?.selection;
    if (selection && !selection.isEmpty) {
      const text = document.getText(selection);
      // Try to find recipe name in selection
      const match = text.match(/^(\w[\w-]*)\s*:/m);
      if (match) {
        recipe = match[1];
      }
    }
  }

  if (!recipe) {
    // Ask user to pick a recipe
    try {
      const context = await justAiClient.getProjectContext();
      const recipeNames = context.recipes.map(r => r.namepath);
      recipe = await vscode.window.showQuickPick(recipeNames, {
        placeHolder: 'Select a recipe to explain'
      });
    } catch (error) {
      vscode.window.showErrorMessage(`Failed to load recipes: ${error}`);
      return;
    }
  }

  if (!recipe) { return; }

  outputChannel.clear();
  outputChannel.show();
  outputChannel.appendLine(`Explaining recipe: ${recipe}...`);

  try {
    const result = await justAiClient.explain(recipe);
    outputChannel.appendLine(result);

    const doc = await vscode.workspace.openTextDocument({
      content: result,
      language: 'markdown'
    });
    await vscode.window.showTextDocument(doc);
  } catch (error) {
    const msg = `Explain failed: ${error}`;
    outputChannel.appendLine(msg);
    vscode.window.showErrorMessage(msg);
  }
}

async function runAdd(): Promise<void> {
  const request = await vscode.window.showInputBox({
    placeHolder: 'Describe the recipe you want to add (e.g., "run tests with coverage")',
    prompt: 'Enter a natural-language description of the recipe to create',
    validateInput: value => value.trim().length < 10 ? 'Please provide a more detailed description' : null
  });

  if (!request) { return; }

  const write = await vscode.window.showQuickPick(['Preview only', 'Write to justfile'], {
    placeHolder: 'Apply the generated recipe?'
  });

  if (!write) { return; }

  outputChannel.clear();
  outputChannel.show();
  outputChannel.appendLine(`Generating recipe for: ${request}...`);

  try {
    const result = await justAiClient.add(request, write === 'Write to justfile');
    outputChannel.appendLine(result);

    if (write === 'Write to justfile') {
      vscode.window.showInformationMessage('Recipe added to justfile!');
    } else {
      const doc = await vscode.workspace.openTextDocument({
        content: result,
        language: 'markdown'
      });
      await vscode.window.showTextDocument(doc);
    }
  } catch (error) {
    const msg = `Add failed: ${error}`;
    outputChannel.appendLine(msg);
    vscode.window.showErrorMessage(msg);
  }
}

async function runFix(): Promise<void> {
  try {
    const context = await justAiClient.getProjectContext();
    const recipeNames = context.recipes.map(r => r.namepath);

    const recipe = await vscode.window.showQuickPick(recipeNames, {
      placeHolder: 'Select a failed recipe to fix'
    });

    if (!recipe) { return; }

    const write = await vscode.window.showQuickPick(['Preview only', 'Write fix to justfile'], {
      placeHolder: 'Apply the generated fix?'
    });

    if (!write) { return; }

    outputChannel.clear();
    outputChannel.show();
    outputChannel.appendLine(`Generating fix for: ${recipe}...`);

    const result = await justAiClient.fix(recipe, write === 'Write fix to justfile');
    outputChannel.appendLine(result);

    if (write === 'Write fix to justfile') {
      vscode.window.showInformationMessage('Fix applied to justfile!');
    } else {
      const doc = await vscode.workspace.openTextDocument({
        content: result,
        language: 'markdown'
      });
      await vscode.window.showTextDocument(doc);
    }
  } catch (error) {
    const msg = `Fix failed: ${error}`;
    outputChannel.appendLine(msg);
    vscode.window.showErrorMessage(msg);
  }
}

async function runExportContext(): Promise<void> {
  outputChannel.clear();
  outputChannel.show();
  outputChannel.appendLine('Exporting project context...');

  try {
    const result = await justAiClient.exportContext(true);
    outputChannel.appendLine('Context exported successfully.');

    const doc = await vscode.workspace.openTextDocument({
      content: result,
      language: 'json'
    });
    await vscode.window.showTextDocument(doc);
  } catch (error) {
    const msg = `Export failed: ${error}`;
    outputChannel.appendLine(msg);
    vscode.window.showErrorMessage(msg);
  }
}

async function runRecipeCommand(): Promise<void> {
  try {
    const context = await justAiClient.getProjectContext();
    const recipeNames = context.recipes.map(r => r.namepath);

    const recipe = await vscode.window.showQuickPick(recipeNames, {
      placeHolder: 'Select a recipe to run'
    });

    if (!recipe) { return; }

    // For now, just run the recipe via terminal
    const terminal = vscode.window.createTerminal({
      name: `just ${recipe}`,
      cwd: vscode.workspace.workspaceFolders?.[0].uri.fsPath
    });
    terminal.show();
    terminal.sendText(`just ${recipe}`);
  } catch (error) {
    vscode.window.showErrorMessage(`Failed to run recipe: ${error}`);
  }
}

async function runWorkflow(): Promise<void> {
  const request = await vscode.window.showInputBox({
    placeHolder: 'Describe the workflow you want to create (e.g., "CI/CD pipeline with build, test, and deploy")',
    prompt: 'Enter a natural-language description of the multi-recipe workflow',
    validateInput: value => value.trim().length < 10 ? 'Please provide a more detailed description' : null
  });

  if (!request) { return; }

  const write = await vscode.window.showQuickPick(['Preview only', 'Write to justfile'], {
    placeHolder: 'Apply the generated workflow?'
  });

  if (!write) { return; }

  outputChannel.clear();
  outputChannel.show();
  outputChannel.appendLine(`Generating workflow for: ${request}...`);

  try {
    const result = await justAiClient.workflow(request, write === 'Write to justfile');
    outputChannel.appendLine(result);

    if (write === 'Write to justfile') {
      vscode.window.showInformationMessage('Workflow added to justfile!');
    } else {
      const doc = await vscode.workspace.openTextDocument({
        content: result,
        language: 'markdown'
      });
      await vscode.window.showTextDocument(doc);
    }
  } catch (error) {
    const msg = `Workflow failed: ${error}`;
    outputChannel.appendLine(msg);
    vscode.window.showErrorMessage(msg);
  }
}

async function runFixBatch(): Promise<void> {
  const write = await vscode.window.showQuickPick(['Preview only', 'Write fixes to justfile'], {
    placeHolder: 'Apply fixes to all failed recipes?'
  });

  if (!write) { return; }

  outputChannel.clear();
  outputChannel.show();
  outputChannel.appendLine('Generating fixes for all failed recipes...');

  try {
    const result = await justAiClient.fixBatch(write === 'Write fixes to justfile');
    outputChannel.appendLine(result);

    if (write === 'Write fixes to justfile') {
      vscode.window.showInformationMessage('Fixes applied to justfile!');
    } else {
      const doc = await vscode.workspace.openTextDocument({
        content: result,
        language: 'markdown'
      });
      await vscode.window.showTextDocument(doc);
    }
  } catch (error) {
    const msg = `Fix batch failed: ${error}`;
    outputChannel.appendLine(msg);
    vscode.window.showErrorMessage(msg);
  }
}

async function runExplainBatch(): Promise<void> {
  // First try to get modules from project context for filtering
  let module: string | undefined;

  try {
    const context = await justAiClient.getProjectContext();
    const modules = context.modules.map(m => m.name);

    const moduleChoice = await vscode.window.showQuickPick(
      ['All recipes', ...modules],
      { placeHolder: 'Select module to explain (or all recipes)' }
    );

    if (!moduleChoice) { return; }
    if (moduleChoice !== 'All recipes') {
      module = moduleChoice;
    }
  } catch (error) {
    // If we can't get modules, just run without filter
  }

  outputChannel.clear();
  outputChannel.show();
  outputChannel.appendLine(module
    ? `Explaining all recipes in module: ${module}...`
    : 'Explaining all recipes...'
  );

  try {
    const result = await justAiClient.explainBatch(module);
    outputChannel.appendLine(result);

    const doc = await vscode.workspace.openTextDocument({
      content: result,
      language: 'markdown'
    });
    await vscode.window.showTextDocument(doc);
  } catch (error) {
    const msg = `Explain batch failed: ${error}`;
    outputChannel.appendLine(msg);
    vscode.window.showErrorMessage(msg);
  }
}

async function runMigrateAnalyze(): Promise<void> {
  const json = await vscode.window.showQuickPick(['Human-readable', 'JSON'], {
    placeHolder: 'Output format'
  });

  if (!json) { return; }

  const similarityThresholdInput = await vscode.window.showInputBox({
    placeHolder: 'Similarity threshold (0.0-1.0)',
    prompt: 'Recipes above this threshold are considered similar',
    value: '0.8',
    validateInput: value => {
      const num = parseFloat(value);
      if (isNaN(num) || num < 0 || num > 1) {
        return 'Must be a number between 0.0 and 1.0';
      }
      return null;
    }
  });

  const similarityThreshold = similarityThresholdInput ? parseFloat(similarityThresholdInput) : 0.8;

  outputChannel.clear();
  outputChannel.show();
  outputChannel.appendLine(`Analyzing project structure...`);

  try {
    const result = await justAiClient.migrateAnalyze(json === 'JSON', similarityThreshold);
    outputChannel.appendLine(result);

    if (json === 'JSON') {
      const doc = await vscode.workspace.openTextDocument({
        content: result,
        language: 'json'
      });
      await vscode.window.showTextDocument(doc);
    }
  } catch (error) {
    const msg = `Migrate analyze failed: ${error}`;
    outputChannel.appendLine(msg);
    vscode.window.showErrorMessage(msg);
  }
}

async function runMigrateModularize(): Promise<void> {
  const mode = await vscode.window.showQuickPick(['Dry run (preview)', 'Write changes'], {
    placeHolder: 'Apply modularization?'
  });

  if (!mode) { return; }

  const write = mode === 'Write changes';
  const dryRun = !write; // dry_run flag for preview

  outputChannel.clear();
  outputChannel.show();
  outputChannel.appendLine(`Modularizing project...`);

  try {
    const result = await justAiClient.migrateModularize(write, dryRun);
    outputChannel.appendLine(result);

    if (write) {
      vscode.window.showInformationMessage('Modularization applied to justfile!');
    }
  } catch (error) {
    const msg = `Migrate modularize failed: ${error}`;
    outputChannel.appendLine(msg);
    vscode.window.showErrorMessage(msg);
  }
}

async function runMigrateDeduplicate(): Promise<void> {
  const mode = await vscode.window.showQuickPick(['Dry run (preview)', 'Write changes'], {
    placeHolder: 'Apply deduplication?'
  });

  if (!mode) { return; }

  const write = mode === 'Write changes';

  const similarityThresholdInput = await vscode.window.showInputBox({
    placeHolder: 'Similarity threshold (0.0-1.0)',
    prompt: 'Recipes above this threshold are considered similar',
    value: '0.8',
    validateInput: value => {
      const num = parseFloat(value);
      if (isNaN(num) || num < 0 || num > 1) {
        return 'Must be a number between 0.0 and 1.0';
      }
      return null;
    }
  });

  const similarityThreshold = similarityThresholdInput ? parseFloat(similarityThresholdInput) : 0.8;

  const merge = await vscode.window.showQuickPick(['Remove one', 'Smart merge (combine both)'], {
    placeHolder: 'How to handle duplicates?'
  });

  if (!merge) { return; }

  const mergeFlag = merge === 'Smart merge (combine both)';

  outputChannel.clear();
  outputChannel.show();
  outputChannel.appendLine(`Deduplicating recipes...`);

  try {
    const result = await justAiClient.migrateDeduplicate(write, similarityThreshold, false, mergeFlag);
    outputChannel.appendLine(result);

    if (write) {
      vscode.window.showInformationMessage('Deduplication applied to justfile!');
    }
  } catch (error) {
    const msg = `Migrate deduplicate failed: ${error}`;
    outputChannel.appendLine(msg);
    vscode.window.showErrorMessage(msg);
  }
}

function showHistoryDetail(record: any): void {
  const panel = vscode.window.createWebviewPanel(
    'just-ai.historyDetail',
    `History: ${record.recipe}`,
    vscode.ViewColumn.One,
    { enableScripts: true }
  );

  panel.webview.html = `
    <!DOCTYPE html>
    <html>
    <head>
      <style>
        body { font-family: var(--vscode-font-family); padding: 16px; line-height: 1.6; }
        .section { margin-bottom: 16px; border-bottom: 1px solid var(--vscode-panel-border); padding-bottom: 16px; }
        .section h3 { margin-top: 0; color: var(--vscode-foreground); }
        pre { background: var(--vscode-textCodeBlock-background); padding: 12px; border-radius: 4px; overflow: auto; }
        code { font-family: var(--vscode-editor-font-family); }
        .status { padding: 4px 8px; border-radius: 4px; display: inline-block; }
        .success { background: var(--vscode-testing-iconPassed); color: var(--vscode-testing-iconPassed); }
        .failed { background: var(--vscode-testing-iconFailed); color: var(--vscode-testing-iconFailed); }
        .metadata { display: grid; grid-template-columns: 120px 1fr; gap: 4px; }
        .label { color: var(--vscode-descriptionForeground); }
      </style>
    </head>
    <body>
      <h2>Recipe: ${record.recipe}</h2>
      <div class="section">
        <h3>Run Details</h3>
        <div class="metadata">
          <span class="label">Status:</span>
          <span class="status ${record.success ? 'success' : 'failed'}">${record.success ? 'Success' : 'Failed'}</span>
          <span class="label">Exit Code:</span>
          <span>${record.exit_code ?? 'N/A'}</span>
          <span class="label">Duration:</span>
          <span>${record.duration_ms}ms</span>
          <span class="label">Started:</span>
          <span>${new Date(record.started_at_ms).toLocaleString()}</span>
        </div>
      </div>
      <div class="section">
        <h3>Stdout</h3>
        <pre><code>${escapeHtml(record.stdout_tail || '(empty)')}</code></pre>
      </div>
      <div class="section">
        <h3>Stderr</h3>
        <pre><code>${escapeHtml(record.stderr_tail || '(empty)')}</code></pre>
      </div>
    </body>
    </html>
  `;

  function escapeHtml(text: string): string {
    return text
      .replace(/&/g, '&')
      .replace(/</g, '<')
      .replace(/>/g, '>')
      .replace(/"/g, '"')
      .replace(/'/g, '&#039;');
  }
}

function getProjectRoot(): string | undefined {
  const workspaceFolders = vscode.workspace.workspaceFolders;
  if (workspaceFolders && workspaceFolders.length > 0) {
    return workspaceFolders[0].uri.fsPath;
  }
  return undefined;
}

export function deactivate(): void {
  console.log('just-ai extension deactivated');
}