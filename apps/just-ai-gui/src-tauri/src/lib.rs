use std::{
  env,
  path::PathBuf,
  sync::Mutex,
  time::{Instant, SystemTime, UNIX_EPOCH},
};

use just_ai::{
  ai_responses::{
    AddRecipeResponse as AiAddRecipeResponse, ExplainResponse, FixResponse, SuggestResponse,
    WorkflowResponse as AiWorkflowResponse,
  },
  application::{
    execution::{CancellationToken, PreparedRun, RecipeExecutor, RunConfirmation, RunRequest},
    history::{RunRecord, create_history},
  },
  cli::AiClient,
  config::{Config, HistoryConfig},
  inspection::{ProjectContext, inspect_project_at},
  prompts,
  proposal::{handle_add, handle_fix, handle_workflow, validate_fix_proposal},
};
use serde::{Deserialize, Serialize};
use tauri::Emitter;

#[tauri::command]
fn inspect_project(project_root: PathBuf) -> Result<ProjectContext, String> {
  if !project_root.is_dir() {
    return Err(format!(
      "project root is not a directory: {}",
      project_root.display()
    ));
  }

  inspect_project_at("just", project_root).map_err(|error| error.to_string())
}

#[tauri::command]
async fn prepare_run(request: RunRequest) -> Result<PreparedRun, String> {
  tauri::async_runtime::spawn_blocking(move || RecipeExecutor::new("just").prepare(request))
    .await
    .map_err(|error| error.to_string())?
    .map_err(|error| error.to_string())
}

#[tauri::command]
async fn recent_runs(project_root: PathBuf, limit: usize) -> Result<Vec<RunRecord>, String> {
  if !project_root.is_dir() {
    return Err(format!(
      "project root is not a directory: {}",
      project_root.display()
    ));
  }
  let config = HistoryConfig::default();
  let history = create_history(config).map_err(|error| error.to_string())?;
  history
    .recent(limit.min(100))
    .map_err(|error| error.to_string())
}

#[derive(Serialize)]
struct RunResult {
  success: bool,
  exit_code: Option<i32>,
  stdout: String,
  stderr: String,
}

#[derive(Default)]
struct ActiveRun(Mutex<Option<CancellationToken>>);

#[tauri::command]
async fn execute_run(
  app: tauri::AppHandle,
  active_run: tauri::State<'_, ActiveRun>,
  prepared: PreparedRun,
  confirmation: RunConfirmation,
) -> Result<RunResult, String> {
  let cancellation = CancellationToken::default();
  {
    let mut active = active_run.0.lock().map_err(|error| error.to_string())?;
    if active.is_some() {
      return Err("another recipe is already running".into());
    }
    *active = Some(cancellation.clone());
  }
  let result = tauri::async_runtime::spawn_blocking(move || -> Result<RunResult, String> {
    let started_at_ms = SystemTime::now()
      .duration_since(UNIX_EPOCH)
      .map_err(|error| error.to_string())?
      .as_millis();
    let started = Instant::now();
    let _project_root = prepared.request.project_root.clone();
    let completed = RecipeExecutor::new("just")
      .execute_streaming(&prepared, &confirmation, &cancellation, |event| {
        let _ = app.emit("run-event", event);
      })
      .map_err(|error| error.to_string())?;
    let record = RunRecord::completed(
      &prepared.request,
      started_at_ms,
      started.elapsed().as_millis(),
      &completed,
      &HistoryConfig::default(),
    );
    let config = HistoryConfig::default();
    let history = create_history(config).map_err(|error| error.to_string())?;
    history.append(&record).map_err(|error| error.to_string())?;
    Ok(RunResult {
      success: completed.status.success(),
      exit_code: completed.status.code(),
      stdout: String::from_utf8_lossy(&completed.stdout).into_owned(),
      stderr: String::from_utf8_lossy(&completed.stderr).into_owned(),
    })
  })
  .await;
  *active_run.0.lock().map_err(|error| error.to_string())? = None;
  result.map_err(|error| error.to_string())?
}

