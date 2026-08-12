use {
  crate::{
    application,
    domain::risk::{RiskFinding, RiskLevel},
    just_dump,
  },
  serde::{Deserialize, Serialize},
  serde_json::Value,
  similar,
  std::{
    collections::{BTreeMap, HashMap, HashSet},
    error::Error,
    path::{Path, PathBuf},
  },
};

fn load_dump(just_binary: &Path) -> Result<DumpModule, Box<dyn Error>> {
  load_dump_at(just_binary, None)
}

pub(crate) fn load_context(just_binary: &Path) -> Result<ProjectContext, Box<dyn Error>> {
  Ok(ProjectContext::from_dump(load_dump(just_binary)?))
}

fn load_dump_at(
  just_binary: &Path,
  project_root: Option<&Path>,
) -> Result<DumpModule, Box<dyn Error>> {
  Ok(serde_json::from_value(just_dump::load_at(
    just_binary,
    project_root,
  )?)?)
}

/// Inspect the project discovered by `just` and return a stable, serializable
/// representation suitable for CLI, desktop, and agent adapters.
pub fn inspect_project(just_binary: impl Into<PathBuf>) -> Result<ProjectContext, Box<dyn Error>> {
  let just_binary = just_binary.into();
  Ok(ProjectContext::from_dump(load_dump(&just_binary)?))
}

/// Inspect a specific project without changing process-global working state.
pub fn inspect_project_at(
  just_binary: impl AsRef<Path>,
  project_root: impl AsRef<Path>,
) -> Result<ProjectContext, Box<dyn Error>> {
  let project_root = project_root.as_ref();
  if !project_root.is_dir() {
    return Err(
      format!(
        "project root is not a directory: {}",
        project_root.display()
      )
      .into(),
    );
  }
  Ok(ProjectContext::from_dump(load_dump_at(
    just_binary.as_ref(),
    Some(project_root),
  )?))
}

#[derive(Debug, Deserialize)]
struct DumpModule {
  #[serde(default)]
  modules: BTreeMap<String, DumpModule>,
  #[serde(default)]
  recipes: BTreeMap<String, DumpRecipe>,
  source: PathBuf,
  #[serde(default)]
  warnings: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct DumpRecipe {
  #[serde(default)]
  body: Vec<Value>,
  #[serde(default)]
  dependencies: Vec<DumpDependency>,
  doc: Option<String>,
  name: String,
  namepath: String,
  #[serde(default)]
  parameters: Vec<DumpParameter>,
  #[serde(default)]
  private: bool,
  #[serde(default)]
  quiet: bool,
  #[serde(default)]
  shebang: bool,
}

#[derive(Debug, Deserialize)]
struct DumpDependency {
  recipe: String,
}

#[derive(Debug, Deserialize)]
struct DumpParameter {
  name: String,
  kind: String,
  #[serde(default)]
  default: Option<Value>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct ProjectContext {
  #[serde(default)]
  pub facts: application::project_context::ProjectFacts,
  pub modules: Vec<ContextModule>,
  pub recipes: Vec<ContextRecipe>,
  pub warnings: Vec<String>,
}

impl ProjectContext {
  fn from_dump(dump: DumpModule) -> Self {
    let facts = dump.source.parent().map_or_else(Default::default, |root| {
      application::project_context::ProjectScanner::default().scan(root)
    });
    let mut context = Self {
      facts,
      modules: Vec::new(),
      recipes: Vec::new(),
      warnings: Vec::new(),
    };

    context.collect_module("", dump);
    context
  }

  fn collect_module(&mut self, module_path: &str, module: DumpModule) {
    self.modules.push(ContextModule {
      module_path: module_path.to_owned(),
      source: module.source,
      recipe_count: module.recipes.len(),
    });

    self.warnings.extend(module.warnings);

    for recipe in module.recipes.into_values() {
      let body = recipe.body.iter().map(render_body_line).collect::<Vec<_>>();
      let risks = RiskFinding::scan_lines(&body);
      let risk = RiskLevel::highest(&risks);

      self.recipes.push(ContextRecipe {
        body,
        dependencies: recipe
          .dependencies
          .into_iter()
          .map(|dependency| dependency.recipe)
          .collect(),
        doc: recipe.doc,
        module_path: module_path.to_owned(),
        name: recipe.name,
        namepath: recipe.namepath,
        parameters: recipe
          .parameters
          .into_iter()
          .map(|parameter| ContextParameter {
            default: parameter.default.map(|default| render_value(&default)),
            kind: parameter.kind,
            name: parameter.name,
          })
          .collect(),
        private: recipe.private,
        quiet: recipe.quiet,
        risk,
        risks,
        shebang: recipe.shebang,
      });
    }

    for (name, child) in module.modules {
      let child_path = if module_path.is_empty() {
        name
      } else {
        format!("{module_path}:{name}")
      };
      self.collect_module(&child_path, child);
    }
  }

