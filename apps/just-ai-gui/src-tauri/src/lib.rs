use std::{
  env, fs,
  path::PathBuf,
  sync::Mutex,
  time::{Instant, SystemTime, UNIX_EPOCH},
};

use just_ai::{
  ContextParameter,
  ai_responses::{
    AddRecipeResponse, AddRecipeResponse as AiAddRecipeResponse,
    ComposeWorkflowResponse as AiComposeWorkflowResponse, ExplainResponse, FixResponse,
    RecipeProposal, SuggestResponse, TemplateResponse as AiTemplateResponse,
    WorkflowResponse as AiWorkflowResponse,
  },
  application::{
    execution::{CancellationToken, PreparedRun, RecipeExecutor, RunConfirmation, RunRequest},
    history::{RunRecord, create_history},
    patches::apply_reviewed_change,
  },
  bounded_file::{max_editable_file_bytes, read_utf8},
  cli::AiClient,
  config::{Config, HistoryConfig},
  domain::risk::{RiskFinding, RiskLevel},
  inspection::{ContextRecipe, ProjectContext, inspect_project_at},
  prompts,
  proposal::{
    handle_add, handle_compose_workflow, handle_fix, handle_workflow, replace_recipe, unified_diff,
    validate_fix_proposal, validate_justfile,
  },
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

// ===== Template Commands =====

#[derive(Deserialize)]
struct TemplateRequest {
  request: String,
}

#[derive(Serialize)]
struct GuiTemplateResponse {
  success: bool,
  template_name: String,
  template_description: String,
  template_category: String,
  template_parameters: Vec<TemplateParameterInfo>,
  template_body: Vec<String>,
  summary: String,
}

#[derive(Serialize)]
struct TemplateParameterInfo {
  name: String,
  description: String,
  required: bool,
  default: Option<String>,
}

#[tauri::command]
async fn ai_template(
  project_root: PathBuf,
  request: TemplateRequest,
) -> Result<GuiTemplateResponse, String> {
  if !project_root.is_dir() {
    return Err(format!(
      "project root is not a directory: {}",
      project_root.display()
    ));
  }
  let context =
    inspect_project_at("just", project_root.clone()).map_err(|error| error.to_string())?;

  let response = AiClient::from_env()
    .map_err(|error| error.to_string())?
    .complete_json::<AiTemplateResponse>(
      "Generate a reusable just recipe template as strict JSON.",
      &prompts::template(
        &serde_json::to_string_pretty(&context).map_err(|error| error.to_string())?,
        &request.request,
      ),
    )
    .map_err(|error| error.to_string())?;

  let template_params: Vec<TemplateParameterInfo> = response
    .template
    .parameters
    .iter()
    .map(|p| TemplateParameterInfo {
      name: p.name.clone(),
      description: p.description.clone(),
      required: p.required,
      default: p.default.clone(),
    })
    .collect();

  Ok(GuiTemplateResponse {
    success: true,
    template_name: response.template.name,
    template_description: response.template.description,
    template_category: response.template.category,
    template_parameters: template_params,
    template_body: response.template.body,
    summary: response.summary,
  })
}

#[derive(Deserialize)]
struct InstantiateTemplateRequest {
  template: String,
  values: std::collections::HashMap<String, String>,
  write: bool,
}

#[derive(Serialize)]
struct GuiInstantiateTemplateResponse {
  success: bool,
  message: String,
  diff: Option<String>,
  recipe_name: Option<String>,
  summary: Option<String>,
  recipe: Option<AiAddRecipeResponse>,
}

#[tauri::command]
async fn ai_instantiate_template(
  project_root: PathBuf,
  request: InstantiateTemplateRequest,
) -> Result<GuiInstantiateTemplateResponse, String> {
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

  // First, generate the template from the template name
  let template_prompt = format!(
    "Find or create a template named '{}' for this project.",
    request.template
  );
  let template_response = AiClient::from_env()
    .map_err(|error| error.to_string())?
    .complete_json::<AiTemplateResponse>(
      "Generate a reusable just recipe template as strict JSON.",
      &prompts::template(
        &serde_json::to_string_pretty(&context).map_err(|error| error.to_string())?,
        &template_prompt,
      ),
    )
    .map_err(|error| error.to_string())?;

  // Check required parameters - fill in defaults if provided
  let mut values_map = request.values.clone();
  for param in &template_response.template.parameters {
    if param.required && !values_map.contains_key(&param.name) {
      if let Some(default) = &param.default {
        values_map.insert(param.name.clone(), default.clone());
      } else {
        return Err(format!("required parameter '{}' not provided", param.name));
      }
    }
  }

  // Substitute template parameters in the body
  let mut recipe_body = Vec::new();
  for line in &template_response.template.body {
    let mut substituted = line.clone();
    for (key, value) in &values_map {
      substituted = substituted.replace(&format!("{{{{{key}}}}}"), value);
    }
    recipe_body.push(substituted);
  }

  // Build the recipe proposal from the template
  let recipe = RecipeProposal {
    name: template_response.template.name.clone(),
    doc: Some(template_response.template.description.clone()),
    parameters: template_response
      .template
      .parameters
      .iter()
      .map(|p| just_ai::ai_responses::RecipeParameterProposal {
        name: p.name.clone(),
        default: values_map.get(&p.name).cloned().or(p.default.clone()),
      })
      .collect(),
    dependencies: vec![],
    body: recipe_body,
  };

  let source = context
    .root_source()
    .ok_or("project context does not contain a root justfile source")?;
  let original = read_utf8(source, max_editable_file_bytes()).map_err(|error| error.to_string())?;
  let rendered = just_ai::proposal::render_recipe(&recipe);
  let proposed = just_ai::proposal::insert_recipe_grouped(
    &original,
    &rendered,
    &context,
    &recipe.dependencies,
    &recipe.name,
  );
  just_ai::bounded_file::ensure_text_limit(
    &proposed,
    "proposed justfile",
    just_ai::bounded_file::max_editable_file_bytes(),
  )
  .map_err(|error| error.to_string())?;

  let just_binary_path = PathBuf::from("just");
  just_ai::proposal::validate_justfile(&just_binary_path, source, &proposed)
    .map_err(|error| error.to_string())?;

  let risks = just_ai::domain::risk::RiskFinding::scan_lines(&recipe.body);
  let risk = just_ai::domain::risk::RiskLevel::highest(&risks);
  if risk == just_ai::domain::risk::RiskLevel::Blocked {
    return Err("instantiated template has blocked risk and will not be written".into());
  }

  let diff = just_ai::proposal::unified_diff(source, &original, &proposed);

  let recipe_clone = recipe.clone();
  let recipe_name = recipe.name.clone();

  if request.write {
    just_ai::application::patches::apply_reviewed_change(source, &original, &proposed)
      .map_err(|error| error.to_string())?;

    Ok(GuiInstantiateTemplateResponse {
      success: true,
      message: format!("Template '{}' instantiated and written", request.template),
      diff: Some(diff),
      recipe_name: Some(recipe_name),
      summary: Some(format!("Template '{}' instantiated", request.template)),
      recipe: Some(AiAddRecipeResponse {
        summary: format!("Template '{}' instantiated", request.template),
        recipe: recipe_clone,
        rationale: vec!["Instantiated from template".to_string()],
      }),
    })
  } else {
    let recipe_clone2 = recipe.clone();
    Ok(GuiInstantiateTemplateResponse {
      success: true,
      message: format!("Template '{}' instantiated (dry run)", request.template),
      diff: Some(diff),
      recipe_name: Some(recipe_name),
      summary: Some(format!("Template '{}' instantiated", request.template)),
      recipe: Some(AiAddRecipeResponse {
        summary: format!("Template '{}' instantiated", request.template),
        recipe: recipe_clone2,
        rationale: vec!["Instantiated from template".to_string()],
      }),
    })
  }
}

#[derive(Deserialize)]
struct ComposeWorkflowRequest {
  request: String,
  write: bool,
}

#[derive(Serialize)]
struct GuiComposeWorkflowResponse {
  success: bool,
  message: String,
  diff: Option<String>,
  recipes: Vec<String>,
  summary: Option<String>,
  execution_order: Option<Vec<String>>,
  workflow: Option<AiComposeWorkflowResponse>,
}

#[tauri::command]
async fn ai_compose_workflow(
  project_root: PathBuf,
  request: ComposeWorkflowRequest,
) -> Result<GuiComposeWorkflowResponse, String> {
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
    .complete_json::<AiComposeWorkflowResponse>(
      "Compose a workflow by reusing and adapting existing recipes as strict JSON.",
      &prompts::compose_workflow(
        &serde_json::to_string_pretty(&context).map_err(|error| error.to_string())?,
        &request.request,
      ),
    )
    .map_err(|error| error.to_string())?;

  let recipe_names: Vec<String> = response.recipes.iter().map(|r| r.name.clone()).collect();

  if request.write {
    handle_compose_workflow(
      &PathBuf::from("just"),
      &context,
      &request.request,
      response,
      true,
    )
    .map_err(|error| error.to_string())?;
    Ok(GuiComposeWorkflowResponse {
      success: true,
      message: "Composed workflow applied successfully".to_string(),
      diff: None,
      recipes: recipe_names,
      summary: None,
      execution_order: None,
      workflow: None,
    })
  } else {
    let summary = response.summary.clone();
    let execution_order = response.execution_order.clone();
    Ok(GuiComposeWorkflowResponse {
      success: true,
      message: "Workflow composed (dry run)".to_string(),
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
      && !filter.contains(&recipe_ctx.namepath)
    {
      continue;
    }
    if let Some(ref module) = request.module
      && !recipe_ctx.namepath.starts_with(module)
    {
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

// ===== Migrate Commands =====

#[derive(Deserialize)]
struct MigrateAnalyzeRequest {
  json: Option<bool>,
}

#[derive(Serialize)]
struct GuiMigrateAnalyzeResult {
  success: bool,
  total_recipes: usize,
  unreferenced_recipes: Vec<String>,
  isolated_recipes: Vec<String>,
  cycles: Vec<Vec<String>>,
  dependency_depths: std::collections::HashMap<String, usize>,
  similar_recipes: Vec<(String, String, f64)>,
}

#[tauri::command]
async fn ai_migrate_analyze(
  project_root: PathBuf,
  _request: MigrateAnalyzeRequest,
) -> Result<GuiMigrateAnalyzeResult, String> {
  if !project_root.is_dir() {
    return Err(format!(
      "project root is not a directory: {}",
      project_root.display()
    ));
  }
  let context =
    inspect_project_at("just", project_root.clone()).map_err(|error| error.to_string())?;

  let unreferenced = context.find_unreferenced_recipes();
  let isolated = context.find_isolated_recipes();
  let cycles = context.detect_cycles();
  let depths = context.calculate_dependency_depths();
  let similar = context.find_similar_recipes(0.8);

  Ok(GuiMigrateAnalyzeResult {
    success: true,
    total_recipes: context.recipes.len(),
    unreferenced_recipes: unreferenced.iter().map(|r| r.namepath.clone()).collect(),
    isolated_recipes: isolated.iter().map(|r| r.namepath.clone()).collect(),
    cycles,
    dependency_depths: depths,
    similar_recipes: similar,
  })
}

#[derive(Deserialize)]
struct MigrateModularizeRequest {
  write: bool,
}

#[derive(Serialize)]
struct GuiMigrateModularizeResult {
  success: bool,
  message: String,
  modules: Vec<String>,
  imports: Vec<String>,
  moved_recipes: Vec<String>,
  diff: Option<String>,
}

#[tauri::command]
async fn ai_migrate_modularize(
  project_root: PathBuf,
  request: MigrateModularizeRequest,
) -> Result<GuiMigrateModularizeResult, String> {
  if !project_root.is_dir() {
    return Err(format!(
      "project root is not a directory: {}",
      project_root.display()
    ));
  }
  let context =
    inspect_project_at("just", project_root.clone()).map_err(|error| error.to_string())?;

  let source = context
    .root_source()
    .ok_or("project context does not contain a root justfile source")?;
  let original = read_utf8(source, max_editable_file_bytes()).map_err(|e| e.to_string())?;
  let mut proposed = original.clone();

  // Group recipes by common prefix
  let mut groups: std::collections::HashMap<String, Vec<&ContextRecipe>> =
    std::collections::HashMap::new();
  for recipe in &context.recipes {
    let prefix = recipe
      .name
      .split('-')
      .next()
      .unwrap_or(&recipe.name)
      .to_owned();
    groups.entry(prefix).or_default().push(recipe);
  }

  let source_dir = source
    .parent()
    .map(PathBuf::from)
    .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));
  let mut import_statements = Vec::new();
  let mut module_names = Vec::new();
  let mut moved_recipes = Vec::new();

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

    for (i, line) in lines.iter().enumerate() {
      let trimmed = line.trim_start();
      if trimmed.starts_with("import ") {
        last_import_idx = Some(i);
      }
    }

    for (i, line) in lines.iter().enumerate() {
      result.push(line.to_string());
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

  for (prefix, recipes) in &groups {
    if recipes.len() < 2 {
      continue;
    }
    let module_filename = format!("{prefix}.just");
    module_names.push(prefix.clone());

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

    if module_content.is_empty() {
      continue;
    }

    for recipe in recipes {
      proposed = replace_recipe(&proposed, &recipe.name, "");
      moved_recipes.push(recipe.name.clone());
    }
    import_statements.push(format!("import '{}'", module_filename));
  }

  if !import_statements.is_empty() {
    proposed = add_imports_at_top(&proposed, &import_statements);
  }

  let diff = unified_diff(source, &original, &proposed);

  if request.write {
    // Write module files FIRST so validation can find them
    for prefix in &module_names {
      let mut module_content = String::new();
      if let Some(recipes) = groups.get(prefix) {
        for recipe in recipes {
          let recipe_text = extract_recipe(&original, &recipe.name);
          if !recipe_text.is_empty() {
            if !module_content.is_empty() {
              module_content.push('\n');
            }
            module_content.push_str(&recipe_text);
          }
        }
      }
      if !module_content.is_empty() {
        let module_filename = format!("{prefix}.just");
        let module_path = source_dir.join(&module_filename);
        fs::write(&module_path, module_content).map_err(|e| e.to_string())?;
      }
    }
    validate_justfile(&PathBuf::from("just"), source, &proposed).map_err(|e| e.to_string())?;
    apply_reviewed_change(source, &original, &proposed).map_err(|e| e.to_string())?;

    Ok(GuiMigrateModularizeResult {
      success: true,
      message: format!("Created {} module files", module_names.len()),
      modules: module_names,
      imports: import_statements,
      moved_recipes,
      diff: Some(diff),
    })
  } else {
    Ok(GuiMigrateModularizeResult {
      success: true,
      message: "Dry run - no changes written".to_string(),
      modules: module_names,
      imports: import_statements,
      moved_recipes,
      diff: Some(diff),
    })
  }
}

#[derive(Deserialize)]
struct MigrateDeduplicateRequest {
  write: bool,
  merge: bool,
  similarity_threshold: Option<f64>,
}

#[derive(Serialize)]
struct GuiMigrateDeduplicateResult {
  success: bool,
  message: String,
  similar_pairs: Vec<(String, String, f64)>,
  removed: Vec<String>,
  merged: Vec<String>,
  diff: Option<String>,
}

#[tauri::command]
async fn ai_migrate_deduplicate(
  project_root: PathBuf,
  request: MigrateDeduplicateRequest,
) -> Result<GuiMigrateDeduplicateResult, String> {
  if !project_root.is_dir() {
    return Err(format!(
      "project root is not a directory: {}",
      project_root.display()
    ));
  }
  let context =
    inspect_project_at("just", project_root.clone()).map_err(|error| error.to_string())?;

  let threshold = request.similarity_threshold.unwrap_or(0.8);
  let similar = context.find_similar_recipes(threshold);

  let source = context
    .root_source()
    .ok_or("project context does not contain a root justfile source")?;
  let original = read_utf8(source, max_editable_file_bytes()).map_err(|e| e.to_string())?;
  let mut proposed = original.clone();

  let mut similar_pairs = Vec::new();
  let mut removed = Vec::new();
  let mut merged = Vec::new();

  fn smart_merge_recipes(a: &ContextRecipe, b: &ContextRecipe) -> String {
    let name = if a.name.len() <= b.name.len() {
      &a.name
    } else {
      &b.name
    };
    let doc = if a.doc.as_deref().map(|d| d.len()).unwrap_or(0)
      >= b.doc.as_deref().map(|d| d.len()).unwrap_or(0)
    {
      a.doc.clone()
    } else {
      b.doc.clone()
    };
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
    let mut deps: std::collections::HashSet<String> = a.dependencies.iter().cloned().collect();
    deps.extend(b.dependencies.iter().cloned());
    let mut dependencies: Vec<String> = deps.into_iter().collect();
    dependencies.sort();
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

  for (a, b, sim) in &similar {
    similar_pairs.push((a.clone(), b.clone(), *sim));

    let recipe_a = context.find_recipe(a);
    let recipe_b = context.find_recipe(b);

    if request.write {
      if request.merge {
        if let (Some(ra), Some(rb)) = (recipe_a, recipe_b) {
          let merged_recipe = smart_merge_recipes(ra, rb);
          proposed = replace_recipe(&proposed, &ra.name, "");
          proposed = replace_recipe(&proposed, &rb.name, &merged_recipe);
          merged.push(ra.name.clone());
        }
      } else {
        let keep = if a.len() <= b.len() { a } else { b };
        let remove = if keep == a { b } else { a };

        if let Some(recipe) = context.find_recipe(remove) {
          proposed = replace_recipe(&proposed, &recipe.name, "");
          removed.push(remove.clone());
        }
      }
    }
  }

  let diff = unified_diff(source, &original, &proposed);

  if request.write {
    validate_justfile(&PathBuf::from("just"), source, &proposed).map_err(|e| e.to_string())?;
    apply_reviewed_change(source, &original, &proposed).map_err(|e| e.to_string())?;

    Ok(GuiMigrateDeduplicateResult {
      success: true,
      message: format!(
        "Processed {} similar pairs (removed: {}, merged: {})",
        similar_pairs.len(),
        removed.len(),
        merged.len()
      ),
      similar_pairs,
      removed,
      merged,
      diff: Some(diff),
    })
  } else {
    Ok(GuiMigrateDeduplicateResult {
      success: true,
      message: "Dry run - no changes written".to_string(),
      similar_pairs,
      removed,
      merged,
      diff: Some(diff),
    })
  }
}

// ========================================================================
// Export Context and Doctor Commands
// ========================================================================

#[derive(Serialize)]
struct GuiExportContextResult {
  success: bool,
  context: serde_json::Value,
}

#[tauri::command]
async fn ai_export_context(project_root: PathBuf) -> Result<GuiExportContextResult, String> {
  if !project_root.is_dir() {
    return Err(format!(
      "project root is not a directory: {}",
      project_root.display()
    ));
  }
  let context =
    inspect_project_at("just", project_root.clone()).map_err(|error| error.to_string())?;

  Ok(GuiExportContextResult {
    success: true,
    context: serde_json::to_value(&context).map_err(|error| error.to_string())?,
  })
}

#[derive(Serialize)]
struct DoctorRecipeGui {
  namepath: String,
  risk: RiskLevel,
  risks: Vec<RiskFinding>,
}

#[derive(Serialize)]
struct GuiDoctorResult {
  success: bool,
  total_recipes: usize,
  low: usize,
  medium: usize,
  high: usize,
  blocked: usize,
  highest_risk: RiskLevel,
  recipes: Vec<DoctorRecipeGui>,
}

#[tauri::command]
async fn ai_doctor(project_root: PathBuf) -> Result<GuiDoctorResult, String> {
  if !project_root.is_dir() {
    return Err(format!(
      "project root is not a directory: {}",
      project_root.display()
    ));
  }
  let context =
    inspect_project_at("just", project_root.clone()).map_err(|error| error.to_string())?;

  let recipes = context
    .recipes
    .iter()
    .map(|recipe| DoctorRecipeGui {
      namepath: recipe.namepath.clone(),
      risk: recipe.risk,
      risks: recipe.risks.clone(),
    })
    .collect::<Vec<_>>();

  let total_recipes = recipes.len();
  let low = recipes.iter().filter(|r| r.risk == RiskLevel::Low).count();
  let medium = recipes.iter().filter(|r| r.risk == RiskLevel::Medium).count();
  let high = recipes.iter().filter(|r| r.risk == RiskLevel::High).count();
  let blocked = recipes.iter().filter(|r| r.risk == RiskLevel::Blocked).count();
  let highest_risk = recipes
    .iter()
    .map(|r| r.risk)
    .max()
    .unwrap_or(RiskLevel::Low);

  Ok(GuiDoctorResult {
    success: true,
    total_recipes,
    low,
    medium,
    high,
    blocked,
    highest_risk,
    recipes,
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
      ai_migrate_analyze,
      ai_migrate_modularize,
      ai_migrate_deduplicate,
      ai_template,
      ai_instantiate_template,
      ai_compose_workflow,
      ai_export_context,
      ai_doctor,
    ])
    .run(tauri::generate_context!())
    .expect("failed to run just-ai desktop application");
}
