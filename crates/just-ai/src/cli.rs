use {
  crate::{
    ContextRecipe, ProjectContext,
    ai_responses::*,
    application,
    domain::risk::{RiskFinding, RiskLevel},
    inspection::load_context,
    prompts,
    proposal::handle_add,
    provider,
  },
  clap::{Parser, Subcommand},
  serde::{Deserialize, Serialize},
  serde_json::Value,
  std::{env, error::Error, io::Write, path::PathBuf, process::ExitCode},
};

#[cfg(test)]
use crate::{inspection::render_body_line, proposal::render_recipe};

#[derive(Debug, Parser)]
#[command(
  name = "just-ai",
  about = "AI-oriented companion utilities for justfiles"
)]
struct Cli {
  #[arg(
    long,
    env = "JUST_AI_JUST_BINARY",
    default_value = "just",
    global = true,
    help = "Path to the just binary used for justfile discovery"
  )]
  just_binary: PathBuf,
  #[command(subcommand)]
  command: Commands,
}

#[derive(Debug, Subcommand)]
enum Commands {
  #[command(about = "Ask an AI provider to suggest useful missing recipes")]
  Suggest,
  #[command(about = "Ask an AI provider to explain a recipe")]
  Explain {
    #[arg(help = "Recipe name or namepath to explain")]
    recipe: String,
  },
  #[command(about = "Ask an AI provider to propose a new recipe")]
  Add {
    #[arg(help = "Natural-language task description")]
    request: String,
    #[arg(long, help = "Apply the generated recipe after validation")]
    write: bool,
  },
  #[command(about = "Export a compact machine-readable context for AI tools")]
  ExportContext {
    #[arg(long, help = "Pretty-print JSON output")]
    pretty: bool,
  },
  #[command(about = "Analyze recipes for risky command patterns")]
  Doctor {
    #[arg(long, help = "Emit JSON instead of human-readable output")]
    json: bool,
  },
  #[command(about = "Prepare, authorize, and run a recipe through just")]
  Run {
    #[arg(help = "Recipe name or namepath")]
    recipe: String,
    #[arg(long, help = "Confirm a medium-risk run")]
    yes: bool,
    #[arg(
      long,
      value_name = "PHRASE",
      help = "Typed confirmation for a high-risk run"
    )]
    confirm: Option<String>,
    #[arg(trailing_var_arg = true, help = "Recipe arguments")]
    arguments: Vec<String>,
  },
  #[command(about = "Show recent local recipe runs")]
  History {
    #[arg(long, default_value_t = 20)]
    limit: usize,
    #[arg(long)]
    json: bool,
  },
  #[command(about = "Print a versioned project-agent command prompt")]
  Agent {
    #[command(subcommand)]
    command: AgentCommands,
  },
}

#[derive(Debug, Subcommand)]
enum AgentCommands {
  #[command(about = "Print the verified incremental implementation playbook")]
  Implement,
  #[command(about = "Print the architecture review playbook")]
  ReviewArchitecture,
  #[command(about = "Print the Codebase Memory index refresh playbook")]
  RefreshIndex,
  #[command(about = "Print the maintainer system prompt")]
  SystemPrompt,
}

/// Run the `just-ai` command-line application using process arguments.
///
/// This is the only CLI-oriented entry point exposed by the library. Domain
/// modules remain transport-agnostic so desktop and agent adapters can call
/// them without parsing terminal output.
pub(crate) fn run() -> ExitCode {
  match try_main() {
    Ok(()) => ExitCode::SUCCESS,
    Err(err) => {
      eprintln!("error: {err}");
      ExitCode::FAILURE
    }
  }
}