  pub fn find_recipe(&self, needle: &str) -> Option<&ContextRecipe> {
    self
      .recipes
      .iter()
      .find(|recipe| recipe.namepath == needle || recipe.name == needle)
  }

  pub fn has_recipe(&self, name: &str) -> bool {
    self
      .recipes
      .iter()
      .any(|recipe| recipe.name == name || recipe.namepath == name)
  }

  pub fn root_source(&self) -> Option<&Path> {
    self.modules.first().map(|module| module.source.as_path())
  }

  /// Find the best line number to insert a new recipe based on dependencies and naming similarity.
  /// Returns the line number (0-indexed) after which to insert, or None to append at end.
  pub fn find_insertion_point(&self, recipe_name: &str, dependencies: &[String]) -> Option<usize> {
    // First priority: insert after a dependency recipe
    for dep in dependencies {
      if let Some(dep_recipe) = self.find_recipe(dep) {
        // Find the line number of the dependency recipe in the source file
        if let Some(source) = &self.modules.first().map(|m| m.source.clone())
          && let Ok(content) = std::fs::read_to_string(source)
        {
          let lines: Vec<&str> = content.lines().collect();
          for (i, line) in lines.iter().enumerate() {
            let trimmed = line.trim_start();
            if trimmed.starts_with(&format!("{} ", dep_recipe.name))
              || trimmed == dep_recipe.name
              || trimmed.starts_with(&format!("{}:", dep_recipe.name))
            {
              // Find the end of this recipe (next non-indented line)
              let mut end = i + 1;
              while end < lines.len()
                && (lines[end].starts_with(' ')
                  || lines[end].starts_with('\t')
                  || lines[end].trim().is_empty())
              {
                end += 1;
              }
              return Some(end);
            }
          }
        }
      }
    }

    // Second priority: insert after recipes with similar name prefixes
    // e.g., "test-unit" goes near "test-integration", "deploy-staging" near "deploy-prod"
    if let Some(prefix_end) = recipe_name.find('-') {
      let prefix = &recipe_name[..=prefix_end];
      let mut candidates = Vec::new();
      for recipe in &self.recipes {
        if recipe.name.starts_with(prefix)
          && recipe.name != recipe_name
          && let Some(source) = &self.modules.first().map(|m| m.source.clone())
          && let Ok(content) = std::fs::read_to_string(source)
        {
          let lines: Vec<&str> = content.lines().collect();
          for (i, line) in lines.iter().enumerate() {
            let trimmed = line.trim_start();
            if trimmed.starts_with(&format!("{} ", recipe.name))
              || trimmed == recipe.name
              || trimmed.starts_with(&format!("{}:", recipe.name))
            {
              let mut end = i + 1;
              while end < lines.len()
                && (lines[end].starts_with(' ')
                  || lines[end].starts_with('\t')
                  || lines[end].trim().is_empty())
              {
                end += 1;
              }
              candidates.push(end);
              break;
            }
          }
        }
      }
      if !candidates.is_empty() {
        // Use the last matching recipe's end as insertion point
        return Some(*candidates.iter().max().unwrap());
      }
    }

    // Third priority: insert after recipes in the same module with similar purpose
    // Group by common prefixes (test, build, deploy, lint, fmt, clean, etc.)
    let purpose_prefixes = [
      "test", "build", "deploy", "lint", "fmt", "clean", "check", "ci", "dev", "docs",
    ];
    for prefix in &purpose_prefixes {
      if recipe_name.starts_with(prefix) {
        let mut candidates = Vec::new();
        for recipe in &self.recipes {
          if recipe.name.starts_with(prefix)
            && recipe.name != recipe_name
            && let Some(source) = &self.modules.first().map(|m| m.source.clone())
            && let Ok(content) = std::fs::read_to_string(source)
          {
            let lines: Vec<&str> = content.lines().collect();
            for (i, line) in lines.iter().enumerate() {
              let trimmed = line.trim_start();
              if trimmed.starts_with(&format!("{} ", recipe.name))
                || trimmed == recipe.name
                || trimmed.starts_with(&format!("{}:", recipe.name))
              {
                let mut end = i + 1;
                while end < lines.len()
                  && (lines[end].starts_with(' ')
                    || lines[end].starts_with('\t')
                    || lines[end].trim().is_empty())
                {
                  end += 1;
                }
                candidates.push(end);
                break;
              }
            }
          }
        }
        if !candidates.is_empty() {
          return Some(*candidates.iter().max().unwrap());
        }
      }
    }

    None
  }
}

impl ProjectContext {
  /// Find recipes that are never used as dependencies by other recipes.
  /// These are "leaf" recipes that nothing depends on (excluding private recipes which may be called directly).
  pub fn find_unreferenced_recipes(&self) -> Vec<&ContextRecipe> {
    let mut referenced: std::collections::HashSet<&str> = std::collections::HashSet::new();

    // Collect all recipes that are referenced as dependencies
    for recipe in &self.recipes {
      for dep in &recipe.dependencies {
        referenced.insert(dep.as_str());
      }
    }

    // Find recipes that are never referenced and are not private
    self
      .recipes
      .iter()
      .filter(|recipe| {
        !referenced.contains(recipe.namepath.as_str())
          && !referenced.contains(recipe.name.as_str())
          && !recipe.private
      })
      .collect()
  }

