use {
  crate::{
    ContextParameter, ContextRecipe, ProjectContext,
    ai_responses::*,
    application,
    config::{HistoryBackend, HistoryConfig},
    domain::risk::{RiskFinding, RiskLevel},
    inspection::load_context,
    prompts,
    proposal::{
      handle_add, handle_compose_workflow, handle_fix, handle_instantiate_template,
      handle_template, handle_workflow, validate_fix_proposal,
    },
    provider,
  },
  clap::{Parser, Subcommand},
  serde::{Deserialize, Serialize},
  serde_json::Value,
  std::{
    collections::HashMap,
    env,
    error::Error,
    fs,
    io::Write,
    path::{Path, PathBuf},
    process::ExitCode,
  },
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
  #[command(about = "Ask an AI provider to explain a recipe, or all recipes with --all")]
  Explain {
    #[arg(help = "Recipe name or namepath to explain (optional with --all)")]
    recipe: Option<String>,
    #[arg(long, help = "Explain all recipes in the project")]
    all: bool,
  },
  #[command(about = "Ask an AI provider to propose a new recipe")]
  Add {
    #[arg(help = "Natural-language task description")]
    request: String,
    #[arg(long, help = "Apply the generated recipe after validation")]
    write: bool,
  },
  #[command(
    about = "Ask an AI provider to propose a fix for a failed recipe, or all failed with --all-failed"
  )]
  Fix {
    #[arg(help = "Recipe name or namepath to fix (optional with --all-failed)")]
    recipe: Option<String>,
    #[arg(long, help = "Apply the generated fix after validation")]
    write: bool,
    #[arg(long, help = "Fix all recipes that have failed runs in history")]
    all_failed: bool,
  },
  #[command(about = "Ask an AI provider to propose a multi-recipe workflow")]
  Workflow {
    #[arg(help = "Natural-language workflow description")]
    request: String,
    #[arg(long, help = "Apply the generated workflow after validation")]
    write: bool,
  },
  #[command(about = "Ask an AI provider to create a reusable recipe template")]
  Template {
    #[arg(help = "Natural-language template description")]
    request: String,
  },
  #[command(about = "Instantiate a template with provided parameter values")]
  InstantiateTemplate {
    #[arg(help = "Template name to instantiate")]
    template: String,
    #[arg(help = "Parameter values as KEY=VALUE pairs")]
    values: Vec<String>,
    #[arg(long, help = "Apply the instantiated recipe after validation")]
    write: bool,
  },
  #[command(about = "Compose a workflow by reusing and adapting existing recipes")]
  ComposeWorkflow {
    #[arg(help = "Natural-language workflow description")]
    request: String,
    #[arg(long, help = "Apply the generated workflow after validation")]
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
    #[arg(long, help = "Use interactive TUI prompts for confirmation")]
    interactive: bool,
    #[arg(trailing_var_arg = true, help = "Recipe arguments")]
    arguments: Vec<String>,
  },
  #[command(about = "Manage local recipe run history")]
  History {
    #[command(subcommand)]
    command: HistoryCommands,
  },
  #[command(about = "Print a versioned project-agent command prompt")]
  Agent {
    #[command(subcommand)]
    command: AgentCommands,
  },
  #[command(about = "Validate or generate schema for just-ai configuration")]
  Config {
    #[command(subcommand)]
    command: ConfigCommands,
  },
  #[command(about = "Analyze and refactor justfile structure")]
  Migrate {
    #[command(subcommand)]
    command: MigrateCommands,
  },
}

#[derive(Debug, Subcommand)]
enum HistoryCommands {
  #[command(about = "Show recent local recipe runs")]
  Recent {
    #[arg(long, default_value_t = 20)]
    limit: usize,
    #[arg(long)]
    json: bool,
  },
  #[command(about = "Migrate history from JSONL to SQLite")]
  Migrate,
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
  #[command(about = "Print the layered verification playbook")]
  Verify,
}

