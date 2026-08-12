use {
  crate::{
    ProjectContext,
    ai_responses::{
      AddRecipeResponse, ComposeWorkflowResponse, FixProposal, FixResponse,
      RecipeParameterProposal, RecipeProposal, TemplateProposal, TemplateResponse,
      WorkflowResponse,
    },
    application,
    bounded_file::{self, max_editable_file_bytes},
    bounded_output,
    cli::print_section,
    domain::risk::{RiskFinding, RiskLevel},
    inspection,
    just_dump::DumpError,
  },
  similar::{ChangeTag, TextDiff},
  std::{
    collections::HashMap,
    error::Error,
    fs,
    path::{Path, PathBuf},
    process::Command,
    time::{SystemTime, UNIX_EPOCH},
  },
};

pub fn handle_add(
  just_binary: &Path,
  context: &ProjectContext,
  request: &str,
  response: AddRecipeResponse,
  write: bool,
) -> Result<(), Box<dyn Error>> {
  validate_proposal(context, &response.recipe)?;

  let source = context
    .root_source()
    .ok_or("project context does not contain a root justfile source")?;
  let original = bounded_file::read_utf8(source, max_editable_file_bytes())?;
  let recipe = render_recipe(&response.recipe);
  let proposed = insert_recipe_grouped(
    &original,
    &recipe,
    context,
    &response.recipe.dependencies,
    &response.recipe.name,
  );
  bounded_file::ensure_text_limit(&proposed, "proposed justfile", max_editable_file_bytes())?;
  validate_justfile(just_binary, source, &proposed)?;

  let risks = RiskFinding::scan_lines(&response.recipe.body);
  let risk = RiskLevel::highest(&risks);
  if risk == RiskLevel::Blocked {
    return Err("generated recipe has blocked risk and will not be written".into());
  }

  println!("{}", response.summary);
  println!();
  println!("Request: {request}");
  println!("Recipe: {} [{}]", response.recipe.name, risk);

  print_section("Rationale", &response.rationale);

  if !risks.is_empty() {
    println!();
    println!("Risk findings:");
    for finding in &risks {
      println!("  - {}: `{}`", finding.reason, finding.line);
    }
  }

  println!();
  println!("{}", unified_diff(source, &original, &proposed));

  if write {
    application::patches::apply_reviewed_change(source, &original, &proposed)?;
    println!("Wrote {}", source.display());
  } else {
    println!("Dry run only. Re-run with --write to apply this recipe.");
  }

  Ok(())
}

pub fn handle_fix(
  just_binary: &Path,
  context: &ProjectContext,
  recipe_name: &str,
  response: FixResponse,
  write: bool,
) -> Result<(), Box<dyn Error>> {
  validate_fix_proposal(context, &response.recipe, recipe_name)?;

  let source = context
    .root_source()
    .ok_or("project context does not contain a root justfile source")?;
  let original = bounded_file::read_utf8(source, max_editable_file_bytes())?;
  let recipe = render_fix_recipe(&response.recipe);
  let proposed = replace_recipe(&original, recipe_name, &recipe);
  bounded_file::ensure_text_limit(&proposed, "proposed justfile", max_editable_file_bytes())?;
  validate_justfile(just_binary, source, &proposed)?;

  let risks = RiskFinding::scan_lines(&response.recipe.body);
  let risk = RiskLevel::highest(&risks);
  if risk == RiskLevel::Blocked {
    return Err("generated fix has blocked risk and will not be written".into());
  }

  println!("{}", response.summary);
  println!();
  println!("Recipe to fix: {recipe_name}");
  println!("Fixed recipe: {} [{}]", response.recipe.name, risk);

  print_section("Rationale", &response.rationale);

  if !risks.is_empty() {
    println!();
    println!("Risk findings:");
    for finding in &risks {
      println!("  - {}: `{}`", finding.reason, finding.line);
    }
  }

  println!();
  println!("{}", unified_diff(source, &original, &proposed));

  if write {
    application::patches::apply_reviewed_change(source, &original, &proposed)?;
    println!("Wrote {}", source.display());
  } else {
    println!("Dry run only. Re-run with --write to apply this fix.");
  }

  Ok(())
}