  /// Find recipes that have no dependencies and are not depended upon (completely isolated).
  pub fn find_isolated_recipes(&self) -> Vec<&ContextRecipe> {
    let mut referenced: std::collections::HashSet<&str> = std::collections::HashSet::new();

    for recipe in &self.recipes {
      for dep in &recipe.dependencies {
        referenced.insert(dep.as_str());
      }
    }

    self
      .recipes
      .iter()
      .filter(|recipe| {
        recipe.dependencies.is_empty()
          && !referenced.contains(recipe.namepath.as_str())
          && !referenced.contains(recipe.name.as_str())
      })
      .collect()
  }

  /// Build a dependency graph and detect cycles.
  pub fn detect_cycles(&self) -> Vec<Vec<String>> {
    let mut graph: HashMap<String, Vec<String>> = HashMap::new();

    for recipe in &self.recipes {
      graph.insert(recipe.namepath.clone(), recipe.dependencies.clone());
    }

    let mut cycles = Vec::new();
    let mut visited: HashSet<String> = HashSet::new();
    let mut rec_stack: HashSet<String> = HashSet::new();
    let mut path = Vec::new();

    fn dfs(
      node: &str,
      graph: &HashMap<String, Vec<String>>,
      visited: &mut HashSet<String>,
      rec_stack: &mut HashSet<String>,
      path: &mut Vec<String>,
      cycles: &mut Vec<Vec<String>>,
    ) {
      visited.insert(node.to_owned());
      rec_stack.insert(node.to_owned());
      path.push(node.to_owned());

      if let Some(neighbors) = graph.get(node) {
        for neighbor in neighbors {
          if !visited.contains(neighbor) {
            dfs(neighbor, graph, visited, rec_stack, path, cycles);
          } else if rec_stack.contains(neighbor) {
            // Found a cycle - extract it from path
            if let Some(idx) = path.iter().position(|n| n == neighbor) {
              cycles.push(path[idx..].to_vec());
            }
          }
        }
      }

      rec_stack.remove(node);
      path.pop();
    }

    let graph_owned = graph;
    for node in graph_owned.keys() {
      if !visited.contains(node) {
        dfs(
          node,
          &graph_owned,
          &mut visited,
          &mut rec_stack,
          &mut path,
          &mut cycles,
        );
      }
    }

    // Deduplicate cycles (same cycle detected from different starting points)
    let mut unique_cycles = Vec::new();
    for cycle in cycles {
      let min_idx = cycle
        .iter()
        .enumerate()
        .min_by_key(|(_, s)| *s)
        .map(|(i, _)| i)
        .unwrap_or(0);
      let normalized: Vec<String> = (0..cycle.len())
        .map(|i| cycle[(min_idx + i) % cycle.len()].clone())
        .collect();
      if !unique_cycles.iter().any(|c: &Vec<String>| c == &normalized) {
        unique_cycles.push(normalized);
      }
    }

    unique_cycles
  }