#[derive(Debug, Subcommand)]
enum ConfigCommands {
  #[command(about = "Validate the just-ai.toml configuration file")]
  Validate,
  #[command(about = "Output JSON Schema for just-ai.toml")]
  Schema,
}

#[derive(Debug, Subcommand)]
enum MigrateCommands {
  #[command(about = "Analyze project for dead code, cycles, and structural issues")]
  Analyze {
    #[arg(long, help = "Emit JSON instead of human-readable output")]
    json: bool,
    #[arg(
      long,
      help = "Similarity threshold for duplicate detection (0.0-1.0)",
      default_value = "0.8"
    )]
    similarity_threshold: f64,
  },
  #[command(about = "Automatically organize recipes into modules based on dependencies")]
  Modularize {
    #[arg(long, help = "Apply changes to justfile")]
    write: bool,
    #[arg(long, help = "Dry run - show what would be changed")]
    dry_run: bool,
  },
  #[command(about = "Find and optionally merge duplicate/similar recipes")]
  Deduplicate {
    #[arg(long, help = "Apply changes to justfile")]
    write: bool,
    #[arg(
      long,
      help = "Similarity threshold for duplicate detection (0.0-1.0)",
      default_value = "0.8"
    )]
    similarity_threshold: f64,
    #[arg(long, help = "Interactive mode - confirm each merge")]
    interactive: bool,
    #[arg(long, help = "Smart merge similar recipes instead of removing one")]
    merge: bool,
  },
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
    Commands::Explain { recipe, all } => {
      if all {
        // Batch explain all recipes
        let recipes = &context.recipes;
        if recipes.is_empty() {
          println!("No recipes to explain.");
        } else {
          for recipe_ctx in recipes {
            let response = AiClient::from_env()?.complete_json::<ExplainResponse>(
              "Explain a just recipe using the supplied project context.",
              &explain_prompt(&context, recipe_ctx)?,
            )?;
            println!();
            println!("=== {} ===", recipe_ctx.namepath);
            print_explanation(&response);
          }
        }
      } else {
        let recipe = recipe.ok_or("recipe name required (or use --all)")?;
        let selected = context
          .find_recipe(&recipe)
          .ok_or_else(|| format!("recipe `{recipe}` not found"))?;
        let response = AiClient::from_env()?.complete_json::<ExplainResponse>(
          "Explain a just recipe using the supplied project context.",
          &explain_prompt(&context, selected)?,
        )?;
        print_explanation(&response);
      }
    }
    Commands::Add { request, write } => {
      let response = AiClient::from_env()?.complete_json::<AddRecipeResponse>(
        "Generate a safe just recipe proposal as strict JSON.",
        &add_prompt(&context, &request)?,
      )?;
      handle_add(&cli.just_binary, &context, &request, response, write)?;
    }
    Commands::Fix {
      recipe,
      write,
      all_failed,
    } => {
      if all_failed {
        // Batch fix all failed recipes
        use crate::config::Config;
        use application::history::create_history;
        let project_root = env::current_dir()?;
        let config = Config::load(&project_root)?;
        let history = create_history(config.history)?;
        // Query for ALL failed runs
        let failed_runs = history.query(None, Some(false), 100)?;

        // Group by recipe name to get unique failed recipes
        let mut failed_recipes: Vec<&String> = failed_runs
          .iter()
          .map(|r| &r.recipe)
          .collect::<std::collections::HashSet<_>>()
          .into_iter()
          .collect();
        failed_recipes.sort();

        if failed_recipes.is_empty() {
          println!("No failed recipes found in history.");
          return Ok(());
        }

        println!("Found {} unique failed recipes:", failed_recipes.len());
        for r in &failed_recipes {
          println!("  - {}", r);
        }
        println!();

        let source = context
          .root_source()
          .ok_or("project context does not contain a root justfile source")?;
        let original =
          crate::bounded_file::read_utf8(source, crate::bounded_file::max_editable_file_bytes())?;

        // We'll collect all fixes and apply them at once
        let mut proposed = original.clone();
        let mut any_written = false;

        for recipe_name in &failed_recipes {
          // Get history for this specific recipe
          let recipe_history = history.query(Some(recipe_name), Some(false), 10)?;
          let history_json = serde_json::to_string_pretty(&recipe_history)?;
          let context_json = serde_json::to_string_pretty(&context)?;

          let response = AiClient::from_env()?.complete_json::<FixResponse>(
            "Generate a fix proposal for a failing just recipe as strict JSON.",
            &prompts::fix(&context_json, recipe_name, &history_json),
          )?;

          // Apply the fix to the proposed content
          validate_fix_proposal(&context, &response.recipe, recipe_name)?;

          let recipe_rendered = crate::proposal::render_fix_recipe(&response.recipe);
          proposed = crate::proposal::replace_recipe(&proposed, recipe_name, &recipe_rendered);

          crate::bounded_file::ensure_text_limit(
            &proposed,
            "proposed justfile",
            crate::bounded_file::max_editable_file_bytes(),
          )?;

          let risks = crate::domain::risk::RiskFinding::scan_lines(&response.recipe.body);
          let risk = crate::domain::risk::RiskLevel::highest(&risks);
          if risk == crate::domain::risk::RiskLevel::Blocked {
            return Err(
              format!(
                "generated fix for `{}` has blocked risk and will not be written",
                recipe_name
              )
              .into(),
            );
          }

          println!("Fixed: {} [{}]", response.recipe.name, risk);
          print_section("Rationale", &response.rationale);
          println!();
        }

        // Final validation
        crate::proposal::validate_justfile(&cli.just_binary, source, &proposed)?;

        println!(
          "{}",
          crate::proposal::unified_diff(source, &original, &proposed)
        );

        if write {
          crate::application::patches::apply_reviewed_change(source, &original, &proposed)?;
          println!("Wrote {}", source.display());
          any_written = true;
        } else {
          println!("Dry run only. Re-run with --write to apply all fixes.");
        }

        if !any_written {
          println!("No changes applied (dry run).");
        }
      } else {
        let recipe = recipe.ok_or("recipe name required (or use --all-failed)")?;
        use crate::config::Config;
        use application::history::create_history;
        let project_root = env::current_dir()?;
        let config = Config::load(&project_root)?;
        let history = create_history(config.history)?;
        // Query for failed runs of this recipe
        let failed_runs = history.query(Some(&recipe), Some(false), 10)?;
        let history_json = serde_json::to_string_pretty(&failed_runs)?;
        let context_json = serde_json::to_string_pretty(&context)?;
        let response = AiClient::from_env()?.complete_json::<FixResponse>(
          "Generate a fix proposal for a failing just recipe as strict JSON.",
          &prompts::fix(&context_json, &recipe, &history_json),
        )?;
        handle_fix(&cli.just_binary, &context, &recipe, response, write)?;
      }
    }
    Commands::Workflow { request, write } => {
      let response = AiClient::from_env()?.complete_json::<WorkflowResponse>(
        "Generate a multi-recipe workflow as strict JSON.",
        &prompts::workflow(&serde_json::to_string_pretty(&context)?, &request),
      )?;
      handle_workflow(&cli.just_binary, &context, &request, response, write)?;
    }
    Commands::ComposeWorkflow { request, write } => {
      let response = AiClient::from_env()?.complete_json::<ComposeWorkflowResponse>(
        "Compose a multi-recipe workflow by reusing existing recipes as strict JSON.",
        &prompts::compose_workflow(&serde_json::to_string_pretty(&context)?, &request),
      )?;
      handle_compose_workflow(&cli.just_binary, &context, &request, response, write)?;
    }
    Commands::Template { request } => {
      let response = AiClient::from_env()?.complete_json::<TemplateResponse>(
        "Generate a reusable just recipe template as strict JSON.",
        &prompts::template(&serde_json::to_string_pretty(&context)?, &request),
      )?;
      handle_template(&context, &request, response)?;
    }
    Commands::InstantiateTemplate {
      template,
      values,
      write,
    } => {
      // Get the template from the user (in practice this would be stored)
      // For now, we ask AI to generate the template and then instantiate it
      let template_prompt = format!(
        "Find or create a template named '{}' for this project.",
        template
      );
      let template_response = AiClient::from_env()?.complete_json::<TemplateResponse>(
        "Generate a reusable just recipe template as strict JSON.",
        &prompts::template(&serde_json::to_string_pretty(&context)?, &template_prompt),
      )?;

      // Parse the provided values
      let mut values_map = std::collections::HashMap::new();
      for value in &values {
        let parts: Vec<&str> = value.splitn(2, '=').collect();
        if parts.len() != 2 {
          return Err(format!("invalid parameter format: '{}', expected KEY=VALUE", value).into());
        }
        values_map.insert(parts[0].to_string(), parts[1].to_string());
      }

      // Check required parameters
      for param in &template_response.template.parameters {
        if param.required && !values_map.contains_key(&param.name) {
          if let Some(default) = &param.default {
            values_map.insert(param.name.clone(), default.clone());
          } else {
            return Err(format!("required parameter '{}' not provided", param.name).into());
          }
        }
      }

      // Instantiate the template
      handle_instantiate_template(
        &cli.just_binary,
        &context,
        &template_response.template,
        &values_map,
        write,
      )?;
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
      interactive,
      arguments,
    } => {
      use crate::config::Config;
      use application::{
        execution::{RecipeExecutor, RunConfirmation, RunRequest, interactive_authorize},
        history::{RunRecord, create_history},
      };
      use std::time::{Instant, SystemTime, UNIX_EPOCH};
      let project_root = env::current_dir()?;
      let config = Config::load(&project_root)?;
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
      let confirmation = if interactive {
        interactive_authorize(&prepared.policy)?
      } else {
        match confirm {
          Some(phrase) => RunConfirmation::Typed { phrase },
          None if yes => RunConfirmation::Confirmed,
          None => RunConfirmation::None,
        }
      };
      let started_at_ms = SystemTime::now().duration_since(UNIX_EPOCH)?.as_millis();
      let started = Instant::now();
      let completed = executor.execute(&prepared, &confirmation)?;
      let record = RunRecord::completed(
        &prepared.request,
        started_at_ms,
        started.elapsed().as_millis(),
        &completed,
        &config.history,
      );
      let history = create_history(config.history)?;
      history.append(&record)?;
      std::io::stdout().write_all(&completed.stdout)?;
      std::io::stderr().write_all(&completed.stderr)?;
      if !completed.status.success() {
        return Err(format!("recipe exited with {}", completed.status).into());
      }
    }
    Commands::History { command } => match command {
      HistoryCommands::Recent { limit, json } => {
        use crate::config::Config;
        use application::history::create_history;
        let project_root = env::current_dir()?;
        let config = Config::load(&project_root)?;
        let history = create_history(config.history)?;
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
      HistoryCommands::Migrate => {
        use crate::application::history::migrate_jsonl_to_sqlite;
        use crate::config::Config;
        let project_root = env::current_dir()?;
        let config = Config::load(&project_root)?;
        let jsonl_config = HistoryConfig {
          backend: HistoryBackend::Jsonl,
          ..config.history.clone()
        };
        let sqlite_config = HistoryConfig {
          backend: HistoryBackend::Sqlite,
          ..config.history
        };
        migrate_jsonl_to_sqlite(jsonl_config, sqlite_config)?;
      }
    },
    Commands::Config { command } => match command {
      ConfigCommands::Validate => {
        use crate::config::Config;
        let project_root = env::current_dir()?;
        Config::load(&project_root)?;
        println!("Configuration is valid.");
      }
      ConfigCommands::Schema => {
        use crate::config::Config;
        use schemars::schema_for;
        let schema = schema_for!(Config);
        println!("{}", serde_json::to_string_pretty(&schema)?);
      }
    },
    Commands::Migrate { command } => match command {
      MigrateCommands::Analyze {
        json,
        similarity_threshold,
      } => {
        analyze_project(&context, json, similarity_threshold)?;
      }
      MigrateCommands::Modularize { write, dry_run } => {
        modularize_project(&cli.just_binary, &context, write, dry_run)?;
      }
      MigrateCommands::Deduplicate {
        write,
        similarity_threshold,
        interactive,
        merge,
      } => {
        deduplicate_project(
          &cli.just_binary,
          &context,
          write,
          similarity_threshold,
          interactive,
          merge,
        )?;
      }
    },
    Commands::Agent { .. } => unreachable!("agent commands return before project discovery"),
  }

  Ok(())
}

