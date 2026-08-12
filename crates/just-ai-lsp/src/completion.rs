//! Completion provider for justfiles

use crate::analysis::ProjectAnalysis;
use lsp_types::*;

/// Provide completions for justfiles
pub fn provide_completions(
  text: &str,
  position: Position,
  analysis: Option<&ProjectAnalysis>,
) -> Vec<CompletionItem> {
  let mut completions = Vec::new();

  let line = text.lines().nth(position.line as usize).unwrap_or("");
  let prefix = extract_completion_prefix(line, position.character as usize);

  // 1. Recipe name completions
  if let Some(analysis) = analysis {
    for (namepath, recipe) in &analysis.recipes {
      if should_complete_recipe(&prefix, line, namepath) {
        completions.push(CompletionItem::new_simple(
          namepath.clone(),
          format!("Recipe ({})", recipe.risk),
        ));
      }
    }

    // 2. Variable completions (from just --dump or context)
    for (var_name, var_info) in &analysis.variables {
      if var_name.starts_with(&prefix) {
        completions.push(CompletionItem::new_simple(
          var_name.clone(),
          format!("Variable: {}", var_info.value),
        ));
      }
    }

    // 3. Module/import completions
    for mod_info in analysis.modules.values() {
      if mod_info.name.starts_with(&prefix) {
        completions.push(CompletionItem::new_simple(
          mod_info.name.clone(),
          format!("Module: {}", mod_info.path.display()),
        ));
      }
    }
  }

  // 4. Built-in function completions
  completions.extend(builtin_function_completions(&prefix));

  // 5. Setting completions
  completions.extend(setting_completions(&prefix));

  // 6. Parameter/variable reference completions ({{var}})
  if line.contains("{{") && line.contains("}}") {
    completions.extend(variable_completions(&prefix, analysis));
  }

  // 7. Flag/option completions
  if prefix.starts_with('-') || prefix.starts_with("--") {
    completions.extend(flag_completions(&prefix));
  }

  completions
}

/// Extract the word being completed at the cursor position
fn extract_completion_prefix(line: &str, char_idx: usize) -> String {
  if char_idx >= line.len() {
    return String::new();
  }

  let chars: Vec<char> = line.chars().collect();
  if char_idx >= chars.len() {
    return String::new();
  }

  let mut start = char_idx;

  // Move backward to find start of word
  while start > 0 && is_identifier_char(chars[start - 1]) {
    start -= 1;
  }

  chars[start..char_idx].iter().collect()
}

fn is_identifier_char(c: char) -> bool {
  c.is_alphanumeric() || c == '_' || c == '-' || c == ':' || c == '$' || c == '@'
}

/// Determine if we should offer recipe completion at this position
fn should_complete_recipe(prefix: &str, line: &str, recipe_name: &str) -> bool {
  if !recipe_name.starts_with(prefix) {
    return false;
  }

  let trimmed = line.trim_start();

  // After "just " command
  if let Some(pos) = trimmed.find("just ") {
    let after = &trimmed[pos + 5..];
    if after.trim_start().starts_with(prefix) {
      return true;
    }
  }

  // In dependency list (after colon)
  if trimmed.contains(':') && !trimmed.starts_with('[') {
    let after_colon = trimmed.split(':').nth(1).unwrap_or("").trim();
    if after_colon.starts_with(prefix) {
      return true;
    }
  }

  // At start of line (recipe definition)
  if trimmed.starts_with(prefix)
    && (trimmed.len() == prefix.len() || trimmed[prefix.len()..].starts_with([' ', ':', '\t']))
  {
    return true;
  }

  // Inside {{ recipe }} interpolation (less common)
  false
}

/// Built-in just functions
fn builtin_function_completions(prefix: &str) -> Vec<CompletionItem> {
  let functions = [
    ("arch", "Target architecture"),
    ("env_var", "Get environment variable"),
    (
      "env_var_or_default",
      "Get environment variable with default",
    ),
    ("num_cpus", "Number of CPUs"),
    ("os", "Operating system"),
    ("os_family", "OS family (unix/windows)"),
    ("quote", "Shell-quote a string"),
    ("shell", "Run shell command"),
    ("which", "Find executable in PATH"),
  ];

  functions
    .iter()
    .filter(|(name, _)| name.starts_with(prefix))
    .map(|(name, doc)| CompletionItem::new_simple(name.to_string(), format!("Function: {}", doc)))
    .collect()
}