#[tauri::command]
fn cancel_run(active_run: tauri::State<'_, ActiveRun>) -> Result<bool, String> {
  let active = active_run.0.lock().map_err(|error| error.to_string())?;
  if let Some(cancellation) = active.as_ref() {
    cancellation.cancel();
    Ok(true)
  } else {
    Ok(false)
  }
}

#[tauri::command]
async fn ai_suggest(project_root: PathBuf) -> Result<SuggestResponse, String> {
  if !project_root.is_dir() {
    return Err(format!(
      "project root is not a directory: {}",
      project_root.display()
    ));
  }
  let context =
    inspect_project_at("just", project_root.clone()).map_err(|error| error.to_string())?;
  let project_root = project_root.canonicalize().unwrap_or(project_root);
  env::set_current_dir(&project_root).map_err(|error| error.to_string())?;
  let response = AiClient::from_env()
    .map_err(|error| error.to_string())?
    .complete_json::<SuggestResponse>(
      "Suggest useful missing just recipes for this project.",
      &prompts::suggest(
        &serde_json::to_string_pretty(&context).map_err(|error| error.to_string())?,
      ),
    )
    .map_err(|error| error.to_string())?;
  Ok(response)
}

#[tauri::command]
async fn ai_explain(project_root: PathBuf, recipe_name: String) -> Result<ExplainResponse, String> {
  if !project_root.is_dir() {
    return Err(format!(
      "project root is not a directory: {}",
      project_root.display()
    ));
  }
  let context =
    inspect_project_at("just", project_root.clone()).map_err(|error| error.to_string())?;
  let project_root = project_root.canonicalize().unwrap_or(project_root);
  env::set_current_dir(&project_root).map_err(|error| error.to_string())?;
  let recipe = context
    .find_recipe(&recipe_name)
    .ok_or_else(|| format!("recipe `{recipe_name}` not found"))?;
  let response = AiClient::from_env()
    .map_err(|error| error.to_string())?
    .complete_json::<ExplainResponse>(
      "Explain a just recipe using the supplied project context.",
      &prompts::explain(
        &serde_json::to_string_pretty(&context).map_err(|error| error.to_string())?,
        &serde_json::to_string_pretty(recipe).map_err(|error| error.to_string())?,
      ),
    )
    .map_err(|error| error.to_string())?;
  Ok(response)
}

#[derive(Deserialize)]
struct AddRecipeRequest {
  request: String,
  write: bool,
}

#[derive(Serialize)]
struct GuiAddRecipeResponse {
  success: bool,
  message: String,
  diff: Option<String>,
  recipe_name: Option<String>,
  summary: Option<String>,
  recipe: Option<AiAddRecipeResponse>,
}

#[tauri::command]
async fn ai_add_recipe(
  project_root: PathBuf,
  request: AddRecipeRequest,
) -> Result<GuiAddRecipeResponse, String> {
  if !project_root.is_dir() {
    return Err(format!(
      "project root is not a directory: {}",
      project_root.display()
    ));
  }
  let context =
    inspect_project_at("just", project_root.clone()).map_err(|error| error.to_string())?;
  let project_root = project_root.canonicalize().unwrap_or(project_root);
  env::set_current_dir(&project_root).map_err(|error| error.to_string())?;
  let response = AiClient::from_env()
    .map_err(|error| error.to_string())?
    .complete_json::<AiAddRecipeResponse>(
      "Generate a safe just recipe proposal as strict JSON.",
      &prompts::add(
        &serde_json::to_string_pretty(&context).map_err(|error| error.to_string())?,
        &request.request,
      ),
    )
    .map_err(|error| error.to_string())?;

  if request.write {
    handle_add(
      &PathBuf::from("just"),
      &context,
      &request.request,
      response,
      true,
    )
    .map_err(|error| error.to_string())?;
    Ok(GuiAddRecipeResponse {
      success: true,
      message: "Recipe added successfully".to_string(),
      diff: None,
      recipe_name: None,
      summary: None,
      recipe: None,
    })
  } else {
    let summary = response.summary.clone();
    let recipe_name = response.recipe.name.clone();
    Ok(GuiAddRecipeResponse {
      success: true,
      message: "Recipe proposed (dry run)".to_string(),
      diff: None,
      recipe_name: Some(recipe_name),
      summary: Some(summary),
      recipe: Some(response),
    })
  }
}

