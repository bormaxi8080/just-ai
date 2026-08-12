//! Diagnostics computation for justfiles

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::LazyLock;

use lsp_types::*;

use crate::analysis::ProjectAnalysis;

/// Pre-compiled regex for variable interpolation detection
static VAR_INTERPOLATION_REGEX: LazyLock<regex::Regex> =
  LazyLock::new(|| regex::Regex::new(r"\{\{\s*([a-zA-Z_][a-zA-Z0-9_]*)\s*\}\}").unwrap());

/// Compute diagnostics for a justfile
pub fn compute_diagnostics(uri: &Url, text: &str, analysis: &ProjectAnalysis) -> Vec<Diagnostic> {
  let mut diagnostics = Vec::new();

  // 1. Syntax diagnostics from just parse errors
  diagnostics.extend(syntax_diagnostics(uri, text));

  // 2. Semantic diagnostics from project analysis
  diagnostics.extend(semantic_diagnostics(uri, text, analysis));

  // 3. Risk diagnostics (from just-ai risk analysis)
  diagnostics.extend(risk_diagnostics(uri, analysis));

  diagnostics
}

/// Syntax diagnostics - basic parsing checks
fn syntax_diagnostics(_uri: &Url, text: &str) -> Vec<Diagnostic> {
  let mut diagnostics = Vec::new();
  let lines: Vec<&str> = text.lines().collect();

  for (line_idx, line) in lines.iter().enumerate() {
    let trimmed = line.trim_start();

    // Check for common syntax issues
    if trimmed.starts_with('#') || trimmed.is_empty() {
      continue;
    }

    // Check for recipe definitions without proper syntax
    if trimmed.contains(':') && !trimmed.starts_with('[') && !trimmed.contains(" := ") {
      // This looks like a recipe definition
      let parts: Vec<&str> = trimmed.split(':').collect();
      if parts.len() == 2 {
        let name_part = parts[0].trim();
        // Check for invalid characters in recipe name
        if name_part.contains(' ') && !name_part.starts_with('[') {
          diagnostics.push(Diagnostic::new(
            Range::new(
              Position::new(line_idx as u32, 0),
              Position::new(line_idx as u32, line.len() as u32),
            ),
            Some(DiagnosticSeverity::WARNING),
            Some(NumberOrString::String("invalid-recipe-name".to_string())),
            Some("just-ai".to_string()),
            format!(
              "Recipe name '{}' contains spaces. Use underscores or hyphens.",
              name_part
            ),
            None,
            None,
          ));
        }
      }
    }

    // Check for unclosed interpolations
    let open_braces = line.matches("{{").count();
    let close_braces = line.matches("}}").count();
    if open_braces != close_braces {
      diagnostics.push(Diagnostic::new(
        Range::new(
          Position::new(line_idx as u32, 0),
          Position::new(line_idx as u32, line.len() as u32),
        ),
        Some(DiagnosticSeverity::ERROR),
        Some(NumberOrString::String("unclosed-interpolation".to_string())),
        Some("just-ai".to_string()),
        "Unclosed interpolation {{ }}".to_string(),
        None,
        None,
      ));
    }

    // Check for bare '@' not at start of line (might be typo)
    if line.contains('@') && !line.trim_start().starts_with('@') && !line.contains("email") {
      // Could be a typo for recipe reference
      // Not flagging as error since it could be intentional
    }
  }

  diagnostics
}

/// Semantic diagnostics - cross-references, undefined symbols, circular deps
fn semantic_diagnostics(uri: &Url, text: &str, analysis: &ProjectAnalysis) -> Vec<Diagnostic> {
  let mut diagnostics = Vec::new();
  let lines: Vec<&str> = text.lines().collect();

  // Build set of all defined recipe names
  let defined_recipes: HashSet<_> = analysis.recipes.keys().cloned().collect();

  // Check each line for recipe references
  for (line_idx, line) in lines.iter().enumerate() {
    let trimmed = line.trim_start();

    // Check dependencies in recipe definitions
    if let Some(colon_pos) = trimmed.find(':') {
      let before_colon = &trimmed[..colon_pos];
      let after_colon = &trimmed[colon_pos + 1..].trim();

      // Recipe name might have parameters before colon
      let _recipe_name = before_colon.split_whitespace().next().unwrap_or("");

      // Check dependencies (after colon, space-separated)
      if !after_colon.is_empty() {
        for dep in after_colon.split_whitespace() {
          // Skip parameters (they contain = or : or are in {{ }})
          if dep.contains('=') || dep.contains(':') || dep.contains("{{") {
            continue;
          }

          // Check if dependency exists
          if !defined_recipes.contains(dep) && !is_builtin_recipe(dep) {
            diagnostics.push(Diagnostic::new(
              Range::new(
                Position::new(
                  line_idx as u32,
                  (colon_pos + 1 + after_colon.find(dep).unwrap_or(0)) as u32,
                ),
                Position::new(
                  line_idx as u32,
                  (colon_pos + 1 + after_colon.find(dep).unwrap_or(0) + dep.len()) as u32,
                ),
              ),
              Some(DiagnosticSeverity::ERROR),
              Some(NumberOrString::String("undefined-recipe".to_string())),
              Some("just-ai".to_string()),
              format!("Undefined recipe '{}'", dep),
              None,
              None,
            ));
          }
        }
      }
    }

    // Check for recipe calls in command bodies (just <recipe>)
    if let Some(just_pos) = line.find("just ") {
      let after_just = &line[just_pos + 5..];
      let words: Vec<&str> = after_just.split_whitespace().collect();
      for word in words {
        if word.starts_with('-') || word.starts_with('@') || word.starts_with('[') {
          continue; // Skip flags and attributes
        }
        if defined_recipes.contains(word) || is_builtin_recipe(word) {
          continue;
        }
        // Might be a recipe call
        let char_offset = just_pos + 5 + after_just.find(word).unwrap_or(0);
        diagnostics.push(Diagnostic::new(
          Range::new(
            Position::new(line_idx as u32, char_offset as u32),
            Position::new(line_idx as u32, (char_offset + word.len()) as u32),
          ),
          Some(DiagnosticSeverity::WARNING),
          Some(NumberOrString::String(
            "potentially-undefined-recipe".to_string(),
          )),
          Some("just-ai".to_string()),
          format!("Potentially undefined recipe '{}'", word),
          None,
          None,
        ));
      }
    }

    // Check variable interpolations {{ var }}
    for mat in VAR_INTERPOLATION_REGEX.find_iter(line) {
      let _var_name = &line[mat.start() + 2..mat.end() - 2].trim();
      // Check if variable is defined (we'd need variable analysis for this)
      // For now, skip
    }
  }

  // Check for circular dependencies
  diagnostics.extend(circular_dependency_diagnostics(uri, analysis));

  diagnostics
}