/// Handle workflow proposal - validates and inserts multiple recipes
pub fn handle_workflow(
  just_binary: &Path,
  context: &ProjectContext,
  request: &str,
  response: WorkflowResponse,
  write: bool,
) -> Result<(), Box<dyn Error>> {
  // Validate all recipes in the workflow
  for recipe in &response.recipes {
    validate_workflow_recipe(context, recipe, &response.recipes)?;
  }

  let source = context
    .root_source()
    .ok_or("project context does not contain a root justfile source")?;
  let original = bounded_file::read_utf8(source, max_editable_file_bytes())?;

  // Build a map of recipe name to rendered recipe
  let mut rendered_recipes: HashMap<String, String> = HashMap::new();
  for recipe in &response.recipes {
    rendered_recipes.insert(recipe.name.clone(), render_recipe(recipe));
  }

  // Insert recipes in execution order
  let mut proposed = original.clone();
  for recipe_name in &response.execution_order {
    let recipe = rendered_recipes
      .get(recipe_name)
      .ok_or_else(|| format!("recipe `{recipe_name}` not found in workflow"))?;

    // Find the recipe proposal to get dependencies
    let recipe_proposal = response
      .recipes
      .iter()
      .find(|r| r.name == *recipe_name)
      .ok_or_else(|| format!("recipe proposal for `{recipe_name}` not found"))?;

    proposed = insert_recipe_grouped(
      &proposed,
      recipe,
      context,
      &recipe_proposal.dependencies,
      recipe_name,
    );
  }

  bounded_file::ensure_text_limit(&proposed, "proposed justfile", max_editable_file_bytes())?;
  validate_justfile(just_binary, source, &proposed)?;

  // Check risk for all recipes
  let mut all_risks = Vec::new();
  for recipe in &response.recipes {
    let risks = RiskFinding::scan_lines(&recipe.body);
    all_risks.extend(risks);
  }
  let risk = RiskLevel::highest(&all_risks);
  if risk == RiskLevel::Blocked {
    return Err("generated workflow has blocked risk and will not be written".into());
  }

  println!("{}", response.summary);
  println!();
  println!("Workflow request: {request}");
  println!(
    "Recipes: {}",
    response
      .recipes
      .iter()
      .map(|r| r.name.as_str())
      .collect::<Vec<_>>()
      .join(", ")
  );
  println!("Execution order: {}", response.execution_order.join(" -> "));

  print_section("Rationale", &response.rationale);

  if !all_risks.is_empty() {
    println!();
    println!("Risk findings:");
    for finding in &all_risks {
      println!("  - {}: `{}`", finding.reason, finding.line);
    }
  }

  println!();
  println!("{}", unified_diff(source, &original, &proposed));

  if write {
    application::patches::apply_reviewed_change(source, &original, &proposed)?;
    println!("Wrote {}", source.display());
  } else {
    println!("Dry run only. Re-run with --write to apply this workflow.");
  }

  Ok(())
}

fn validate_workflow_recipe(
  context: &ProjectContext,
  proposal: &RecipeProposal,
  all_recipes: &[RecipeProposal],
) -> Result<(), Box<dyn Error>> {
  if proposal.name.is_empty() {
    return Err("generated recipe name is empty".into());
  }

  if proposal.body.is_empty() {
    return Err("generated recipe body is empty".into());
  }

  if !proposal
    .name
    .chars()
    .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
  {
    return Err(
      format!(
        "recipe name `{}` contains unsupported characters",
        proposal.name
      )
      .into(),
    );
  }

  // Check for duplicate names within the workflow
  let name_count = all_recipes
    .iter()
    .filter(|r| r.name == proposal.name)
    .count();
  if name_count > 1 {
    return Err(format!("duplicate recipe name `{}` in workflow", proposal.name).into());
  }

  // Check if recipe already exists in project (but allow if it's being replaced by this workflow)
  if context.has_recipe(&proposal.name) {
    // Check if this recipe is being replaced by another recipe in the workflow
    let is_replaced = all_recipes
      .iter()
      .any(|r| r.name == proposal.name && r.body != proposal.body);
    if !is_replaced {
      return Err(format!("recipe `{}` already exists", proposal.name).into());
    }
  }

  for dependency in &proposal.dependencies {
    // Check if dependency exists in project or in this workflow
    let in_workflow = all_recipes.iter().any(|r| r.name == *dependency);
    if !context.has_recipe(dependency) && !in_workflow {
      return Err(format!("dependency recipe `{dependency}` does not exist").into());
    }
  }

  Ok(())
}