fn analyze_project(
  context: &ProjectContext,
  json: bool,
  similarity_threshold: f64,
) -> Result<(), Box<dyn Error>> {
  let unreferenced = context.find_unreferenced_recipes();
  let isolated = context.find_isolated_recipes();
  let cycles = context.detect_cycles();
  let depths = context.calculate_dependency_depths();
  let similar = context.find_similar_recipes(similarity_threshold);

  if json {
    let report = serde_json::json!({
      "unreferenced_recipes": unreferenced.iter().map(|r| r.namepath.clone()).collect::<Vec<_>>(),
      "isolated_recipes": isolated.iter().map(|r| r.namepath.clone()).collect::<Vec<_>>(),
      "cycles": cycles,
      "dependency_depths": depths,
      "similar_recipes": similar.iter().map(|(a, b, s)| {
        serde_json::json!({"recipe1": a, "recipe2": b, "similarity": s})
      }).collect::<Vec<_>>(),
    });
    println!("{}", serde_json::to_string_pretty(&report)?);
  } else {
    println!("=== Project Analysis ===");
    println!("Total recipes: {}", context.recipes.len());
    println!("Modules: {}", context.modules.len());
    println!();

    if !unreferenced.is_empty() {
      println!("Unreferenced recipes (no other recipe depends on them):");
      for recipe in &unreferenced {
        println!("  - {} [{}]", recipe.namepath, recipe.module_path);
      }
      println!();
    } else {
      println!("No unreferenced recipes found.");
      println!();
    }

    if !isolated.is_empty() {
      println!("Isolated recipes (no dependencies, no dependents):");
      for recipe in &isolated {
        println!("  - {} [{}]", recipe.namepath, recipe.module_path);
      }
      println!();
    } else {
      println!("No isolated recipes found.");
      println!();
    }

    if !cycles.is_empty() {
      println!("⚠️  Dependency cycles detected:");
      for (i, cycle) in cycles.iter().enumerate() {
        println!("  Cycle {}: {}", i + 1, cycle.join(" -> "));
      }
      println!();
    } else {
      println!("No dependency cycles detected.");
      println!();
    }

    println!("Dependency depths:");
    let mut depth_vec: Vec<_> = depths.iter().collect();
    depth_vec.sort_by(|a, b| b.1.cmp(a.1));
    for (recipe, depth) in depth_vec.iter().take(20) {
      println!("  {}: depth {}", recipe, depth);
    }
    if depth_vec.len() > 20 {
      println!("  ... and {} more", depth_vec.len() - 20);
    }
    println!();

    if !similar.is_empty() {
      println!(
        "Similar recipes (potential duplicates, threshold {:.0}%):",
        similarity_threshold * 100.0
      );
      for (a, b, sim) in &similar {
        println!("  {:.1}%: {} ~ {}", sim * 100.0, a, b);
      }
      println!();
    } else {
      println!(
        "No similar recipes found at threshold {:.0}%.",
        similarity_threshold * 100.0
      );
      println!();
    }
  }

  Ok(())
}

