use {
  crate::{
    ProjectContext,
    ai_responses::{AddRecipeResponse, RecipeProposal},
    application, bounded_output,
    cli::print_section,
    domain::risk::{RiskFinding, RiskLevel},
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

pub(crate) fn handle_add(
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
  let original = fs::read_to_string(source)?;
  let recipe = render_recipe(&response.recipe);
  let proposed = append_recipe(&original, &recipe);
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

pub(crate) fn render_recipe(proposal: &RecipeProposal) -> String {
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

fn append_recipe(original: &str, recipe: &str) -> String {
  let mut proposed = original.to_owned();

  if !proposed.ends_with('\n') {
    proposed.push('\n');
  }

  proposed.push('\n');
  proposed.push_str(recipe);
  proposed
}

fn validate_justfile(
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

fn unified_diff(path: &Path, original: &str, proposed: &str) -> String {
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