pub fn validate_fix_proposal(
  context: &ProjectContext,
  proposal: &FixProposal,
  original_recipe_name: &str,
) -> Result<(), Box<dyn Error>> {
  if proposal.name.is_empty() {
    return Err("generated recipe name is empty".into());
  }

  if proposal.body.is_empty() {
    return Err("generated recipe body is empty".into());
  }

  if !proposal
    .name
    .chars()
    .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
  {
    return Err(
      format!(
        "recipe name `{}` contains unsupported characters",
        proposal.name
      )
      .into(),
    );
  }

  for dependency in &proposal.dependencies {
    if !context.has_recipe(dependency) {
      return Err(format!("dependency recipe `{dependency}` does not exist").into());
    }
  }

  // The fix can replace the original recipe, so allow same name
  if proposal.name != original_recipe_name && context.has_recipe(&proposal.name) {
    return Err(format!("recipe `{}` already exists", proposal.name).into());
  }

  Ok(())
}

pub fn render_fix_recipe(proposal: &FixProposal) -> String {
  let mut rendered = String::new();

  if let Some(doc) = &proposal.doc {
    rendered.push_str("# ");
    rendered.push_str(doc.trim());
    rendered.push('\n');
  }

  rendered.push_str(&proposal.name);

  for parameter in &proposal.parameters {
    rendered.push(' ');
    rendered.push_str(&parameter.name);
    if let Some(default) = &parameter.default {
      rendered.push_str("='");
      rendered.push_str(&default.replace('\'', "\\'"));
      rendered.push('\'');
    }
  }

  if !proposal.dependencies.is_empty() {
    rendered.push_str(": ");
    rendered.push_str(
      &proposal
        .dependencies
        .iter()
        .map(|dependency| format!("({dependency})"))
        .collect::<Vec<_>>()
        .join(" "),
    );
  } else {
    rendered.push(':');
  }

  rendered.push('\n');

  for line in &proposal.body {
    rendered.push_str("  ");
    rendered.push_str(line);
    rendered.push('\n');
  }

  rendered
}

pub fn replace_recipe(original: &str, recipe_name: &str, new_recipe: &str) -> String {
  let lines: Vec<&str> = original.lines().collect();
  let mut result = Vec::new();
  let mut i = 0;
  let mut found = false;

  while i < lines.len() {
    let line = lines[i];
    // Check if this line starts a recipe definition matching recipe_name
    // Recipe definition: name [params] [: deps] :
    let trimmed = line.trim_start();
    let is_recipe_def = trimmed.starts_with(&format!("{recipe_name} "))
      || trimmed == recipe_name
      || trimmed.starts_with(&format!("{recipe_name}:"));
    if !found && is_recipe_def {
      // Found the recipe - skip until we hit a non-indented line (next recipe or EOF)
      found = true;
      result.push(new_recipe);
      // Skip the original recipe body (indented lines)
      i += 1;
      while i < lines.len()
        && (lines[i].starts_with(' ') || lines[i].starts_with('\t') || lines[i].trim().is_empty())
      {
        i += 1;
      }
      continue;
    }
    result.push(line);
    i += 1;
  }

  // If recipe wasn't found, append at end (fallback to append behavior)
  if !found {
    result.push("");
    result.push(new_recipe.trim_end());
  }

  result.join("\n")
}

fn validate_proposal(
  context: &ProjectContext,
  proposal: &RecipeProposal,
) -> Result<(), Box<dyn Error>> {
  if proposal.name.is_empty() {
    return Err("generated recipe name is empty".into());
  }

  if proposal.body.is_empty() {
    return Err("generated recipe body is empty".into());
  }

  if context.has_recipe(&proposal.name) {
    return Err(format!("recipe `{}` already exists", proposal.name).into());
  }

  if !proposal
    .name
    .chars()
    .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
  {
    return Err(
      format!(
        "recipe name `{}` contains unsupported characters",
        proposal.name
      )
      .into(),
    );
  }

  for dependency in &proposal.dependencies {
    if !context.has_recipe(dependency) {
      return Err(format!("dependency recipe `{dependency}` does not exist").into());
    }
  }

  Ok(())
}