fn modularize_project(
  just_binary: &Path,
  context: &ProjectContext,
  write: bool,
  dry_run: bool,
) -> Result<(), Box<dyn Error>> {
  use crate::bounded_file::{max_editable_file_bytes, read_utf8};
  use crate::proposal::{unified_diff, validate_justfile};

  // Group recipes by common prefix (e.g., "test-", "build-", "deploy-")
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

  let source = context
    .root_source()
    .ok_or("project context does not contain a root justfile source")?;
  let original = read_utf8(source, max_editable_file_bytes())?;
  let mut proposed = original.clone();

  let source_dir = source.parent().unwrap_or_else(|| Path::new("."));
  let mut import_statements = Vec::new();

  println!("=== Modularization Plan ===");
  for (prefix, recipes) in &groups {
    if recipes.len() < 2 {
      continue; // Skip single recipes
    }
    println!("Module '{}': {} recipes", prefix, recipes.len());
    for recipe in recipes {
      println!("  - {}", recipe.namepath);
    }
  }
  println!();

  // For each group, create a module file and move recipes
  for (prefix, recipes) in &groups {
    if recipes.len() < 2 {
      continue;
    }

    let module_filename = format!("{prefix}.just");
    let _module_path = source_dir.join(&module_filename);

    // Extract recipes for this module
    let mut module_content = String::new();
    for recipe in recipes {
      // Find the recipe in the original content
      let recipe_text = extract_recipe(&original, &recipe.name);
      if !recipe_text.is_empty() {
        if !module_content.is_empty() {
          module_content.push('\n');
        }
        module_content.push_str(&recipe_text);
      }
    }

    if module_content.is_empty() {
      continue;
    }

    // Remove recipes from proposed content
    for recipe in recipes {
      proposed = crate::proposal::replace_recipe(&proposed, &recipe.name, "");
    }

    // Add import statement
    import_statements.push(format!("import '{}'", module_filename));
  }

  // Add import statements at the top of the file (after any existing imports)
  if !import_statements.is_empty() {
    proposed = add_imports_at_top(&proposed, &import_statements);
  }

  println!("{}", unified_diff(source, &original, &proposed));

  if dry_run || !write {
    println!("Dry run only. Re-run with --write to apply changes.");
    return Ok(());
  }

  // Write module files FIRST so validation can find them
  for (prefix, recipes) in &groups {
    if recipes.len() < 2 {
      continue;
    }
    let module_filename = format!("{prefix}.just");
    let module_path = source_dir.join(&module_filename);
    let mut module_content = String::new();
    for recipe in recipes {
      let recipe_text = extract_recipe(&original, &recipe.name);
      if !recipe_text.is_empty() {
        if !module_content.is_empty() {
          module_content.push('\n');
        }
        module_content.push_str(&recipe_text);
      }
    }
    if !module_content.is_empty() {
      fs::write(&module_path, module_content)?;
      println!("Created {}", module_path.display());
    }
  }

  // Final validation (now module files exist)
  validate_justfile(just_binary, source, &proposed)?;

  // Write modified root justfile
  application::patches::apply_reviewed_change(source, &original, &proposed)?;
  println!("Wrote {}", source.display());

  Ok(())
}