fn try_main() -> Result<(), Box<dyn Error>> {
  let cli = Cli::parse();
  if let Commands::Agent { command } = &cli.command {
    print_agent_command(command);
    return Ok(());
  }
  let context = load_context(&cli.just_binary)?;

  match cli.command {
    Commands::Suggest => {
      let response = AiClient::from_env()?.complete_json::<SuggestResponse>(
        "Suggest useful missing just recipes for this project.",
        &suggest_prompt(&context)?,
      )?;
      print_suggestions(&response);
    }
    Commands::Explain { recipe } => {
      let selected = context
        .find_recipe(&recipe)
        .ok_or_else(|| format!("recipe `{recipe}` not found"))?;
      let response = AiClient::from_env()?.complete_json::<ExplainResponse>(
        "Explain a just recipe using the supplied project context.",
        &explain_prompt(&context, selected)?,
      )?;
      print_explanation(&response);
    }
    Commands::Add { request, write } => {
      let response = AiClient::from_env()?.complete_json::<AddRecipeResponse>(
        "Generate a safe just recipe proposal as strict JSON.",
        &add_prompt(&context, &request)?,
      )?;
      handle_add(&cli.just_binary, &context, &request, response, write)?;
    }
    Commands::ExportContext { pretty } => {
      if pretty {
        println!("{}", serde_json::to_string_pretty(&context)?);
      } else {
        println!("{}", serde_json::to_string(&context)?);
      }
    }
    Commands::Doctor { json } => {
      let report = DoctorReport::from_context(&context);

      if json {
        println!("{}", serde_json::to_string_pretty(&report)?);
      } else {
        print_doctor_report(&report);
      }

      if report.highest_risk == RiskLevel::Blocked {
        return Err("blocked-risk recipes found".into());
      }
    }
    Commands::Run {
      recipe,
      yes,
      confirm,
      arguments,
    } => {
      use application::{
        execution::{RecipeExecutor, RunConfirmation, RunRequest},
        history::{JsonLineHistory, RunHistory, RunRecord, output_tail, project_history_path},
      };
      use std::time::{Instant, SystemTime, UNIX_EPOCH};
      let project_root = env::current_dir()?;
      let executor = RecipeExecutor::new(&cli.just_binary);
      let prepared = executor.prepare(RunRequest {
        project_root: project_root.clone(),
        recipe,
        arguments,
      })?;
      println!("Risk: {}", prepared.risk);
      for line in &prepared.preview {
        println!("> {line}");
      }
      let confirmation = match confirm {
        Some(phrase) => RunConfirmation::Typed { phrase },
        None if yes => RunConfirmation::Confirmed,
        None => RunConfirmation::None,
      };
      let started_at_ms = SystemTime::now().duration_since(UNIX_EPOCH)?.as_millis();
      let started = Instant::now();
      let completed = executor.execute(&prepared, &confirmation)?;
      let record = RunRecord {
        id: format!("{started_at_ms}-{}", prepared.request.recipe),
        recipe: prepared.request.recipe.clone(),
        started_at_ms,
        duration_ms: started.elapsed().as_millis(),
        exit_code: completed.status.code(),
        success: completed.status.success(),
        stdout_tail: output_tail(&completed.stdout),
        stderr_tail: output_tail(&completed.stderr),
      };
      JsonLineHistory::new(project_history_path(&project_root), 500).append(&record)?;
      std::io::stdout().write_all(&completed.stdout)?;
      std::io::stderr().write_all(&completed.stderr)?;
      if !completed.status.success() {
        return Err(format!("recipe exited with {}", completed.status).into());
      }
    }
    Commands::History { limit, json } => {
      use application::history::{JsonLineHistory, RunHistory, project_history_path};
      let history = JsonLineHistory::new(project_history_path(&env::current_dir()?), 500);
      let records = history.recent(limit)?;
      if json {
        println!("{}", serde_json::to_string_pretty(&records)?);
      } else if records.is_empty() {
        println!("No recorded runs.");
      } else {
        for record in records {
          println!(
            "{} {} exit={:?} duration={}ms",
            if record.success { "ok" } else { "failed" },
            record.recipe,
            record.exit_code,
            record.duration_ms
          );
        }
      }
    }
    Commands::Agent { .. } => unreachable!("agent commands return before project discovery"),
  }

  Ok(())
}