#[derive(Deserialize)]
struct FixRecipeRequest {
  recipe_name: String,
  write: bool,
}

#[derive(Serialize)]
struct GuiFixRecipeResponse {
  success: bool,
  message: String,
  diff: Option<String>,
  recipe_name: Option<String>,
  summary: Option<String>,
  recipe: Option<FixResponse>,
}

#[tauri::command]
async fn ai_fix_recipe(
  project_root: PathBuf,
  request: FixRecipeRequest,
) -> Result<GuiFixRecipeResponse, String> {
  if !project_root.is_dir() {
    return Err(format!(
      "project root is not a directory: {}",
      project_root.display()
    ));
  }
  let context =
    inspect_project_at("just", project_root.clone()).map_err(|error| error.to_string())?;
  let project_root = project_root.canonicalize().unwrap_or(project_root);
  env::set_current_dir(&project_root).map_err(|error| error.to_string())?;

  let config = Config::load(&project_root).map_err(|error| error.to_string())?;
  let history = create_history(config.history).map_err(|error| error.to_string())?;
  let failed_runs = history
    .query(Some(&request.recipe_name), Some(false), 10)
    .map_err(|error| error.to_string())?;
  let history_json =
    serde_json::to_string_pretty(&failed_runs).map_err(|error| error.to_string())?;
  let context_json = serde_json::to_string_pretty(&context).map_err(|error| error.to_string())?;

  let response = AiClient::from_env()
    .map_err(|error| error.to_string())?
    .complete_json::<FixResponse>(
      "Generate a fix proposal for a failing just recipe as strict JSON.",
      &prompts::fix(&context_json, &request.recipe_name, &history_json),
    )
    .map_err(|error| error.to_string())?;

  if request.write {
    handle_fix(
      &PathBuf::from("just"),
      &context,
      &request.recipe_name,
      response,
      true,
    )
    .map_err(|error| error.to_string())?;
    Ok(GuiFixRecipeResponse {
      success: true,
      message: "Fix applied successfully".to_string(),
      diff: None,
      recipe_name: None,
      summary: None,
      recipe: None,
    })
  } else {
    let summary = response.summary.clone();
    let recipe_name = response.recipe.name.clone();
    Ok(GuiFixRecipeResponse {
      success: true,
      message: "Fix proposed (dry run)".to_string(),
      diff: None,
      recipe_name: Some(recipe_name),
      summary: Some(summary),
      recipe: Some(response),
    })
  }
}

#[derive(Deserialize)]
struct WorkflowRequest {
  request: String,
  write: bool,
}

#[derive(Serialize)]
struct GuiWorkflowResponse {
  success: bool,
  message: String,
  diff: Option<String>,
  recipes: Vec<String>,
  summary: Option<String>,
  execution_order: Option<Vec<String>>,
  workflow: Option<AiWorkflowResponse>,
}