fn extract_recipe(content: &str, recipe_name: &str) -> String {
  let lines: Vec<&str> = content.lines().collect();
  let mut result = Vec::new();
  let mut i = 0;
  let mut found = false;

  while i < lines.len() {
    let line = lines[i];
    let trimmed = line.trim_start();
    let is_recipe_def = trimmed.starts_with(&format!("{recipe_name} "))
      || trimmed == recipe_name
      || trimmed.starts_with(&format!("{recipe_name}:"));
    if !found && is_recipe_def {
      found = true;
      // Include the recipe definition and its body (indented lines)
      result.push(line);
      i += 1;
      while i < lines.len()
        && (lines[i].starts_with(' ') || lines[i].starts_with('\t') || lines[i].trim().is_empty())
      {
        result.push(lines[i]);
        i += 1;
      }
      continue;
    }
    i += 1;
  }

  result.join("\n").trim_end().to_string()
}

fn add_imports_at_top(content: &str, imports: &[String]) -> String {
  let lines: Vec<&str> = content.lines().collect();
  let mut result: Vec<String> = Vec::new();
  let mut import_added = false;
  let mut last_import_idx = None;

  // First pass: find the last import line
  for (i, line) in lines.iter().enumerate() {
    let trimmed = line.trim_start();
    if trimmed.starts_with("import ") {
      last_import_idx = Some(i);
    }
  }

  for (i, line) in lines.iter().enumerate() {
    result.push(line.to_string());
    // If this is the last import line, add our imports after it
    if last_import_idx == Some(i) && !import_added {
      if !result.last().unwrap().trim().is_empty() {
        result.push(String::new());
      }
      for import in imports {
        result.push(import.clone());
      }
      import_added = true;
    }
  }

  // If no imports were found, add at the very beginning
  if !import_added {
    let mut new_result = Vec::new();
    for import in imports {
      new_result.push(import.clone());
    }
    new_result.push(String::new());
    new_result.extend(result);
    return new_result.join("\n");
  }

  result.join("\n")
}