pub fn render_recipe(proposal: &RecipeProposal) -> String {
  let mut rendered = String::new();

  if let Some(doc) = &proposal.doc {
    rendered.push_str("# ");
    rendered.push_str(doc.trim());
    rendered.push('\n');
  }

  rendered.push_str(&proposal.name);

  for parameter in &proposal.parameters {
    rendered.push(' ');
    rendered.push_str(&parameter.name);
    if let Some(default) = &parameter.default {
      rendered.push_str("='");
      rendered.push_str(&default.replace('\'', "\\'"));
      rendered.push('\'');
    }
  }

  if !proposal.dependencies.is_empty() {
    rendered.push_str(": ");
    rendered.push_str(
      &proposal
        .dependencies
        .iter()
        .map(|dependency| format!("({dependency})"))
        .collect::<Vec<_>>()
        .join(" "),
    );
  } else {
    rendered.push(':');
  }

  rendered.push('\n');

  for line in &proposal.body {
    rendered.push_str("  ");
    rendered.push_str(line);
    rendered.push('\n');
  }

  rendered
}

pub fn append_recipe(original: &str, recipe: &str) -> String {
  let mut proposed = original.to_owned();

  if !proposed.ends_with('\n') {
    proposed.push('\n');
  }

  proposed.push('\n');
  proposed.push_str(recipe);
  proposed
}

/// Insert a recipe at a specific line number in the original content.
pub fn insert_recipe_at(original: &str, recipe: &str, insert_line: usize) -> String {
  let lines: Vec<&str> = original.lines().collect();
  let mut result: Vec<&str> = Vec::new();

  for (i, line) in lines.iter().enumerate() {
    if i == insert_line {
      // Ensure proper spacing before the inserted recipe
      if !result.is_empty() && !result.last().unwrap().trim().is_empty() {
        result.push("");
      }
      result.push(recipe.trim_end());
      if i < lines.len() && !lines[i].trim().is_empty() {
        result.push("");
      }
    }
    result.push(line);
  }

  // If insert_line is at the end (past all lines), append
  if insert_line >= lines.len() {
    if !result.is_empty() && !result.last().unwrap().trim().is_empty() {
      result.push("");
    }
    result.push(recipe.trim_end());
  }

  result.join("\n")
}

/// Insert a recipe grouped with related recipes based on dependencies and naming patterns.
pub fn insert_recipe_grouped(
  original: &str,
  recipe: &str,
  context: &ProjectContext,
  dependencies: &[String],
  recipe_name: &str,
) -> String {
  // Try to find the best insertion point using the context
  if let Some(insert_line) =
    inspection::ProjectContext::find_insertion_point(context, recipe_name, dependencies)
  {
    insert_recipe_at(original, recipe, insert_line)
  } else {
    // Fallback to append behavior
    append_recipe(original, recipe)
  }
}

pub fn validate_justfile(
  just_binary: &Path,
  source: &Path,
  proposed: &str,
) -> Result<(), Box<dyn Error>> {
  let temp_path = temporary_justfile_path(source)?;
  fs::write(&temp_path, proposed)?;

  let mut command = Command::new(just_binary);
  command
    .arg("--justfile")
    .arg(&temp_path)
    .args(["--dump", "--dump-format", "json"]);
  let output = bounded_output::capture(&mut command);

  let remove_result = fs::remove_file(&temp_path);
  let output = output?;

  if let Err(err) = remove_result {
    return Err(format!("failed to remove temporary justfile: {err}").into());
  }

  if !output.status.success() {
    return Err(
      DumpError {
        status: output.status.to_string(),
        stderr: String::from_utf8_lossy(&output.stderr).trim().to_owned(),
      }
      .into(),
    );
  }

  Ok(())
}

fn temporary_justfile_path(source: &Path) -> Result<PathBuf, Box<dyn Error>> {
  let directory = source.parent().unwrap_or_else(|| Path::new("."));
  let nanos = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
  Ok(directory.join(format!(".just-ai-{nanos}.justfile")))
}

pub fn unified_diff(path: &Path, original: &str, proposed: &str) -> String {
  let diff = TextDiff::from_lines(original, proposed);
  let mut rendered = String::new();

  rendered.push_str(&format!("--- {}\n", path.display()));
  rendered.push_str(&format!("+++ {}\n", path.display()));

  for change in diff.iter_all_changes() {
    let sign = match change.tag() {
      ChangeTag::Delete => "-",
      ChangeTag::Insert => "+",
      ChangeTag::Equal => " ",
    };
    rendered.push_str(sign);
    rendered.push_str(change.value());
  }

  rendered
}

