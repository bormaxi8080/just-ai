//! Shared recipe merge planning used by presentation adapters.

use crate::{ContextParameter, ContextRecipe};

/// Try to merge two similar recipes intelligently.
/// Combines parameters, uses the more complete doc, merges dependencies,
/// and for body lines, keeps unique lines from both.
pub fn smart_merge_recipes(a: &ContextRecipe, b: &ContextRecipe) -> String {
  // Use the shorter name (more generic)
  let name = if a.name.len() <= b.name.len() {
    &a.name
  } else {
    &b.name
  };

  // Use the doc from the recipe that has one (prefer longer)
  let doc = if a.doc.as_deref().map(|d| d.len()).unwrap_or(0)
    >= b.doc.as_deref().map(|d| d.len()).unwrap_or(0)
  {
    a.doc.clone()
  } else {
    b.doc.clone()
  };

  // Merge parameters (union by name, prefer one with default)
  let mut param_map: std::collections::HashMap<String, ContextParameter> =
    std::collections::HashMap::new();
  for p in &a.parameters {
    param_map.insert(p.name.clone(), p.clone());
  }
  for p in &b.parameters {
    param_map
      .entry(p.name.clone())
      .and_modify(|existing| {
        if existing.default.is_none() && p.default.is_some() {
          *existing = p.clone();
        }
      })
      .or_insert_with(|| p.clone());
  }
  let mut parameters: Vec<ContextParameter> = param_map.into_values().collect();
  parameters.sort_by(|a, b| a.name.cmp(&b.name));

  // Merge dependencies (union)
  let mut deps: std::collections::HashSet<String> = a.dependencies.iter().cloned().collect();
  deps.extend(b.dependencies.iter().cloned());
  let mut dependencies: Vec<String> = deps.into_iter().collect();
  dependencies.sort();

  // Smart merge body lines - keep unique lines from both
  let mut body_lines: Vec<String> = Vec::new();
  let mut seen = std::collections::HashSet::new();

  for line in &a.body {
    let trimmed = line.trim();
    if !trimmed.is_empty() && seen.insert(trimmed.to_string()) {
      body_lines.push(line.clone());
    }
  }
  for line in &b.body {
    let trimmed = line.trim();
    if !trimmed.is_empty() && seen.insert(trimmed.to_string()) {
      body_lines.push(line.clone());
    }
  }

  // Render the merged recipe
  let mut rendered = String::new();
  if let Some(doc) = doc {
    rendered.push_str("# ");
    rendered.push_str(doc.trim());
    rendered.push('\n');
  }

  rendered.push_str(name);
  for param in &parameters {
    rendered.push(' ');
    rendered.push_str(&param.name);
    if let Some(default) = &param.default {
      rendered.push_str("='");
      rendered.push_str(&default.replace('\'', "\\'"));
      rendered.push('\'');
    }
  }

  if !dependencies.is_empty() {
    rendered.push_str(": ");
    rendered.push_str(
      &dependencies
        .iter()
        .map(|d| format!("({d})"))
        .collect::<Vec<_>>()
        .join(" "),
    );
  } else {
    rendered.push(':');
  }
  rendered.push('\n');

  for line in body_lines {
    rendered.push_str("  ");
    rendered.push_str(&line);
    rendered.push('\n');
  }

  rendered
}