fn deduplicate_project(
  just_binary: &Path,
  context: &ProjectContext,
  write: bool,
  similarity_threshold: f64,
  interactive: bool,
  merge: bool,
) -> Result<(), Box<dyn Error>> {
  use crate::bounded_file::{max_editable_file_bytes, read_utf8};
  use crate::proposal::{replace_recipe, unified_diff, validate_justfile};

  let similar = context.find_similar_recipes(similarity_threshold);

  if similar.is_empty() {
    println!(
      "No similar recipes found at threshold {:.0}%.",
      similarity_threshold * 100.0
    );
    return Ok(());
  }

  println!("=== Duplicate Analysis ===");
  println!("Found {} similar recipe pairs:", similar.len());
  println!();

  let source = context
    .root_source()
    .ok_or("project context does not contain a root justfile source")?;
  let original = read_utf8(source, max_editable_file_bytes())?;
  let mut proposed = original.clone();

  for (a, b, sim) in &similar {
    println!("{:.1}% similar:", sim * 100.0);
    println!("  1. {}", a);
    println!("  2. {}", b);

    let recipe_a = context.find_recipe(a);
    let recipe_b = context.find_recipe(b);

    if interactive {
      print!("Merge? [1=keep first, 2=keep second, m=smart merge, s=skip] ");
      std::io::stdout().flush()?;
      let mut input = String::new();
      std::io::stdin().read_line(&mut input)?;

      match input.trim() {
        "1" => {
          // Keep a, remove b
          if let Some(recipe_b) = recipe_b {
            proposed = replace_recipe(&proposed, &recipe_b.name, "");
            println!("  Marked '{}' for removal", b);
          }
        }
        "2" => {
          // Keep b, remove a
          if let Some(recipe_a) = recipe_a {
            proposed = replace_recipe(&proposed, &recipe_a.name, "");
            println!("  Marked '{}' for removal", a);
          }
        }
        "m" => {
          // Smart merge - try to combine the best of both recipes
          if let (Some(ra), Some(rb)) = (recipe_a, recipe_b) {
            let merged = smart_merge_recipes(ra, rb);
            proposed = replace_recipe(&proposed, &ra.name, "");
            proposed = replace_recipe(&proposed, &rb.name, &merged);
            println!("  Smart merged into '{}'", ra.name);
          }
        }
        _ => {
          println!("  Skipped");
        }
      }
    } else if write {
      if merge {
        // Smart auto-merge: combine similar recipes
        if let (Some(ra), Some(rb)) = (recipe_a, recipe_b) {
          let merged = smart_merge_recipes(ra, rb);
          proposed = replace_recipe(&proposed, &ra.name, "");
          proposed = replace_recipe(&proposed, &rb.name, &merged);
          println!("  Smart auto-merged into '{}'", ra.name);
        }
      } else {
        // Non-interactive: keep the one with shorter namepath (more generic)
        let keep = if a.len() <= b.len() { a } else { b };
        let remove = if keep == a { b } else { a };

        if let Some(recipe) = context.find_recipe(remove) {
          proposed = replace_recipe(&proposed, &recipe.name, "");
          println!("  Auto-merged: kept '{}', removed '{}'", keep, remove);
        }
      }
    }
    println!();
  }

  if write || interactive {
    validate_justfile(just_binary, source, &proposed)?;

    println!("{}", unified_diff(source, &original, &proposed));

    if write {
      use crate::application::patches::apply_reviewed_change;
      apply_reviewed_change(source, &original, &proposed)?;
      println!("Wrote {}", source.display());
    } else {
      println!("Dry run only. Re-run with --write to apply changes.");
    }
  }

  Ok(())
}