fn print_agent_command(command: &AgentCommands) {
  let prompt = match command {
    AgentCommands::Implement => include_str!("../../../agent/commands/implement.md"),
    AgentCommands::ReviewArchitecture => {
      include_str!("../../../agent/commands/review-architecture.md")
    }
    AgentCommands::RefreshIndex => include_str!("../../../agent/commands/refresh-index.md"),
    AgentCommands::SystemPrompt => include_str!("../../../agent/prompts/system.md"),
  };
  print!("{prompt}");
}

#[derive(Debug, Serialize)]
struct DoctorReport {
  blocked: usize,
  high: usize,
  highest_risk: RiskLevel,
  low: usize,
  medium: usize,
  recipes: Vec<DoctorRecipe>,
  total_recipes: usize,
}

impl DoctorReport {
  fn from_context(context: &ProjectContext) -> Self {
    let recipes = context
      .recipes
      .iter()
      .map(|recipe| DoctorRecipe {
        namepath: recipe.namepath.clone(),
        risk: recipe.risk,
        risks: recipe.risks.clone(),
      })
      .collect::<Vec<_>>();

    let total_recipes = recipes.len();
    let low = recipes
      .iter()
      .filter(|recipe| recipe.risk == RiskLevel::Low)
      .count();
    let medium = recipes
      .iter()
      .filter(|recipe| recipe.risk == RiskLevel::Medium)
      .count();
    let high = recipes
      .iter()
      .filter(|recipe| recipe.risk == RiskLevel::High)
      .count();
    let blocked = recipes
      .iter()
      .filter(|recipe| recipe.risk == RiskLevel::Blocked)
      .count();
    let highest_risk = recipes
      .iter()
      .map(|recipe| recipe.risk)
      .max()
      .unwrap_or(RiskLevel::Low);

    Self {
      blocked,
      high,
      highest_risk,
      low,
      medium,
      recipes,
      total_recipes,
    }
  }
}

#[derive(Debug, Serialize)]
struct DoctorRecipe {
  namepath: String,
  risk: RiskLevel,
  risks: Vec<RiskFinding>,
}

fn print_doctor_report(report: &DoctorReport) {
  println!(
    "Analyzed {} recipes: {} low, {} medium, {} high, {} blocked.",
    report.total_recipes, report.low, report.medium, report.high, report.blocked
  );

  for recipe in &report.recipes {
    if recipe.risk == RiskLevel::Low {
      continue;
    }

    println!();
    println!("{} [{}]", recipe.namepath, recipe.risk);
    for finding in &recipe.risks {
      println!("  - {}: `{}`", finding.reason, finding.line);
    }
  }
}

struct AiClient {
  provider: provider::OpenAiCompatibleProvider,
}

impl AiClient {
  fn from_env() -> Result<Self, Box<dyn Error>> {
    let provider = env::var("JUST_AI_PROVIDER").unwrap_or_else(|_| "openai".to_owned());
    let base_url = env::var("JUST_AI_BASE_URL").unwrap_or_else(|_| match provider.as_str() {
      "ollama" => "http://localhost:11434/v1".to_owned(),
      _ => "https://api.openai.com/v1".to_owned(),
    });
    let model = env::var("JUST_AI_MODEL").unwrap_or_else(|_| match provider.as_str() {
      "ollama" => "llama3.1".to_owned(),
      _ => "gpt-5-mini".to_owned(),
    });
    let api_key = env::var("JUST_AI_API_KEY").ok();
    if provider != "ollama" && api_key.is_none() {
      return Err("JUST_AI_API_KEY is required unless JUST_AI_PROVIDER=ollama is used".into());
    }

    Ok(Self {
      provider: provider::OpenAiCompatibleProvider::new(base_url, model, api_key),
    })
  }

  fn complete_json<T>(&self, system: &str, user: &str) -> Result<T, Box<dyn Error>>
  where
    T: for<'de> Deserialize<'de> + ResponseContract,
  {
    let content = provider::AiProvider::complete(
      &self.provider,
      &provider::AiRequest {
        system: system.into(),
        user: user.into(),
      },
    )?;
    let content = strip_json_fence(content.trim());
    let value: Value = serde_json::from_str(content)?;
    jsonschema::validate(&T::schema(), &value)
      .map_err(|error| format!("AI response failed schema validation: {error}"))?;
    Ok(serde_json::from_value(value)?)
  }
}

