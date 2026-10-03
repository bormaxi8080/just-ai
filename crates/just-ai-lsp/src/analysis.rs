// Project analysis for justfiles
//
// This module parses justfiles using the upstream `just` crate's JSON dump
// and provides semantic analysis for LSP features.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use anyhow::Result;
use just_ai::inspection::inspect_project_at;
use just_ai::inspection::{ContextRecipe, ProjectContext};
use lsp_types::{Location, Position, Range, Url};
use serde::{Deserialize, Serialize};

/// Complete project analysis for LSP features
#[derive(Debug, Clone)]
#[allow(dead_code)] // context field kept for potential future use
pub struct ProjectAnalysis {
  /// Project context from just --dump
  pub context: ProjectContext,
  /// Recipe name -> recipe info
  pub recipes: HashMap<String, RecipeInfo>,
  /// Variable name -> variable info
  pub variables: HashMap<String, VariableInfo>,
  /// Import/module name -> module info
  pub modules: HashMap<String, ModuleInfo>,
  /// File path -> file content (for source locations)
  pub file_contents: HashMap<PathBuf, String>,
}

/// Information about a recipe
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecipeInfo {
  pub name: String,
  pub namepath: String,
  pub line: u32,
  pub end_line: u32,
  pub doc: Option<String>,
  pub parameters: Vec<ParameterInfo>,
  pub dependencies: Vec<String>,
  pub body: Vec<String>,
  pub risk: String,
  pub risks: Vec<RiskFinding>,
  pub private: bool,
  pub file: PathBuf,
}

/// Information about a recipe parameter
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParameterInfo {
  pub name: String,
  pub kind: String, // "singular", "plus", "star"
  pub default: Option<String>,
  pub is_variadic: bool,
}

/// Information about a variable
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VariableInfo {
  pub name: String,
  pub line: u32,
  pub value: String,
  pub exported: bool,
  pub file: PathBuf,
}

/// Information about a module/import
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModuleInfo {
  pub name: String,
  pub path: PathBuf,
  pub alias: Option<String>,
}

/// Risk finding from local analysis
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RiskFinding {
  pub level: String,
  pub line: String,
  pub reason: String,
}

/// Analyze a project by running just --dump
pub async fn analyze_project(project_root: &Path) -> Result<ProjectAnalysis> {
  let project_root = project_root.to_owned();
  let just_binary = std::env::var("JUST_AI_JUST_BINARY")
    .or_else(|_| std::env::var("JUST_BINARY"))
    .unwrap_or_else(|_| "just".to_owned());
  let context = tokio::task::spawn_blocking(move || {
    inspect_project_at(&just_binary, &project_root)
      .map_err(|error| anyhow::anyhow!(error.to_string()))
  })
  .await??;
  analyze_context(context)
}

fn analyze_context(context: ProjectContext) -> Result<ProjectAnalysis> {
  let mut recipes = HashMap::new();
  let variables = HashMap::new();
  let modules = HashMap::new();
  let mut file_contents = HashMap::new();

  // Load file contents for source location mapping
  for module in &context.modules {
    if let Ok(content) = std::fs::read_to_string(&module.source) {
      file_contents.insert(module.source.clone(), content);
    }
  }

  // Parse recipes
  for recipe in &context.recipes {
    let info = parse_recipe(&context, recipe, &file_contents)?;
    recipes.insert(recipe.namepath.clone(), info);
  }

  // Note: just --dump doesn't currently include variables and imports in a structured way
  // We would need to parse the justfile directly for those
  // For now, we rely on the context.recipes and context.modules

  Ok(ProjectAnalysis {
    context,
    recipes,
    variables,
    modules,
    file_contents,
  })
}