#[tauri::command]
async fn ai_workflow(
  project_root: PathBuf,
  request: WorkflowRequest,
) -> Result<GuiWorkflowResponse, String> {
  if !project_root.is_dir() {
    return Err(format!(
      "project root is not a directory: {}",
      project_root.display()
    ));
  }
  let context =
    inspect_project_at("just", project_root.clone()).map_err(|error| error.to_string())?;
  let project_root = project_root.canonicalize().unwrap_or(project_root);
  env::set_current_dir(&project_root).map_err(|error| error.to_string())?;

  let response = AiClient::from_env()
    .map_err(|error| error.to_string())?
    .complete_json::<AiWorkflowResponse>(
      "Generate a multi-recipe workflow as strict JSON.",
      &prompts::workflow(
        &serde_json::to_string_pretty(&context).map_err(|error| error.to_string())?,
        &request.request,
      ),
    )
    .map_err(|error| error.to_string())?;

  let recipe_names: Vec<String> = response.recipes.iter().map(|r| r.name.clone()).collect();

  if request.write {
    handle_workflow(
      &PathBuf::from("just"),
      &context,
      &request.request,
      response,
      true,
    )
    .map_err(|error| error.to_string())?;
    Ok(GuiWorkflowResponse {
      success: true,
      message: "Workflow applied successfully".to_string(),
      diff: None,
      recipes: recipe_names,
      summary: None,
      execution_order: None,
      workflow: None,
    })
  } else {
    let summary = response.summary.clone();
    let execution_order = response.execution_order.clone();
    let recipe_names: Vec<String> = response.recipes.iter().map(|r| r.name.clone()).collect();
    Ok(GuiWorkflowResponse {
      success: true,
      message: "Workflow proposed (dry run)".to_string(),
      diff: None,
      recipes: recipe_names,
      summary: Some(summary),
      execution_order: Some(execution_order),
      workflow: Some(response),
    })
  }
}

#[derive(Deserialize)]
struct FixBatchRequest {
  write: bool,
}

#[derive(Serialize)]
struct GuiFixBatchResponse {
  success: bool,
  message: String,
  diff: Option<String>,
  fixed_recipes: Vec<String>,
}

#[tauri::command]
async fn ai_fix_batch(
  project_root: PathBuf,
  request: FixBatchRequest,
) -> Result<GuiFixBatchResponse, String> {
  if !project_root.is_dir() {
    return Err(format!(
      "project root is not a directory: {}",
      project_root.display()
    ));
  }
  let context =
    inspect_project_at("just", project_root.clone()).map_err(|error| error.to_string())?;
  let project_root = project_root.canonicalize().unwrap_or(project_root);
  env::set_current_dir(&project_root).map_err(|error| error.to_string())?;

  let config = Config::load(&project_root).map_err(|error| error.to_string())?;
  let history = create_history(config.history).map_err(|error| error.to_string())?;
  let failed_runs = history
    .query(None, Some(false), 100)
    .map_err(|error| error.to_string())?;

  let mut failed_recipes: Vec<String> = failed_runs
    .iter()
    .map(|r| r.recipe.clone())
    .collect::<std::collections::HashSet<_>>()
    .into_iter()
    .collect();
  failed_recipes.sort();

  if failed_recipes.is_empty() {
    return Ok(GuiFixBatchResponse {
      success: true,
      message: "No failed recipes found".to_string(),
      diff: None,
      fixed_recipes: vec![],
    });
  }

  let client = AiClient::from_env().map_err(|error| error.to_string())?;
  let source = context
    .root_source()
    .ok_or("project context does not contain a root justfile source")?;
  let original =
    just_ai::bounded_file::read_utf8(source, just_ai::bounded_file::max_editable_file_bytes())
      .map_err(|error| error.to_string())?;
  let mut proposed = original.clone();
  let mut fixed_recipes = Vec::new();

  for recipe_name in &failed_recipes {
    let recipe_history = history
      .query(Some(recipe_name), Some(false), 10)
      .map_err(|error| error.to_string())?;
    let history_json =
      serde_json::to_string_pretty(&recipe_history).map_err(|error| error.to_string())?;
    let context_json = serde_json::to_string_pretty(&context).map_err(|error| error.to_string())?;

    let response = client
      .complete_json::<FixResponse>(
        "Generate a fix proposal for a failing just recipe as strict JSON.",
        &prompts::fix(&context_json, recipe_name, &history_json),
      )
      .map_err(|error| error.to_string())?;

    validate_fix_proposal(&context, &response.recipe, recipe_name)
      .map_err(|error| error.to_string())?;

    let recipe_rendered = just_ai::proposal::render_fix_recipe(&response.recipe);
    proposed = just_ai::proposal::replace_recipe(&proposed, recipe_name, &recipe_rendered);

    just_ai::bounded_file::ensure_text_limit(
      &proposed,
      "proposed justfile",
      just_ai::bounded_file::max_editable_file_bytes(),
    )
    .map_err(|error| error.to_string())?;

    let risks = just_ai::domain::risk::RiskFinding::scan_lines(&response.recipe.body);
    let risk = just_ai::domain::risk::RiskLevel::highest(&risks);
    if risk == just_ai::domain::risk::RiskLevel::Blocked {
      return Err(format!(
        "generated fix for `{}` has blocked risk and will not be written",
        recipe_name
      ));
    }

    fixed_recipes.push(response.recipe.name.clone());
  }

  let just_binary_path = PathBuf::from("just");
  just_ai::proposal::validate_justfile(&just_binary_path, source, &proposed)
    .map_err(|error| error.to_string())?;

  if request.write {
    just_ai::application::patches::apply_reviewed_change(source, &original, &proposed)
      .map_err(|error| error.to_string())?;
    Ok(GuiFixBatchResponse {
      success: true,
      message: format!("Fixed {} failed recipes", fixed_recipes.len()),
      diff: None,
      fixed_recipes,
    })
  } else {
    Ok(GuiFixBatchResponse {
      success: true,
      message: format!(
        "Proposed fixes for {} failed recipes (dry run)",
        fixed_recipes.len()
      ),
      diff: None,
      fixed_recipes,
    })
  }
}

