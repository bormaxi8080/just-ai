use {
  crate::{
    ProjectContext,
    ai_responses::{AddRecipeResponse, FixProposal as AiFixProposal, FixResponse, RecipeProposal},
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

fn validate_fix_proposal(
  context: &ProjectContext,
  proposal: &AiFixProposal,
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

pub fn render_fix_recipe(proposal: &AiFixProposal) -> String {
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