/// Handle template proposal - stores the template for later instantiation
pub fn handle_template(
  context: &ProjectContext,
  request: &str,
  response: TemplateResponse,
) -> Result<(), Box<dyn Error>> {
  let source = context
    .root_source()
    .ok_or("project context does not contain a root justfile source")?;
  let _original = bounded_file::read_utf8(source, max_editable_file_bytes())?;

  // Templates are stored as comments in the justfile for now
  // In the future, we could store them in a separate .just-ai-templates file
  let template_name = &response.template.name;
  let _template_json = serde_json::to_string_pretty(&response.template)?;

  println!("{}", response.summary);
  println!();
  println!("Template request: {request}");
  println!("Template name: {template_name}");
  println!("Category: {}", response.template.category);
  println!("Parameters:");
  for param in &response.template.parameters {
    let req = if param.required { " (required)" } else { "" };
    let def = param.default.as_deref().unwrap_or("none");
    println!(
      "  - {}: {}{} [default: {def}]",
      param.name, param.description, req
    );
  }

  print_section("Rationale", &[format!("Template created for: {request}")]);

  println!();
  println!("Template body:");
  for line in &response.template.body {
    println!("  {line}");
  }

  println!();
  println!(
    "To instantiate this template, run: just-ai instantiate-template {template_name} <param=value>..."
  );

  Ok(())
}

/// Handle template instantiation - creates a recipe from a template with provided values
pub fn handle_instantiate_template(
  just_binary: &Path,
  context: &ProjectContext,
  template: &TemplateProposal,
  values: &std::collections::HashMap<String, String>,
  write: bool,
) -> Result<(), Box<dyn Error>> {
  // Substitute template parameters in the body
  let mut recipe_body = Vec::new();
  for line in &template.body {
    let mut substituted = line.clone();
    for (key, value) in values {
      substituted = substituted.replace(&format!("{{{{{key}}}}}"), value);
    }
    recipe_body.push(substituted);
  }

  // Build the recipe proposal from the template
  let recipe = RecipeProposal {
    name: template.name.clone(),
    doc: Some(template.description.clone()),
    parameters: template
      .parameters
      .iter()
      .map(|p| RecipeParameterProposal {
        name: p.name.clone(),
        default: p.default.clone(),
      })
      .collect(),
    dependencies: vec![],
    body: recipe_body,
  };

  validate_proposal(context, &recipe)?;

  let source = context
    .root_source()
    .ok_or("project context does not contain a root justfile source")?;
  let original = bounded_file::read_utf8(source, max_editable_file_bytes())?;
  let rendered = render_recipe(&recipe);
  let proposed = insert_recipe_grouped(
    &original,
    &rendered,
    context,
    &recipe.dependencies,
    &recipe.name,
  );
  bounded_file::ensure_text_limit(&proposed, "proposed justfile", max_editable_file_bytes())?;
  validate_justfile(just_binary, source, &proposed)?;

  let risks = RiskFinding::scan_lines(&recipe.body);
  let risk = RiskLevel::highest(&risks);
  if risk == RiskLevel::Blocked {
    return Err("instantiated template has blocked risk and will not be written".into());
  }

  println!("Template instantiated: {}", template.name);
  println!();
  println!("Recipe: {} [{}]", recipe.name, risk);

  println!();
  println!("{}", unified_diff(source, &original, &proposed));

  if write {
    application::patches::apply_reviewed_change(source, &original, &proposed)?;
    println!("Wrote {}", source.display());
  } else {
    println!("Dry run only. Re-run with --write to apply this recipe.");
  }

  Ok(())
}