  /// Calculate dependency depth for each recipe (longest path from a root).
  /// Roots (recipes with no dependencies) have depth 0.
  pub fn calculate_dependency_depths(&self) -> HashMap<String, usize> {
    let mut graph: HashMap<String, Vec<String>> = HashMap::new();
    let mut reverse_graph: HashMap<String, Vec<String>> = HashMap::new();

    for recipe in &self.recipes {
      graph.insert(recipe.namepath.clone(), recipe.dependencies.clone());
      // Build reverse graph for finding roots
      for dep in &recipe.dependencies {
        reverse_graph
          .entry(dep.clone())
          .or_default()
          .push(recipe.namepath.clone());
      }
      // Ensure all nodes are in reverse_graph
      reverse_graph.entry(recipe.namepath.clone()).or_default();
    }

    let mut depths = HashMap::new();

    // Find roots (nodes with no outgoing edges = no dependencies)
    let roots: Vec<&String> = graph
      .iter()
      .filter(|(_, deps)| deps.is_empty())
      .map(|(name, _)| name)
      .collect();

    // BFS from roots to compute depths
    use std::collections::VecDeque;
    let mut queue = VecDeque::new();

    // Initialize roots with depth 0
    for root in roots {
      depths.insert(root.clone(), 0);
      queue.push_back((root.clone(), 0));
    }

    while let Some((node, depth)) = queue.pop_front() {
      if let Some(dependents) = reverse_graph.get(&node) {
        for dependent in dependents {
          let new_depth = depth + 1;
          // Only update if this path gives a greater depth
          if depths.get(dependent).is_none_or(|&d| new_depth > d) {
            depths.insert(dependent.clone(), new_depth);
            queue.push_back((dependent.clone(), new_depth));
          }
        }
      }
    }

    // Ensure all recipes have an entry (isolated nodes not reachable from roots)
    for recipe in &self.recipes {
      depths.entry(recipe.namepath.clone()).or_insert(0);
    }

    depths
  }