/// Parse a recipe into RecipeInfo with source locations
fn parse_recipe(
  context: &ProjectContext,
  recipe: &ContextRecipe,
  file_contents: &HashMap<PathBuf, String>,
) -> Result<RecipeInfo> {
  // Find the source file for this recipe
  let source_file = context
    .modules
    .iter()
    .find(|module| module.module_path == recipe.module_path)
    .map(|m| m.source.clone())
    .unwrap_or_default();

  // Find line numbers by searching in the source file
  let (line, end_line) =
    find_recipe_lines(&source_file, &recipe.name, file_contents).unwrap_or((0, 0));

  let parameters = recipe
    .parameters
    .iter()
    .map(|p| ParameterInfo {
      name: p.name.clone(),
      kind: p.kind.clone(),
      default: p.default.clone(),
      is_variadic: p.kind == "plus" || p.kind == "star",
    })
    .collect();

  let risks = recipe
    .risks
    .iter()
    .map(|r| RiskFinding {
      level: format!("{:?}", r.level).to_lowercase(),
      line: r.line.clone(),
      reason: r.reason.clone(),
    })
    .collect();

  Ok(RecipeInfo {
    name: recipe.name.clone(),
    namepath: recipe.namepath.clone(),
    line,
    end_line,
    doc: recipe.doc.clone(),
    parameters,
    dependencies: recipe.dependencies.clone(),
    body: recipe.body.clone(),
    risk: format!("{:?}", recipe.risk).to_lowercase(),
    risks,
    private: recipe.private,
    file: source_file,
  })
}

/// Find recipe line numbers in source file
fn find_recipe_lines(
  source_file: &Path,
  recipe_name: &str,
  file_contents: &HashMap<PathBuf, String>,
) -> Option<(u32, u32)> {
  let content = file_contents.get(source_file)?;
  let lines: Vec<&str> = content.lines().collect();

  let mut start_line = None;
  let mut end_line = None;

  for (i, line) in lines.iter().enumerate() {
    let trimmed = line.trim_start();
    // Match recipe definition: "name:", "name params:", "name params"
    if trimmed.starts_with(&format!("{} ", recipe_name))
      || trimmed.starts_with(&format!("{}:", recipe_name))
      || trimmed == recipe_name
    {
      start_line = Some(i as u32);
      // Find end of recipe (next non-indented non-empty line)
      for (j, next_line) in lines.iter().enumerate().skip(i + 1) {
        let next_trimmed = next_line.trim_start();
        if !next_trimmed.is_empty() && !next_line.starts_with(' ') && !next_line.starts_with('\t') {
          end_line = Some(j as u32);
          break;
        }
      }
      if end_line.is_none() {
        end_line = Some(lines.len() as u32);
      }
      break;
    }
  }

  start_line.zip(end_line)
}

/// Find definition location for a symbol at a position
pub fn find_definition(
  text: &str,
  position: lsp_types::Position,
  analysis: Option<&ProjectAnalysis>,
  uri: &lsp_types::Url,
) -> Option<Vec<lsp_types::Location>> {
  let line_text = text.lines().nth(position.line as usize)?;
  let char_idx = crate::utf16_character_index(line_text, position.character);

  // Find word at position
  let word = extract_word_at(line_text, char_idx)?;

  // Search in recipes
  if let Some(analysis) = analysis {
    // Check recipe names
    for (namepath, recipe) in &analysis.recipes {
      if namepath.contains(&word) || recipe.name == word {
        return Some(vec![Location::new(
          uri.clone(),
          Range::new(
            Position::new(recipe.line, 0),
            Position::new(recipe.end_line, 0),
          ),
        )]);
      }
    }

    // Check dependencies (recipe references)
    for recipe in analysis.recipes.values() {
      for dep in &recipe.dependencies {
        if dep == &word {
          if let Some(dep_recipe) = analysis.recipes.get(dep) {
            return Some(vec![Location::new(
              uri.clone(),
              Range::new(
                Position::new(dep_recipe.line, 0),
                Position::new(dep_recipe.end_line, 0),
              ),
            )]);
          }
        }
      }
    }
  }

  None
}

