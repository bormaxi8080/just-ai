//! Shared deterministic instantiation of stored recipe templates.
use crate::{
  ProjectContext,
  ai_responses::{RecipeProposal, TemplateProposal},
  application::patches::apply_reviewed_change,
  bounded_file::{ensure_text_limit, max_editable_file_bytes, read_utf8},
  proposal::{
    insert_recipe_grouped, parse_template_body, render_recipe, validate_generated_source,
    validate_proposal,
  },
};
use std::{
  collections::HashMap,
  error::Error,
  path::{Path, PathBuf},
};
pub struct TemplatePlan {
  pub source: PathBuf,
  pub original: String,
  pub proposed: String,
  pub recipes: Vec<RecipeProposal>,
}
impl TemplatePlan {
  pub fn prepare(
    context: &ProjectContext,
    template: &TemplateProposal,
    supplied: &HashMap<String, String>,
    force: bool,
  ) -> Result<Self, Box<dyn Error>> {
    if supplied
      .keys()
      .any(|key| !template.parameters.iter().any(|param| &param.name == key))
    {
      return Err("unknown template parameter".into());
    }
    let mut values = supplied.clone();
    for param in &template.parameters {
      if !values.contains_key(&param.name) {
        if let Some(default) = &param.default {
          values.insert(param.name.clone(), default.clone());
        } else if param.required {
          return Err(format!("required parameter '{}' not provided", param.name).into());
        }
      }
    }
    let mut body = Vec::new();
    for line in &template.body {
      // Substitute in one pass: values must not introduce recursive placeholders.
      let mut rendered = String::new();
      let mut remaining = line.as_str();
      while let Some(start) = remaining.find("{{") {
        rendered.push_str(&remaining[..start]);
        let after = &remaining[start + 2..];
        let Some(end) = after.find("}}") else {
          return Err("unterminated template placeholder".into());
        };
        let key = &after[..end];
        rendered.push_str(
          values
            .get(key)
            .ok_or_else(|| format!("unresolved template placeholder '{key}'"))?,
        );
        remaining = &after[end + 2..];
      }
      rendered.push_str(remaining);
      body.push(rendered);
    }
    let recipes = parse_template_body(
      &body,
      &template.name,
      &template.description,
      &template.parameters,
    )?;
    let source = context
      .root_source()
      .ok_or("missing root source")?
      .to_owned();
    let original = read_utf8(&source, max_editable_file_bytes())?;
    let mut proposed = original.clone();
    let mut selected = Vec::new();
    for recipe in &recipes {
      if let Some(existing) = context.find_recipe(&recipe.name)
        && (force || (existing.body == recipe.body && existing.dependencies == recipe.dependencies))
      {
        continue;
      }
      validate_proposal(context, recipe, Some(&recipes))?;
      proposed = insert_recipe_grouped(
        &proposed,
        &render_recipe(recipe),
        context,
        &recipe.dependencies,
        &recipe.name,
      );
      selected.push(recipe.clone());
    }
    ensure_text_limit(&proposed, "proposed justfile", max_editable_file_bytes())?;
    Ok(Self {
      source,
      original,
      proposed,
      recipes: selected,
    })
  }
  pub fn validate(&self, binary: &Path) -> Result<(), Box<dyn Error>> {
    validate_generated_source(binary, &self.source, &self.proposed)
  }
  pub fn apply(&self, binary: &Path) -> Result<(), Box<dyn Error>> {
    self.validate(binary)?;
    apply_reviewed_change(&self.source, &self.original, &self.proposed)?;
    Ok(())
  }
}