#[derive(Deserialize)]
struct ExplainBatchRequest {
  recipes: Option<Vec<String>>,
  module: Option<String>,
}

#[derive(Serialize)]
struct GuiExplainBatchResponse {
  success: bool,
  explanations: Vec<ExplainResponse>,
}

#[tauri::command]
async fn ai_explain_batch(
  project_root: PathBuf,
  request: ExplainBatchRequest,
) -> Result<GuiExplainBatchResponse, String> {
  if !project_root.is_dir() {
    return Err(format!(
      "project root is not a directory: {}",
      project_root.display()
    ));
  }
  let context =
    inspect_project_at("just", project_root.clone()).map_err(|error| error.to_string())?;
  let project_root = project_root.canonicalize().unwrap_or(project_root);
  env::set_current_dir(&project_root).map_err(|error| error.to_string())?;

  let client = AiClient::from_env().map_err(|error| error.to_string())?;
  let mut explanations = Vec::new();

  for recipe_ctx in &context.recipes {
    if let Some(ref filter) = request.recipes
      && !filter.contains(&recipe_ctx.namepath) {
      continue;
    }
    if let Some(ref module) = request.module
      && !recipe_ctx.namepath.starts_with(module) {
      continue;
    }

    let response = client
      .complete_json::<ExplainResponse>(
        "Explain a just recipe using the supplied project context.",
        &prompts::explain(
          &serde_json::to_string_pretty(&context).map_err(|error| error.to_string())?,
          &serde_json::to_string_pretty(recipe_ctx).map_err(|error| error.to_string())?,
        ),
      )
      .map_err(|error| error.to_string())?;

    explanations.push(response);
  }

  Ok(GuiExplainBatchResponse {
    success: true,
    explanations,
  })
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
  tauri::Builder::default()
    .manage(ActiveRun::default())
    .invoke_handler(tauri::generate_handler![
      inspect_project,
      prepare_run,
      recent_runs,
      execute_run,
      cancel_run,
      ai_suggest,
      ai_explain,
      ai_add_recipe,
      ai_fix_recipe,
      ai_workflow,
      ai_fix_batch,
      ai_explain_batch,
    ])
    .run(tauri::generate_context!())
    .expect("failed to run just-ai desktop application");
}