/// Provide code actions for diagnostics
pub fn provide_code_actions(
  text: &str,
  _range: lsp_types::Range,
  diagnostics: &[lsp_types::Diagnostic],
  _analysis: Option<&ProjectAnalysis>,
  uri: &Url,
) -> Vec<lsp_types::CodeActionOrCommand> {
  let mut actions = Vec::new();

  for diagnostic in diagnostics {
    // Action: Add missing recipe
    if diagnostic.message.contains("Undefined recipe") {
      if let Some(recipe_name) = extract_recipe_name_from_diagnostic(&diagnostic.message) {
        actions.push(lsp_types::CodeActionOrCommand::CodeAction(
          lsp_types::CodeAction {
            title: format!("Add recipe '{}'", recipe_name),
            kind: Some(lsp_types::CodeActionKind::QUICKFIX),
            diagnostics: Some(vec![diagnostic.clone()]),
            edit: Some(lsp_types::WorkspaceEdit {
              changes: Some({
                let mut map = HashMap::new();
                map.insert(
                  uri.clone(),
                  vec![lsp_types::TextEdit {
                    range: Range::new(
                      Position::new(text.lines().count() as u32, 0),
                      Position::new(text.lines().count() as u32, 0),
                    ),
                    new_text: format!("\n{}:\n  # implement {}\n", recipe_name, recipe_name),
                  }],
                );
                map
              }),
              document_changes: None,
              change_annotations: None,
            }),
            ..Default::default()
          },
        ));
      }
    }

    // Action: Fix circular dependency
    if diagnostic.message.contains("circular") || diagnostic.message.contains("Circular") {
      actions.push(lsp_types::CodeActionOrCommand::CodeAction(
        lsp_types::CodeAction {
          title: "Remove circular dependency".to_string(),
          kind: Some(lsp_types::CodeActionKind::REFACTOR),
          diagnostics: Some(vec![diagnostic.clone()]),
          ..Default::default()
        },
      ));
    }
  }

  actions
}

/// Extract word at character position in a line
fn extract_word_at(line: &str, char_idx: usize) -> Option<String> {
  if char_idx >= line.len() {
    return None;
  }

  let chars: Vec<char> = line.chars().collect();
  if char_idx >= chars.len() {
    return None;
  }

  let mut start = char_idx;
  let mut end = char_idx;

  // Expand backward
  while start > 0 && is_identifier_char(chars[start - 1]) {
    start -= 1;
  }

  // Expand forward
  while end < chars.len() && is_identifier_char(chars[end]) {
    end += 1;
  }

  if start < end {
    Some(chars[start..end].iter().collect())
  } else {
    None
  }
}

fn is_identifier_char(c: char) -> bool {
  c.is_alphanumeric() || c == '_' || c == '-' || c == ':' || c == '$'
}

/// Extract recipe name from diagnostic message
fn extract_recipe_name_from_diagnostic(message: &str) -> Option<String> {
  // Look for patterns like "Undefined recipe 'foo'" or "recipe 'foo' not found"
  if let Some(start) = message.find('\'') {
    if let Some(end) = message[start + 1..].find('\'') {
      return Some(message[start + 1..start + 1 + end].to_string());
    }
  }
  None
}

#[cfg(test)]
mod tests {
  use super::*;

  #[tokio::test]
  async fn loads_core_context_and_refreshes_saved_recipes() {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path().join("justfile");
    std::fs::write(&root, "test:\n  echo a\n").unwrap();
    let first = analyze_project(directory.path()).await.unwrap();
    assert!(first.recipes.contains_key("test"));
    std::fs::write(&root, "build:\n  echo b\n").unwrap();
    let next = analyze_project(directory.path()).await.unwrap();
    assert!(next.recipes.contains_key("build"));
    assert!(!next.recipes.contains_key("test"));
  }

  #[tokio::test]
  async fn maps_module_recipe_to_its_own_source() {
    let directory = tempfile::tempdir().unwrap();
    std::fs::write(
      directory.path().join("justfile"),
      "mod child\n\nroot:\n  echo root\n",
    )
    .unwrap();
    std::fs::write(
      directory.path().join("child.just"),
      "child-recipe:\n  echo child\n",
    )
    .unwrap();
    let analysis = analyze_project(directory.path()).await.unwrap();
    let recipe = analysis
      .recipes
      .get("child::child-recipe")
      .or_else(|| {
        analysis
          .recipes
          .values()
          .find(|recipe| recipe.name == "child-recipe")
      })
      .unwrap();
    assert_eq!(recipe.file.file_name().unwrap(), "child.just");
    assert_eq!(recipe.line, 0);
  }
}