  /// Find recipes with similar bodies (potential duplicates).
  pub fn find_similar_recipes(&self, threshold: f64) -> Vec<(String, String, f64)> {
    use similar::TextDiff;

    let mut similar = Vec::new();

    for i in 0..self.recipes.len() {
      for j in (i + 1)..self.recipes.len() {
        let r1 = &self.recipes[i];
        let r2 = &self.recipes[j];

        // Skip if in same module and same name (shouldn't happen)
        if r1.namepath == r2.namepath {
          continue;
        }

        let body1 = r1.body.join("\n");
        let body2 = r2.body.join("\n");

        let diff = TextDiff::from_lines(&body1, &body2);
        let total_changes = diff.iter_all_changes().count() as f64;
        let equal_changes = diff
          .iter_all_changes()
          .filter(|c| c.tag() == similar::ChangeTag::Equal)
          .count() as f64;

        if total_changes > 0.0 {
          let similarity = equal_changes / total_changes;
          if similarity >= threshold {
            similar.push((r1.namepath.clone(), r2.namepath.clone(), similarity));
          }
        }
      }
    }

    similar.sort_by(|a, b| b.2.partial_cmp(&a.2).unwrap());
    similar
  }
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct ContextModule {
  pub module_path: String,
  pub recipe_count: usize,
  pub source: PathBuf,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct ContextRecipe {
  pub body: Vec<String>,
  pub dependencies: Vec<String>,
  pub doc: Option<String>,
  pub module_path: String,
  pub name: String,
  pub namepath: String,
  pub parameters: Vec<ContextParameter>,
  pub private: bool,
  pub quiet: bool,
  pub risk: RiskLevel,
  pub risks: Vec<RiskFinding>,
  pub shebang: bool,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct ContextParameter {
  pub default: Option<String>,
  pub kind: String,
  pub name: String,
}

pub(crate) fn render_body_line(value: &Value) -> String {
  match value {
    Value::Array(parts) => parts
      .iter()
      .map(render_fragment)
      .collect::<Vec<_>>()
      .join(""),
    Value::String(string) => string.clone(),
    other => other.to_string(),
  }
}

fn render_value(value: &Value) -> String {
  match value {
    Value::Array(_) => render_fragment(value),
    Value::String(string) => string.clone(),
    other => other.to_string(),
  }
}

fn render_fragment(value: &Value) -> String {
  match value {
    Value::String(string) => string.clone(),
    Value::Array(parts) => {
      let head = parts
        .first()
        .and_then(Value::as_str)
        .unwrap_or("expression");
      format!("{{{{{head}:...}}}}")
    }
    other => other.to_string(),
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn parses_versioned_basic_dump_fixture() {
    let dump: DumpModule =
      serde_json::from_str(include_str!("../tests/fixtures/just-dump-basic.json")).unwrap();
    let context = ProjectContext::from_dump(dump);
    assert_eq!(context.recipes.len(), 2);
    assert_eq!(context.warnings, ["fixture warning"]);
    let deploy = context.find_recipe("deploy").unwrap();
    assert_eq!(deploy.dependencies, ["test"]);
    assert_eq!(deploy.parameters[0].default.as_deref(), Some("production"));
    assert_eq!(deploy.risk, RiskLevel::Medium);
  }

  #[test]
  fn parses_versioned_rich_dump_fixture() {
    let dump: DumpModule =
      serde_json::from_str(include_str!("../tests/fixtures/just-dump-rich.json")).unwrap();
    let context = ProjectContext::from_dump(dump);

    assert_eq!(
      context
        .modules
        .iter()
        .map(|module| module.module_path.as_str())
        .collect::<Vec<_>>(),
      ["", "tools", "tools:ci"]
    );
    assert_eq!(context.warnings, ["module warning"]);

    let script = context.find_recipe("script").unwrap();
    assert!(script.shebang);
    assert!(script.quiet);
    assert_eq!(script.body, ["#!/usr/bin/env bash", "echo {{TARGET:...}}"]);
    assert_eq!(script.parameters[0].kind, "singular");
    assert_eq!(script.parameters[1].kind, "plus");

    let optional = context.find_recipe("optional").unwrap();
    assert!(optional.private);
    assert_eq!(optional.parameters[0].kind, "star");

    let nested = context.find_recipe("tools::ci::test").unwrap();
    assert_eq!(nested.module_path, "tools:ci");
    assert_eq!(nested.body, ["cargo test"]);
  }

  #[test]
  fn parses_versioned_windows_dump_fixture() {
    let dump: DumpModule =
      serde_json::from_str(include_str!("../tests/fixtures/just-dump-windows.json")).unwrap();
    let context = ProjectContext::from_dump(dump);

    assert_eq!(context.warnings, ["windows fixture warning"]);
    assert_eq!(context.modules.len(), 2);
    assert_eq!(
      context.modules[0].source,
      PathBuf::from(r"C:\workspace\justfile")
    );
    assert_eq!(
      context.modules[1].source,
      PathBuf::from(r"C:\workspace\tools.just")
    );

    let build = context.find_recipe("build").unwrap();
    assert_eq!(build.dependencies, ["prepare"]);
    assert_eq!(
      build.body,
      [r#"powershell.exe -NoProfile -Command "Write-Output {{TARGET:...}}""#]
    );
    assert_eq!(
      build.parameters[0].default.as_deref(),
      Some(r"C:\Program Files\just-ai")
    );
    assert_eq!(build.parameters[1].kind, "star");
    assert!(build.quiet);

    let nested = context.find_recipe("tools::status").unwrap();
    assert_eq!(nested.module_path, "tools");
    assert_eq!(nested.body, ["git status --short"]);

    #[cfg(windows)]
    assert!(context.root_source().unwrap().is_absolute());
  }

  #[test]
  fn parses_versioned_migrate_fixture() {
    let dump: DumpModule =
      serde_json::from_str(include_str!("../tests/fixtures/just-dump-migrate.json")).unwrap();
    let context = ProjectContext::from_dump(dump);

    assert_eq!(context.recipes.len(), 7);
    assert_eq!(context.modules.len(), 1);

    // test-unit and test-integration are unreferenced (nothing depends on them)
    // but build depends on test-unit, so test-unit is referenced
    let unreferenced = context.find_unreferenced_recipes();
    assert_eq!(unreferenced.len(), 4); // test-integration, fmt, clean, deploy are unreferenced
    // build depends on test-unit, lint. deploy depends on build.
    // So referenced: test-unit, lint, build
    // Unreferenced: test-integration, fmt, clean, deploy (private recipes filtered out)
    let unreferenced_names: Vec<_> = unreferenced.iter().map(|r| r.namepath.clone()).collect();
    assert!(unreferenced_names.contains(&"test-integration".to_string()));
    assert!(unreferenced_names.contains(&"fmt".to_string()));
    assert!(unreferenced_names.contains(&"clean".to_string()));
    assert!(unreferenced_names.contains(&"deploy".to_string()));
    assert!(!unreferenced_names.contains(&"test-unit".to_string()));
    assert!(!unreferenced_names.contains(&"lint".to_string()));
    assert!(!unreferenced_names.contains(&"build".to_string()));

    // Isolated recipes: no deps, no dependents
    // test-integration, fmt, clean have no deps and no one depends on them
    // deploy has a dependency on build, so it's NOT isolated
    // test-unit has no deps but build depends on it
    // lint has no deps but build depends on it
    let isolated = context.find_isolated_recipes();
    let isolated_names: Vec<_> = isolated.iter().map(|r| r.namepath.clone()).collect();
    assert!(isolated_names.contains(&"test-integration".to_string()));
    assert!(isolated_names.contains(&"fmt".to_string()));
    assert!(isolated_names.contains(&"clean".to_string()));
    assert!(!isolated_names.contains(&"deploy".to_string())); // deploy depends on build
    assert_eq!(isolated.len(), 3);

    // No cycles
    let cycles = context.detect_cycles();
    assert!(cycles.is_empty());

    // Dependency depths: build depends on test-unit, lint (depth 1 each). deploy depends on build (depth 2).
    // test-unit: 0, test-integration: 0, lint: 0, fmt: 0, clean: 0, build: 1, deploy: 2
    let depths = context.calculate_dependency_depths();
    assert_eq!(depths.get("test-unit").copied(), Some(0));
    assert_eq!(depths.get("test-integration").copied(), Some(0));
    assert_eq!(depths.get("lint").copied(), Some(0));
    assert_eq!(depths.get("fmt").copied(), Some(0));
    assert_eq!(depths.get("clean").copied(), Some(0));
    assert_eq!(depths.get("build").copied(), Some(1));
    assert_eq!(depths.get("deploy").copied(), Some(2));
  }

  #[test]
  fn detects_cycles() {
    let dump: DumpModule =
      serde_json::from_str(include_str!("../tests/fixtures/just-dump-cycle.json")).unwrap();
    let context = ProjectContext::from_dump(dump);

    let cycles = context.detect_cycles();
    assert_eq!(cycles.len(), 1);
    assert_eq!(cycles[0].len(), 3);
    // Cycle should be normalized to start with lexicographically smallest
    assert!(cycles[0].contains(&"a".to_string()));
    assert!(cycles[0].contains(&"b".to_string()));
    assert!(cycles[0].contains(&"c".to_string()));
  }

  #[test]
  fn finds_similar_recipes() {
    let dump: DumpModule =
      serde_json::from_str(include_str!("../tests/fixtures/just-dump-similar.json")).unwrap();
    let context = ProjectContext::from_dump(dump);

    // test-unit and test-unit-alt have identical bodies (100% similar)
    let similar = context.find_similar_recipes(0.8);
    assert!(!similar.is_empty());

    let test_unit_pair = similar.iter().find(|(a, b, _)| {
      (a == "test-unit" && b == "test-unit-alt") || (a == "test-unit-alt" && b == "test-unit")
    });
    assert!(test_unit_pair.is_some());
    assert_eq!(test_unit_pair.unwrap().2, 1.0); // identical = 100% similarity

    // build-release and build-debug are different (cargo build --release vs cargo build)
    let build_pair = similar.iter().find(|(a, b, _)| {
      (a == "build-release" && b == "build-debug") || (a == "build-debug" && b == "build-release")
    });
    // They might have some similarity but less than 0.8
    if let Some(pair) = build_pair {
      assert!(pair.2 < 0.8);
    }
  }

  #[test]
  fn parses_versioned_modularize_fixture() {
    let dump: DumpModule =
      serde_json::from_str(include_str!("../tests/fixtures/just-dump-modularize.json")).unwrap();
    let context = ProjectContext::from_dump(dump);

    assert_eq!(context.recipes.len(), 8);
    assert_eq!(context.modules.len(), 1);

    // Group recipes by prefix
    let mut groups: HashMap<String, Vec<&ContextRecipe>> = HashMap::new();
    for recipe in &context.recipes {
      let prefix = recipe
        .name
        .split('-')
        .next()
        .unwrap_or(&recipe.name)
        .to_owned();
      groups.entry(prefix).or_default().push(recipe);
    }

    // build recipes: build, build-release (2)
    assert_eq!(groups.get("build").map(|v| v.len()), Some(2));
    // test recipes: test-unit, test-integration (2)
    assert_eq!(groups.get("test").map(|v| v.len()), Some(2));
    // Other singles: lint, fmt, deploy, clean (1 each)
    assert_eq!(groups.get("lint").map(|v| v.len()), Some(1));
    assert_eq!(groups.get("fmt").map(|v| v.len()), Some(1));
    assert_eq!(groups.get("deploy").map(|v| v.len()), Some(1));
    assert_eq!(groups.get("clean").map(|v| v.len()), Some(1));
  }
}