/// Handle compose workflow proposal - validates and inserts/modifies recipes based on composition
pub fn handle_compose_workflow(
  just_binary: &Path,
  context: &ProjectContext,
  request: &str,
  response: ComposeWorkflowResponse,
  write: bool,
) -> Result<(), Box<dyn Error>> {
  // Validate all recipes in the workflow
  let existing_recipe_names: std::collections::HashSet<_> =
    context.recipes.iter().map(|r| r.namepath.clone()).collect();

  for recipe in &response.recipes {
    if recipe.name.is_empty() {
      return Err("generated recipe name is empty".into());
    }

    if recipe.body.is_empty() {
      return Err("generated recipe body is empty".into());
    }

    if !recipe
      .name
      .chars()
      .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
    {
      return Err(
        format!(
          "recipe name `{}` contains unsupported characters",
          recipe.name
        )
        .into(),
      );
    }

    // Check source type matches reality
    let exists = existing_recipe_names.contains(&recipe.name);
    match recipe.source.as_str() {
      "existing" => {
        if !exists {
          return Err(
            format!(
              "recipe `{}` marked as existing but not found in project",
              recipe.name
            )
            .into(),
          );
        }
      }
      "modified" => {
        if !exists {
          return Err(
            format!(
              "recipe `{}` marked as modified but not found in project",
              recipe.name
            )
            .into(),
          );
        }
      }
      "new" => {
        if exists {
          return Err(
            format!(
              "recipe `{}` marked as new but already exists in project",
              recipe.name
            )
            .into(),
          );
        }
      }
      _ => {
        return Err(
          format!(
            "invalid source type `{}`, must be existing|new|modified",
            recipe.source
          )
          .into(),
        );
      }
    }

    for dependency in &recipe.dependencies {
      let in_workflow = response.recipes.iter().any(|r| r.name == *dependency);
      if !context.has_recipe(dependency) && !in_workflow {
        return Err(format!("dependency recipe `{dependency}` does not exist").into());
      }
    }
  }

  let source = context
    .root_source()
    .ok_or("project context does not contain a root justfile source")?;
  let original = bounded_file::read_utf8(source, max_editable_file_bytes())?;

  // Build a map of recipe name to rendered recipe
  let mut rendered_recipes: HashMap<String, String> = HashMap::new();
  for recipe in &response.recipes {
    let proposal = RecipeProposal {
      name: recipe.name.clone(),
      doc: recipe.doc.clone(),
      parameters: recipe.parameters.clone(),
      dependencies: recipe.dependencies.clone(),
      body: recipe.body.clone(),
    };
    rendered_recipes.insert(recipe.name.clone(), render_recipe(&proposal));
  }

  // Apply recipes in execution order
  let mut proposed = original.clone();
  for recipe_name in &response.execution_order {
    let recipe = rendered_recipes
      .get(recipe_name)
      .ok_or_else(|| format!("recipe `{recipe_name}` not found in workflow"))?;

    let recipe_proposal = response
      .recipes
      .iter()
      .find(|r| r.name == *recipe_name)
      .ok_or_else(|| format!("recipe proposal for `{recipe_name}` not found"))?;

    if recipe_proposal.source == "existing" {
      // Skip existing recipes - they're already in the justfile
      continue;
    }

    proposed = insert_recipe_grouped(
      &proposed,
      recipe,
      context,
      &recipe_proposal.dependencies,
      recipe_name,
    );
  }

  bounded_file::ensure_text_limit(&proposed, "proposed justfile", max_editable_file_bytes())?;
  validate_justfile(just_binary, source, &proposed)?;

  // Check risk for all recipes
  let mut all_risks = Vec::new();
  for recipe in &response.recipes {
    let risks = RiskFinding::scan_lines(&recipe.body);
    all_risks.extend(risks);
  }
  let risk = RiskLevel::highest(&all_risks);
  if risk == RiskLevel::Blocked {
    return Err("composed workflow has blocked risk and will not be written".into());
  }

  println!("{}", response.summary);
  println!();
  println!("Workflow request: {request}");
  println!(
    "Recipes: {}",
    response
      .recipes
      .iter()
      .map(|r| format!("{} ({})", r.name, r.source))
      .collect::<Vec<_>>()
      .join(", ")
  );
  println!("Execution order: {}", response.execution_order.join(" -> "));

  print_section("Rationale", &response.rationale);

  if !all_risks.is_empty() {
    println!();
    println!("Risk findings:");
    for finding in &all_risks {
      println!("  - {}: `{}`", finding.reason, finding.line);
    }
  }

  println!();
  println!("{}", unified_diff(source, &original, &proposed));

  if write {
    application::patches::apply_reviewed_change(source, &original, &proposed)?;
    println!("Wrote {}", source.display());
  } else {
    println!("Dry run only. Re-run with --write to apply this workflow.");
  }

  Ok(())
}