/// Check for circular dependencies in the recipe graph
fn circular_dependency_diagnostics(_uri: &Url, analysis: &ProjectAnalysis) -> Vec<Diagnostic> {
  let mut diagnostics = Vec::new();

  // Simple DFS for each recipe to find cycles
  for (namepath, recipe) in &analysis.recipes {
    let mut visited = HashSet::new();
    let mut rec_stack = HashSet::new();
    if has_cycle(
      namepath,
      &analysis.recipes,
      &mut visited,
      &mut rec_stack,
      &mut Vec::new(),
    ) {
      diagnostics.push(Diagnostic::new(
        Range::new(
          Position::new(recipe.line, 0),
          Position::new(recipe.end_line, 0),
        ),
        Some(DiagnosticSeverity::ERROR),
        Some(NumberOrString::String("circular-dependency".to_string())),
        Some("just-ai".to_string()),
        format!(
          "Circular dependency detected involving recipe '{}'",
          namepath
        ),
        None,
        None,
      ));
    }
  }

  diagnostics
}

fn has_cycle(
  current: &str,
  recipes: &std::collections::HashMap<String, crate::analysis::RecipeInfo>,
  visited: &mut HashSet<String>,
  rec_stack: &mut HashSet<String>,
  path: &mut Vec<String>,
) -> bool {
  visited.insert(current.to_string());
  rec_stack.insert(current.to_string());
  path.push(current.to_string());

  if let Some(recipe) = recipes.get(current) {
    for dep in &recipe.dependencies {
      if !visited.contains(dep) {
        if has_cycle(dep, recipes, visited, rec_stack, path) {
          return true;
        }
      } else if rec_stack.contains(dep) {
        // Found a cycle
        return true;
      }
    }
  }

  rec_stack.remove(current);
  path.pop();
  false
}

/// Risk diagnostics from just-ai risk analysis
fn risk_diagnostics(_uri: &Url, analysis: &ProjectAnalysis) -> Vec<Diagnostic> {
  let mut diagnostics = Vec::new();

  for recipe in analysis.recipes.values() {
    let severity = match recipe.risk.as_str() {
      "blocked" => DiagnosticSeverity::ERROR,
      "high" => DiagnosticSeverity::WARNING,
      "medium" => DiagnosticSeverity::INFORMATION,
      "low" => DiagnosticSeverity::HINT,
      _ => DiagnosticSeverity::HINT,
    };

    for finding in &recipe.risks {
      // Try to find the line in the source
      let line_idx = find_finding_line(&recipe.file, &finding.line, &analysis.file_contents);
      if let Some(line) = line_idx {
        let finding_severity = match finding.level.as_str() {
          "blocked" => DiagnosticSeverity::ERROR,
          "high" => DiagnosticSeverity::WARNING,
          "medium" => DiagnosticSeverity::INFORMATION,
          "low" => DiagnosticSeverity::HINT,
          _ => severity,
        };

        diagnostics.push(Diagnostic::new(
          Range::new(
            Position::new(line, 0),
            Position::new(line, 1000), // Full line
          ),
          Some(finding_severity),
          Some(NumberOrString::String(format!("just-ai-{}", finding.level))),
          Some("just-ai".to_string()),
          format!("[just-ai] {}: `{}`", finding.reason, finding.line.trim()),
          None,
          None,
        ));
      }
    }
  }

  diagnostics
}

/// Find the line number of a risk finding in the source file
fn find_finding_line(
  file: &Path,
  search_line: &str,
  file_contents: &std::collections::HashMap<PathBuf, String>,
) -> Option<u32> {
  let content = file_contents.get(file)?;
  let normalized_search = search_line.trim().to_lowercase();

  for (i, line) in content.lines().enumerate() {
    let line_trimmed = line.trim().to_lowercase();
    if line_trimmed.contains(&normalized_search) || normalized_search.contains(&line_trimmed) {
      return Some(i as u32);
    }
  }
  None
}

/// Check if a recipe name is a built-in just command
fn is_builtin_recipe(name: &str) -> bool {
  matches!(
    name,
    "init"
      | "list"
      | "summary"
      | "show"
      | "evaluate"
      | "edit"
      | "fmt"
      | "check"
      | "completions"
      | "dump"
      | "doctor"
      | "help"
      | "version"
  )
}