/// Try to merge two similar recipes intelligently.
/// Combines parameters, uses the more complete doc, merges dependencies,
/// and for body lines, keeps unique lines from both.
fn smart_merge_recipes(a: &ContextRecipe, b: &ContextRecipe) -> String {
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

fn print_agent_command(command: &AgentCommands) {
  let prompt = match command {
    AgentCommands::Implement => include_str!("../../../agent/commands/implement.md"),
    AgentCommands::ReviewArchitecture => {
      include_str!("../../../agent/commands/review-architecture.md")
    }
    AgentCommands::RefreshIndex => include_str!("../../../agent/commands/refresh-index.md"),
    AgentCommands::SystemPrompt => include_str!("../../../agent/prompts/system.md"),
    AgentCommands::Verify => include_str!("../../../agent/commands/verify.md"),
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

pub struct AiClient {
  provider: Box<dyn provider::AiProvider>,
}

impl AiClient {
  pub fn from_env() -> Result<Self, Box<dyn Error>> {
    let provider = env::var("JUST_AI_PROVIDER").unwrap_or_else(|_| "openai".to_owned());
    let base_url = env::var("JUST_AI_BASE_URL").unwrap_or_else(|_| match provider.as_str() {
      "ollama" => "http://localhost:11434".to_owned(),
      _ => "https://api.openai.com/v1".to_owned(),
    });
    let model = env::var("JUST_AI_MODEL").unwrap_or_else(|_| match provider.as_str() {
      "ollama" => "llama3.1".to_owned(),
      "openai" => "gpt-5.6-terra".to_owned(),
      _ => "gpt-5-mini".to_owned(),
    });
    let api_key = env::var("JUST_AI_API_KEY").ok();
    if provider != "ollama" && api_key.is_none() {
      return Err("JUST_AI_API_KEY is required unless JUST_AI_PROVIDER=ollama is used".into());
    }

    let provider: Box<dyn provider::AiProvider> = match provider.as_str() {
      "openai" => Box::new(provider::OpenAiResponsesProvider::new(
        base_url,
        model,
        api_key.expect("API key requirement checked above"),
      )),
      "ollama" => Box::new(provider::OllamaProvider::new(base_url, model, api_key)),
      "openai-compatible" => Box::new(provider::OpenAiCompatibleProvider::new(
        base_url, model, api_key,
      )),
      "anthropic" => Box::new(provider::AnthropicProvider::new(
        base_url,
        model,
        api_key.expect("API key requirement checked above"),
      )),
      "azure" => {
        let deployment = env::var("JUST_AI_AZURE_DEPLOYMENT").unwrap_or_else(|_| model.clone());
        let api_version =
          env::var("JUST_AI_API_VERSION").unwrap_or_else(|_| "2024-08-01-preview".to_owned());
        Box::new(provider::AzureOpenAiProvider::new(
          base_url,
          deployment,
          api_key.expect("API key requirement checked above"),
          api_version,
        ))
      }
      "gemini" => Box::new(provider::GeminiProvider::new(
        base_url,
        model,
        api_key.expect("API key requirement checked above"),
      )),
      other => return Err(format!("unsupported JUST_AI_PROVIDER `{other}`").into()),
    };
    Ok(Self { provider })
  }

  pub fn complete_json<T>(&self, system: &str, user: &str) -> Result<T, Box<dyn Error>>
  where
    T: for<'de> Deserialize<'de> + ResponseContract,
  {
    let content = provider::AiProvider::complete(
      self.provider.as_ref(),
      &provider::AiRequest {
        system: system.into(),
        user: user.into(),
        schema_name: std::any::type_name::<T>()
          .rsplit("::")
          .next()
          .unwrap_or("just_ai_response")
          .to_owned(),
        schema: T::schema(),
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
