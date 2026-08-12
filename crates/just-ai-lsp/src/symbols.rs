use crate::analysis::ProjectAnalysis;
use lsp_types::{DocumentSymbol, Position, Range, SymbolKind, SymbolTag};

/// Provide document symbols (recipes, variables, imports, modules)
pub fn provide_document_symbols(
  text: &str,
  analysis: Option<&ProjectAnalysis>,
) -> Vec<DocumentSymbol> {
  let mut symbols = Vec::new();

  if let Some(analysis) = analysis {
    // Add recipes as symbols
    for (namepath, recipe) in &analysis.recipes {
      symbols.push(DocumentSymbol {
        name: namepath.clone(),
        detail: Some(format!("Recipe ({})", recipe.risk)),
        kind: SymbolKind::FUNCTION,
        tags: if recipe.private {
          Some(vec![SymbolTag::DEPRECATED])
        } else {
          None
        },
        #[allow(deprecated)]
        deprecated: None,
        range: Range::new(
          Position::new(recipe.line, 0),
          Position::new(recipe.end_line, 0),
        ),
        selection_range: Range::new(
          Position::new(recipe.line, 0),
          Position::new(recipe.line, recipe.name.len() as u32),
        ),
        children: Some(recipe_children(recipe)),
      });
    }

    // Add variables as symbols
    for (name, var_info) in &analysis.variables {
      symbols.push(DocumentSymbol {
        name: name.clone(),
        detail: Some(format!("Variable: {}", var_info.value)),
        kind: SymbolKind::VARIABLE,
        tags: None,
        #[allow(deprecated)]
        deprecated: None,
        range: Range::new(
          Position::new(var_info.line, 0),
          Position::new(var_info.line, 100),
        ),
        selection_range: Range::new(
          Position::new(var_info.line, 0),
          Position::new(var_info.line, name.len() as u32),
        ),
        children: None,
      });
    }

    // Add modules as symbols
    for (name, mod_info) in &analysis.modules {
      symbols.push(DocumentSymbol {
        name: name.clone(),
        detail: Some(format!("Module: {}", mod_info.path.display())),
        kind: SymbolKind::MODULE,
        tags: None,
        #[allow(deprecated)]
        deprecated: None,
        range: Range::new(Position::new(0, 0), Position::new(0, name.len() as u32)),
        selection_range: Range::new(Position::new(0, 0), Position::new(0, name.len() as u32)),
        children: None,
      });
    }
  } else {
    // Fallback: parse text directly for basic symbols
    symbols.extend(parse_fallback_symbols(text));
  }

  symbols
}

/// Create child symbols for a recipe (parameters, dependencies)
fn recipe_children(recipe: &crate::analysis::RecipeInfo) -> Vec<DocumentSymbol> {
  let mut children = Vec::new();

  // Parameters
  for param in &recipe.parameters {
    children.push(DocumentSymbol {
      name: param.name.clone(),
      detail: Some(format!(
        "Parameter ({})",
        if param.is_variadic {
          "variadic"
        } else {
          "singular"
        }
      )),
      kind: SymbolKind::VARIABLE,
      tags: None,
      #[allow(deprecated)]
      deprecated: None,
      range: Range::new(
        Position::new(recipe.line, 0),
        Position::new(recipe.line, param.name.len() as u32),
      ),
      selection_range: Range::new(
        Position::new(recipe.line, 0),
        Position::new(recipe.line, param.name.len() as u32),
      ),
      children: None,
    });
  }

  // Dependencies
  for dep in &recipe.dependencies {
    children.push(DocumentSymbol {
      name: dep.clone(),
      detail: Some("Dependency".to_string()),
      kind: SymbolKind::METHOD,
      tags: None,
      #[allow(deprecated)]
      deprecated: None,
      range: Range::new(
        Position::new(recipe.line, 0),
        Position::new(recipe.line, dep.len() as u32),
      ),
      selection_range: Range::new(
        Position::new(recipe.line, 0),
        Position::new(recipe.line, dep.len() as u32),
      ),
      children: None,
    });
  }

  children
}

/// Fallback symbol parsing when analysis is not available
fn parse_fallback_symbols(text: &str) -> Vec<DocumentSymbol> {
  let mut symbols = Vec::new();
  let lines: Vec<&str> = text.lines().collect();

  for (i, line) in lines.iter().enumerate() {
    let trimmed = line.trim_start();

    // Recipe definition
    if !trimmed.is_empty() && !trimmed.starts_with('#') && !trimmed.starts_with('[') {
      if trimmed.contains(':') && !trimmed.contains(" := ") {
        let parts: Vec<&str> = trimmed.split(':').collect();
        if parts.len() == 2 {
          let name_part = parts[0].trim();
          if !name_part.contains(' ') || name_part.starts_with('[') {
            // Valid recipe name
            symbols.push(DocumentSymbol {
              name: name_part.to_string(),
              detail: Some("Recipe".to_string()),
              kind: SymbolKind::FUNCTION,
              tags: None,
              #[allow(deprecated)]
              deprecated: None,
              range: Range::new(
                Position::new(i as u32, 0),
                Position::new(i as u32, line.len() as u32),
              ),
              selection_range: Range::new(
                Position::new(i as u32, 0),
                Position::new(i as u32, name_part.len() as u32),
              ),
              children: None,
            });
          }
        }
      }

      // Variable assignment
      if trimmed.contains(" := ") || trimmed.contains(" = ") {
        let parts: Vec<&str> = trimmed.split_whitespace().collect();
        if parts.len() >= 3 && (parts[1] == ":=" || parts[1] == "=") {
          let var_name = parts[0].trim_end_matches(':');
          symbols.push(DocumentSymbol {
            name: var_name.to_string(),
            detail: Some("Variable".to_string()),
            kind: SymbolKind::VARIABLE,
            tags: None,
            #[allow(deprecated)]
            deprecated: None,
            range: Range::new(
              Position::new(i as u32, 0),
              Position::new(i as u32, line.len() as u32),
            ),
            selection_range: Range::new(
              Position::new(i as u32, 0),
              Position::new(i as u32, var_name.len() as u32),
            ),
            children: None,
          });
        }
      }

      // Import/module
      if trimmed.starts_with("mod ") || trimmed.starts_with("import ") {
        let parts: Vec<&str> = trimmed.split_whitespace().collect();
        if parts.len() >= 2 {
          let mod_name = parts[1];
          symbols.push(DocumentSymbol {
            name: mod_name.to_string(),
            detail: Some("Module".to_string()),
            kind: SymbolKind::MODULE,
            tags: None,
            #[allow(deprecated)]
            deprecated: None,
            range: Range::new(
              Position::new(i as u32, 0),
              Position::new(i as u32, line.len() as u32),
            ),
            selection_range: Range::new(
              Position::new(i as u32, 0),
              Position::new(i as u32, mod_name.len() as u32),
            ),
            children: None,
          });
        }
      }
    }
  }

  symbols
}
