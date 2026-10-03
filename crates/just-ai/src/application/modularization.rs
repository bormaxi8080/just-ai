//! Validated, deterministic plans for moving root recipes into imported files.

use {
  crate::{
    ProjectContext,
    application::patches,
    bounded_file::{max_editable_file_bytes, read_utf8},
    proposal::validate_justfile,
  },
  std::{
    collections::BTreeMap,
    error::Error,
    fs,
    path::{Path, PathBuf},
  },
};

pub struct PlannedModule {
  pub prefix: String,
  pub recipes: Vec<String>,
  pub path: PathBuf,
  pub content: String,
}

pub struct ModularizationPlan {
  pub source: PathBuf,
  pub original: String,
  pub proposed: String,
  pub modules: Vec<PlannedModule>,
}

impl ModularizationPlan {
  pub fn prepare(context: &ProjectContext) -> Result<Self, Box<dyn Error>> {
    let source = context
      .root_source()
      .ok_or("missing root justfile")?
      .to_owned();
    let original = read_utf8(&source, max_editable_file_bytes())?;
    // Existing imports may use arbitrary paths and share recipe names. Keep
    // those projects intact until an import-aware migration is available.
    if context.modules.len() != 1
      || original.lines().any(|line| {
        ["import ", "import? ", "mod ", "mod? "]
          .iter()
          .any(|prefix| line.starts_with(prefix))
      })
    {
      return Err(
        "modularization of existing imports or modules is not supported; no files changed".into(),
      );
    }
    let parent = source.parent().ok_or("missing source directory")?;
    let mut groups: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for recipe in &context.recipes {
      let prefix = recipe.name.split('-').next().unwrap_or(&recipe.name);
      groups
        .entry(prefix.to_owned())
        .or_default()
        .push(recipe.name.clone());
    }
    let mut modules = Vec::new();
    let mut removed = Vec::new();
    for (prefix, recipes) in groups {
      if recipes.len() < 2 {
        continue;
      }
      let path = parent.join(format!("{prefix}.just"));
      if fs::symlink_metadata(&path).is_ok() {
        return Err(format!("refusing to overwrite existing module {}", path.display()).into());
      }
      let mut content = String::new();
      for recipe in &recipes {
        let range = recipe_range(&original, recipe)
          .ok_or_else(|| format!("cannot locate recipe {recipe}"))?;
        content.push_str(original[range.clone()].trim_end());
        content.push_str("\n\n");
        removed.push(range);
      }
      modules.push(PlannedModule {
        prefix,
        recipes,
        path,
        content,
      });
    }
    removed.sort_by_key(|range| range.start);
    if removed.windows(2).any(|pair| pair[0].end > pair[1].start) {
      return Err("ambiguous recipe boundaries; no files changed".into());
    }
    let mut proposed = original.clone();
    for range in removed.into_iter().rev() {
      proposed.replace_range(range, "");
    }
    let imports = modules
      .iter()
      .map(|module| format!("import '{}.just'\n", module.prefix))
      .collect::<String>();
    proposed = format!("{imports}{proposed}");
    Ok(Self {
      source,
      original,
      proposed,
      modules,
    })
  }

  pub fn validate(&self, just_binary: &Path) -> Result<(), Box<dyn Error>> {
    let staging = tempfile::tempdir()?;
    let source = staging.path().join("justfile");
    for module in &self.modules {
      fs::write(
        staging
          .path()
          .join(module.path.file_name().ok_or("invalid module path")?),
        &module.content,
      )?;
    }
    validate_justfile(just_binary, &source, &self.proposed)
  }

  pub fn apply(&self, just_binary: &Path) -> Result<(), Box<dyn Error>> {
    self.validate(just_binary)?;
    let additions = self
      .modules
      .iter()
      .map(|module| (module.path.clone(), module.content.clone()))
      .collect::<Vec<_>>();
    patches::apply_reviewed_changes(&self.source, &self.original, &self.proposed, &additions)?;
    Ok(())
  }
}

pub(crate) fn recipe_range(content: &str, name: &str) -> Option<std::ops::Range<usize>> {
  let lines = content.split_inclusive('\n').collect::<Vec<_>>();
  let header = lines.iter().position(|line| {
    line
      .strip_prefix(name)
      .is_some_and(|rest| rest.starts_with(':') || rest.starts_with(' ') || rest.starts_with('\t'))
  })?;
  let mut start = header;
  while start > 0 && (lines[start - 1].starts_with('#') || lines[start - 1].starts_with('[')) {
    start -= 1;
  }
  let mut end = header + 1;
  while end < lines.len()
    && (lines[end].starts_with(' ') || lines[end].starts_with('\t') || lines[end].trim().is_empty())
  {
    end += 1;
  }
  Some(
    lines[..start].iter().map(|line| line.len()).sum()
      ..lines[..end].iter().map(|line| line.len()).sum(),
  )
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn preserves_recipe_attributes_and_documentation() {
    let content = "x := 'a'\n\n# unit test\n[private]\ntest-a:\n  echo a\n\ntest-b:\n  echo b\n";
    let range = recipe_range(content, "test-a").unwrap();
    assert_eq!(
      &content[range],
      "# unit test\n[private]\ntest-a:\n  echo a\n\n"
    );
  }
}