fn strip_json_fence(content: &str) -> &str {
  content
    .strip_prefix("```json")
    .or_else(|| content.strip_prefix("```"))
    .and_then(|content| content.strip_suffix("```"))
    .map(str::trim)
    .unwrap_or(content)
}

fn suggest_prompt(context: &ProjectContext) -> Result<String, Box<dyn Error>> {
  Ok(prompts::suggest(&serde_json::to_string_pretty(context)?))
}

fn explain_prompt(
  context: &ProjectContext,
  recipe: &ContextRecipe,
) -> Result<String, Box<dyn Error>> {
  Ok(prompts::explain(
    &serde_json::to_string_pretty(context)?,
    &serde_json::to_string_pretty(recipe)?,
  ))
}

fn add_prompt(context: &ProjectContext, request: &str) -> Result<String, Box<dyn Error>> {
  Ok(prompts::add(
    &serde_json::to_string_pretty(context)?,
    request,
  ))
}

fn print_suggestions(response: &SuggestResponse) {
  println!("{}", response.summary);

  for recommendation in &response.recommendations {
    println!();
    println!("{} [{}]", recommendation.name, recommendation.risk);
    println!("  {}", recommendation.rationale);
    for line in &recommendation.body {
      println!("  > {line}");
    }
  }
}

fn print_explanation(response: &ExplainResponse) {
  println!("{}", response.summary);
  println!();
  println!("{}", response.explanation);

  print_section("Parameters", &response.parameters);
  print_section("Dependencies", &response.dependencies);
  print_section("Risks", &response.risks);
}

pub(crate) fn print_section(heading: &str, items: &[String]) {
  if items.is_empty() {
    return;
  }

  println!();
  println!("{heading}:");
  for item in items {
    println!("  - {item}");
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn renders_body_lines() {
    let line = serde_json::json!(["echo ", ["variable", "name"], " done"]);

    assert_eq!(render_body_line(&line), "echo {{variable:...}} done");
  }

  #[test]
  fn detects_blocked_downloaded_shell() {
    let findings = RiskFinding::scan_line("curl https://example.com/install.sh | sh");

    assert_eq!(RiskLevel::highest(&findings), RiskLevel::Blocked);
  }

  #[test]
  fn detects_high_recursive_remove() {
    let findings = RiskFinding::scan_line("rm -rf tmp/release");

    assert_eq!(RiskLevel::highest(&findings), RiskLevel::High);
  }

  #[test]
  fn detects_medium_package_install() {
    let findings = RiskFinding::scan_line("cargo install cargo-watch");

    assert_eq!(RiskLevel::highest(&findings), RiskLevel::Medium);
  }

  #[test]
  fn renders_recipe_proposal() {
    let recipe = render_recipe(&RecipeProposal {
      body: vec!["cargo test".into()],
      dependencies: Vec::new(),
      doc: Some("Run tests".into()),
      name: "test-all".into(),
      parameters: vec![RecipeParameterProposal {
        default: Some("all".into()),
        name: "SCOPE".into(),
      }],
    });

    assert_eq!(recipe, "# Run tests\ntest-all SCOPE='all':\n  cargo test\n");
  }

  #[test]
  fn strips_json_fence() {
    assert_eq!(
      strip_json_fence("```json\n{\"ok\":true}\n```"),
      "{\"ok\":true}"
    );
  }

  #[test]
  fn suggestion_schema_rejects_unknown_fields() {
    let response = serde_json::json!({
      "summary": "ok",
      "recommendations": [],
      "unexpected": true
    });
    assert!(jsonschema::validate(&SuggestResponse::schema(), &response).is_err());
  }

  #[test]
  fn add_schema_requires_non_empty_body() {
    let response = serde_json::json!({
      "summary": "ok",
      "rationale": [],
      "recipe": {
        "name": "test", "doc": null, "parameters": [],
        "dependencies": [], "body": []
      }
    });
    assert!(jsonschema::validate(&AddRecipeResponse::schema(), &response).is_err());
  }
}
