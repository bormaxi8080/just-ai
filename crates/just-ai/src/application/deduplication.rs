//! Source-preserving plans for removing exact, unreferenced duplicate recipes.
use crate::{
  ProjectContext,
  application::{modularization::recipe_range, patches::apply_reviewed_change},
  bounded_file::{max_editable_file_bytes, read_utf8},
  proposal::validate_justfile,
};
use std::{
  collections::HashSet,
  error::Error,
  path::{Path, PathBuf},
};

pub struct DeduplicationPlan {
  pub source: PathBuf,
  pub original: String,
  pub proposed: String,
  pub similar_pairs: Vec<(String, String, f64)>,
  pub removed: Vec<String>,
  pub skipped: Vec<String>,
}
impl DeduplicationPlan {
  pub fn prepare(
    context: &ProjectContext,
    threshold: f64,
    merge: bool,
  ) -> Result<Self, Box<dyn Error>> {
    if !threshold.is_finite() || !(0.0..=1.0).contains(&threshold) {
      return Err("similarity threshold must be between zero and one".into());
    }
    let source = context
      .root_source()
      .ok_or("missing root source")?
      .to_owned();
    let original = read_utf8(&source, max_editable_file_bytes())?;
    if context.modules.len() != 1
      || original.lines().any(|line| {
        ["import ", "import? ", "mod ", "mod? "]
          .iter()
          .any(|prefix| line.trim_start().starts_with(prefix))
      })
    {
      return Err(
        "deduplication of existing imports or modules is not supported; no files changed".into(),
      );
    }
    let similar_pairs = context.find_similar_recipes(threshold);
    let mut removed = Vec::new();
    let mut skipped = Vec::new();
    let mut ranges = Vec::new();
    let mut seen = HashSet::new();
    for (a, b, _) in &similar_pairs {
      if seen.contains(a) || seen.contains(b) {
        continue;
      }
      let ra = context.find_recipe(a).ok_or("missing first recipe")?;
      let rb = context.find_recipe(b).ok_or("missing second recipe")?;
      let range_a = recipe_range(&original, &ra.name).ok_or("missing first source range")?;
      let range_b = recipe_range(&original, &rb.name).ok_or("missing second source range")?;
      let canonical = |text: &str, name: &str| {
        text
          .lines()
          .map(|line| {
            if line.starts_with(&format!("{name}:")) || line.starts_with(&format!("{name} ")) {
              format!("RECIPE{}", &line[name.len()..])
            } else {
              line.to_owned()
            }
          })
          .collect::<Vec<_>>()
          .join("\n")
          .trim_end()
          .to_owned()
      };
      if canonical(&original[range_a.clone()], &ra.name)
        != canonical(&original[range_b.clone()], &rb.name)
      {
        if merge {
          return Err("automatic merging of non-identical recipes cannot preserve semantics; no files changed".into());
        }
        skipped.push(format!("{a}, {b}: not source-equivalent"));
        continue;
      }
      let (remove, range) = if a.len() <= b.len() {
        (b, range_b)
      } else {
        (a, range_a)
      };
      if context
        .recipes
        .iter()
        .any(|recipe| recipe.dependencies.iter().any(|dep| dep == remove))
      {
        skipped.push(format!("{remove}: referenced by another recipe"));
        continue;
      }
      seen.insert(remove.clone());
      removed.push(remove.clone());
      ranges.push(range);
    }
    ranges.sort_by_key(|range| range.start);
    let mut proposed = original.clone();
    for range in ranges.into_iter().rev() {
      proposed.replace_range(range, "");
    }
    Ok(Self {
      source,
      original,
      proposed,
      similar_pairs,
      removed,
      skipped,
    })
  }
  pub fn validate(&self, binary: &Path) -> Result<(), Box<dyn Error>> {
    validate_justfile(binary, &self.source, &self.proposed)
  }
  pub fn apply(&self, binary: &Path) -> Result<(), Box<dyn Error>> {
    self.validate(binary)?;
    apply_reviewed_change(&self.source, &self.original, &self.proposed)?;
    Ok(())
  }
}