/// Just settings
fn setting_completions(prefix: &str) -> Vec<CompletionItem> {
  let settings = [
    ("shell", "Set default shell"),
    ("windows_shell", "Set Windows shell"),
    ("script_interpreter", "Set script interpreter"),
    ("default_shell", "Set default shell"),
    ("positional_arguments", "Use positional arguments"),
    ("export", "Export all variables"),
    ("dotenv_load", "Load .env files"),
    ("dotenv_filename", "Custom .env filename"),
    ("dotenv_path", "Custom .env path"),
    ("dotenv_required", "Require .env file"),
    ("allow_duplicate_recipes", "Allow duplicate recipes"),
    ("allow_duplicate_variables", "Allow duplicate variables"),
    ("unstable", "Enable unstable features"),
    ("lists", "Enable list support"),
    ("temporary_directory", "Set temp directory"),
    ("tempdir", "Set temp directory (alias)"),
  ];

  settings
    .iter()
    .filter(|(name, _)| name.starts_with(prefix.trim_start_matches("set ")))
    .map(|(name, doc)| {
      CompletionItem::new_simple(format!("set {}", name), format!("Setting: {}", doc))
    })
    .collect()
}

/// Variable completions for {{ var }} interpolations
fn variable_completions(prefix: &str, analysis: Option<&ProjectAnalysis>) -> Vec<CompletionItem> {
  let mut completions = Vec::new();

  // Built-in variables
  let builtin_vars = [
    ("recipe", "Current recipe name"),
    ("args", "All arguments"),
    ("arg0", "Recipe name (alias)"),
    ("justfile", "Path to justfile"),
    ("justfile_directory", "Directory containing justfile"),
    ("invocation_directory", "Directory where just was invoked"),
    ("invocation_directory_native", "Native invocation directory"),
  ];

  for (name, doc) in builtin_vars {
    if name.starts_with(prefix) {
      completions.push(CompletionItem::new_simple(
        name.to_string(),
        format!("Built-in: {}", doc),
      ));
    }
  }

  // User-defined variables from analysis
  if let Some(analysis) = analysis {
    for (var_name, var_info) in &analysis.variables {
      if var_name.starts_with(prefix) {
        completions.push(CompletionItem::new_simple(
          var_name.clone(),
          format!("Variable: {}", var_info.value),
        ));
      }
    }
  }

  completions
}

/// Flag/option completions
fn flag_completions(prefix: &str) -> Vec<CompletionItem> {
  let flags = [
    ("--list", "List all recipes"),
    ("--summary", "List recipes without docs"),
    ("--show", "Show recipe body"),
    ("--evaluate", "Evaluate variables"),
    ("--edit", "Edit justfile"),
    ("--fmt", "Format justfile"),
    ("--check", "Check formatting"),
    ("--dump", "Dump JSON"),
    ("--dump-format", "Set dump format (json)"),
    ("--init", "Create new justfile"),
    ("--chooser", "Use external chooser"),
    ("--color", "Color output (auto/always/never)"),
    ("--working-directory", "Set working directory"),
    ("--dotenv-filename", "Custom .env filename"),
    ("--dotenv-path", "Custom .env path"),
    ("--unset", "Unset variable"),
    ("--shell", "Set shell"),
    ("--shell-arg", "Set shell argument"),
    ("--tempdir", "Set temp directory"),
    ("-l", "Alias for --list"),
    ("-s", "Alias for --show"),
    ("-e", "Alias for --evaluate"),
    ("-f", "Alias for --fmt"),
    ("-n", "Dry run"),
    ("-q", "Quiet"),
    ("-v", "Verbose"),
    ("--verbose", "Verbose output"),
    ("--version", "Show version"),
    ("--help", "Show help"),
  ];

  flags
    .iter()
    .filter(|(name, _)| name.starts_with(prefix))
    .map(|(name, doc)| CompletionItem::new_simple(name.to_string(), doc.to_string()))
    .collect()
}
