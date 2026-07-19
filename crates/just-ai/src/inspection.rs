use {
  crate::{
    application,
    domain::risk::{RiskFinding, RiskLevel},
  },
  serde::{Deserialize, Serialize},
  serde_json::Value,
  std::{
    collections::BTreeMap,
    error::Error,
    fmt::{self, Display, Formatter},
    path::{Path, PathBuf},
    process::Command,
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
  let mut command = Command::new(just_binary);
  command.args(["--dump", "--dump-format", "json"]);
  if let Some(project_root) = project_root {
    command.current_dir(project_root);
  }
  let output = command.output()?;

  if !output.status.success() {
    return Err(
      DumpError {
        status: output.status.to_string(),
        stderr: String::from_utf8_lossy(&output.stderr).trim().to_owned(),
      }
      .into(),
    );
  }

  Ok(serde_json::from_slice(&output.stdout)?)
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

#[derive(Debug)]
pub(crate) struct DumpError {
  pub(crate) status: String,
  pub(crate) stderr: String,
}

impl Display for DumpError {
  fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
    if self.stderr.is_empty() {
      write!(f, "just dump failed with {}", self.status)
    } else {
      write!(f, "just dump failed with {}: {}", self.status, self.stderr)
    }
  }
}

impl Error for DumpError {}

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

#[derive(Debug, Deserialize, Serialize)]
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

  pub(crate) fn find_recipe(&self, needle: &str) -> Option<&ContextRecipe> {
    self
      .recipes
      .iter()
      .find(|recipe| recipe.namepath == needle || recipe.name == needle)
  }

  pub(crate) fn has_recipe(&self, name: &str) -> bool {
    self
      .recipes
      .iter()
      .any(|recipe| recipe.name == name || recipe.namepath == name)
  }

  pub(crate) fn root_source(&self) -> Option<&Path> {
    self.modules.first().map(|module| module.source.as_path())
  }
}

#[derive(Debug, Deserialize, Serialize)]
pub struct ContextModule {
  pub module_path: String,
  pub recipe_count: usize,
  pub source: PathBuf,
}

#[derive(Debug, Deserialize, Serialize)]
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

#[derive(Debug, Deserialize, Serialize)]
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
}
