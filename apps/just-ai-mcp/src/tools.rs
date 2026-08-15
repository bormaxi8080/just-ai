use {
  just_ai::{
    ContextParameter,
    application::{
      execution::{RecipeExecutor, RunConfirmation, RunRequest},
      history::create_history,
      patches::apply_reviewed_change,
    },
    bounded_file::{max_editable_file_bytes, read_utf8},
    cli::AiClient,
    config::Config,
    inspection::{ContextRecipe, inspect_project_at},
    prompts,
    proposal::{replace_recipe, unified_diff, validate_justfile},
  },
  serde_json::{Map, Value, json},
  std::{
    env, fs,
    path::{Path, PathBuf},
  },
};

pub(super) fn tool_definitions() -> Value {
  json!([
    {
      "name": "inspect_project",
      "description": "Inspect recipes and deterministic risk findings through just's JSON dump without executing recipes.",
      "inputSchema": tool_schema(false),
      "annotations": { "readOnlyHint": true, "destructiveHint": false }
    },
    {
      "name": "doctor",
      "description": "Return deterministic risk reports for recipes without executing them.",
      "inputSchema": tool_schema(false),
      "annotations": { "readOnlyHint": true, "destructiveHint": false }
    },
    {
      "name": "prepare_run",
      "description": "Dry-run a recipe and return preview, risk, and confirmation policy. Never executes the recipe.",
      "inputSchema": tool_schema(true),
      "annotations": { "readOnlyHint": true, "destructiveHint": false }
    },
    {
      "name": "run_recipe",
      "description": "Execute a recipe through just with policy-based confirmation. Requires appropriate confirmation based on risk level.",
      "inputSchema": run_recipe_schema(),
      "annotations": { "readOnlyHint": false, "destructiveHint": true }
    },
    {
      "name": "get_history",
      "description": "Query run history with optional filters (recipe name, success status, limit).",
      "inputSchema": get_history_schema(),
      "annotations": { "readOnlyHint": true, "destructiveHint": false }
    },
    {
      "name": "add_recipe",
      "description": "Ask an AI provider to propose a new recipe and optionally write it to the justfile.",
      "inputSchema": add_recipe_schema(),
      "annotations": { "readOnlyHint": false, "destructiveHint": true }
    },
    {
      "name": "fix_recipe",
      "description": "Ask an AI provider to propose a fix for a failed recipe and optionally write it to the justfile.",
      "inputSchema": fix_recipe_schema(),
      "annotations": { "readOnlyHint": false, "destructiveHint": true }
    },
    {
      "name": "add_workflow",
      "description": "Ask an AI provider to propose a multi-recipe workflow and optionally write it to the justfile.",
      "inputSchema": add_workflow_schema(),
      "annotations": { "readOnlyHint": false, "destructiveHint": true }
    },
    {
      "name": "fix_batch",
      "description": "Ask an AI provider to propose fixes for all failed recipes and optionally write them to the justfile.",
      "inputSchema": fix_batch_schema(),
      "annotations": { "readOnlyHint": false, "destructiveHint": true }
    },
    {
      "name": "explain_batch",
      "description": "Ask an AI provider to explain multiple recipes (all or filtered by module).",
      "inputSchema": explain_batch_schema(),
      "annotations": { "readOnlyHint": true, "destructiveHint": false }
    },
    {
      "name": "create_template",
      "description": "Ask an AI provider to create a reusable recipe template with placeholders.",
      "inputSchema": create_template_schema(),
      "annotations": { "readOnlyHint": false, "destructiveHint": true }
    },
    {
      "name": "instantiate_template",
      "description": "Instantiate a recipe template with provided parameter values.",
      "inputSchema": instantiate_template_schema(),
      "annotations": { "readOnlyHint": false, "destructiveHint": true }
    },
    {
      "name": "compose_workflow",
      "description": "Ask an AI provider to compose a workflow by reusing and adapting existing recipes.",
      "inputSchema": compose_workflow_schema(),
      "annotations": { "readOnlyHint": false, "destructiveHint": true }
    },
    {
      "name": "migrate_analyze",
      "description": "Analyze project structure: find unreferenced, isolated recipes, cycles, dependency depths, and similar recipes.",
      "inputSchema": migrate_analyze_schema(),
      "annotations": { "readOnlyHint": true, "destructiveHint": false }
    },
    {
      "name": "migrate_modularize",
      "description": "Group recipes by prefix into module files and update imports. Dry-run by default.",
      "inputSchema": migrate_modularize_schema(),
      "annotations": { "readOnlyHint": true, "destructiveHint": false }
    },
    {
      "name": "migrate_deduplicate",
      "description": "Find and optionally remove or smart-merge similar recipes. Dry-run by default.",
      "inputSchema": migrate_deduplicate_schema(),
      "annotations": { "readOnlyHint": false, "destructiveHint": true }
    }
  ])
}

fn tool_schema(include_recipe: bool) -> Value {
  let mut properties = json!({});
  let mut required = Vec::new();
  if include_recipe {
    properties["recipe"] = json!({ "type": "string" });
    properties["arguments"] =
      json!({ "type": "array", "items": { "type": "string" }, "default": [] });
    required.push("recipe");
  }
  json!({ "type": "object", "properties": properties, "required": required, "additionalProperties": false })
}

fn run_recipe_schema() -> Value {
  json!({
    "type": "object",
    "properties": {
      "recipe": { "type": "string" },
      "arguments": { "type": "array", "items": { "type": "string" }, "default": [] },
      "confirmation": {
        "type": "object",
        "properties": {
          "type": { "type": "string", "enum": ["none", "confirmed", "typed"] },
          "phrase": { "type": "string" }
        },
        "required": ["type"]
      }
    },
    "required": ["recipe"],
    "additionalProperties": false
  })
}

fn get_history_schema() -> Value {
  json!({
    "type": "object",
    "properties": {
      "recipe": { "type": "string" },
      "success": { "type": "boolean" },
      "limit": { "type": "integer", "default": 20 }
    },
    "additionalProperties": false
  })
}

fn add_recipe_schema() -> Value {
  json!({
    "type": "object",
    "properties": {
      "request": { "type": "string" },
      "write": { "type": "boolean", "default": false }
    },
    "required": ["request"],
    "additionalProperties": false
  })
}

fn fix_recipe_schema() -> Value {
  json!({
    "type": "object",
    "properties": {
      "recipe": { "type": "string" },
      "write": { "type": "boolean", "default": false }
    },
    "required": ["recipe"],
    "additionalProperties": false
  })
}

fn add_workflow_schema() -> Value {
  json!({
    "type": "object",
    "properties": {
      "request": { "type": "string" },
      "write": { "type": "boolean", "default": false }
    },
    "required": ["request"],
    "additionalProperties": false
  })
}

fn fix_batch_schema() -> Value {
  json!({
    "type": "object",
    "properties": {
      "write": { "type": "boolean", "default": false }
    },
    "additionalProperties": false
  })
}

fn explain_batch_schema() -> Value {
  json!({
    "type": "object",
    "properties": {
      "recipes": { "type": "array", "items": { "type": "string" } },
      "module": { "type": "string" }
    },
    "additionalProperties": false
  })
}

fn create_template_schema() -> Value {
  json!({
    "type": "object",
    "properties": {
      "request": { "type": "string" },
      "write": { "type": "boolean", "default": false }
    },
    "required": ["request"],
    "additionalProperties": false
  })
}

fn instantiate_template_schema() -> Value {
  json!({
    "type": "object",
    "properties": {
      "template": { "type": "string" },
      "values": { "type": "object", "additionalProperties": { "type": "string" } },
      "write": { "type": "boolean", "default": false }
    },
    "required": ["template", "values"],
    "additionalProperties": false
  })
}

fn compose_workflow_schema() -> Value {
  json!({
    "type": "object",
    "properties": {
      "request": { "type": "string" },
      "write": { "type": "boolean", "default": false }
    },
    "required": ["request"],
    "additionalProperties": false
  })
}

fn migrate_analyze_schema() -> Value {
  json!({
    "type": "object",
    "properties": {
      "json": { "type": "boolean", "default": false }
    },
    "additionalProperties": false
  })
}

fn migrate_modularize_schema() -> Value {
  json!({
    "type": "object",
    "properties": {
      "write": { "type": "boolean", "default": false }
    },
    "additionalProperties": false
  })
}

fn migrate_deduplicate_schema() -> Value {
  json!({
    "type": "object",
    "properties": {
      "write": { "type": "boolean", "default": false },
      "merge": { "type": "boolean", "default": false },
      "similarity_threshold": { "type": "number", "minimum": 0.0, "maximum": 1.0, "default": 0.8 }
    },
    "additionalProperties": false
  })
}

pub(super) fn call_tool(params: &Value) -> Result<Value, String> {
  let project_root = env::current_dir().map_err(|error| error.to_string())?;
  call_tool_at(params, Path::new("just"), &project_root)
}

fn call_tool_at(params: &Value, just_binary: &Path, project_root: &Path) -> Result<Value, String> {
  let name = params
    .get("name")
    .and_then(Value::as_str)
    .ok_or("tool name is required")?;
  let allowed_arguments: &[&str] = match name {
    "inspect_project" | "doctor" => &[],
    "prepare_run" => &["recipe", "arguments"],
    "run_recipe" => &["recipe", "arguments", "confirmation"],
    "get_history" => &["recipe", "success", "limit"],
    "add_recipe" => &["request", "write"],
    "fix_recipe" => &["recipe", "write"],
    "add_workflow" => &["request", "write"],
    "fix_batch" => &["write"],
    "explain_batch" => &["recipes", "module"],
    "create_template" => &["request", "write"],
    "instantiate_template" => &["template", "values", "write"],
    "compose_workflow" => &["request", "write"],
    "migrate_analyze" => &["json"],
    "migrate_modularize" => &["write"],
    "migrate_deduplicate" => &["write", "merge", "similarity_threshold"],
    _ => return Err(format!("unknown tool `{name}`")),
  };
  let empty_arguments = Map::new();
  let arguments = match params.get("arguments") {
    None | Some(Value::Null) => &empty_arguments,
    Some(Value::Object(arguments)) => arguments,
    Some(_) => return Err("`arguments` must be an object".to_owned()),
  };
  if let Some(argument) = arguments
    .keys()
    .find(|argument| !allowed_arguments.contains(&argument.as_str()))
  {
    return Err(format!("unsupported argument `{argument}` for `{name}`"));
  }
  let value = match name {
    "inspect_project" => serde_json::to_value(
      inspect_project_at(just_binary, project_root).map_err(|error| error.to_string())?,
    )
    .map_err(|error| error.to_string())?,
    "doctor" => {
      let context =
        inspect_project_at(just_binary, project_root).map_err(|error| error.to_string())?;
      json!({ "recipes": context.recipes.into_iter().map(|recipe| json!({
        "namepath": recipe.namepath, "risk": recipe.risk, "findings": recipe.risks
      })).collect::<Vec<_>>() })
    }
    "prepare_run" => {
      let recipe = string_argument(arguments, "recipe")?;
      let arguments = arguments
        .get("arguments")
        .cloned()
        .unwrap_or_else(|| json!([]));
      let arguments: Vec<String> =
        serde_json::from_value(arguments).map_err(|error| error.to_string())?;
      serde_json::to_value(
        RecipeExecutor::new(just_binary)
          .prepare(RunRequest {
            project_root: PathBuf::from(project_root),
            recipe,
            arguments,
          })
          .map_err(|error| error.to_string())?,
      )
      .map_err(|error| error.to_string())?
    }
    "run_recipe" => {
      let recipe = string_argument(arguments, "recipe")?;
      let arguments_value = arguments
        .get("arguments")
        .cloned()
        .unwrap_or_else(|| json!([]));
      let arguments_vec: Vec<String> =
        serde_json::from_value(arguments_value).map_err(|error| error.to_string())?;
      let confirmation = parse_confirmation(arguments.get("confirmation"))?;
      let executor = RecipeExecutor::new(just_binary);
      let prepared = executor
        .prepare(RunRequest {
          project_root: PathBuf::from(project_root),
          recipe: recipe.clone(),
          arguments: arguments_vec.clone(),
        })
        .map_err(|error| error.to_string())?;
      let completed = executor
        .execute(&prepared, &confirmation)
        .map_err(|error| error.to_string())?;
      json!({
        "status": completed.status.to_string(),
        "success": completed.status.success(),
        "stdout": String::from_utf8_lossy(&completed.stdout),
        "stderr": String::from_utf8_lossy(&completed.stderr),
        "cancelled": completed.cancelled,
      })
    }
    "get_history" => {
      let config = Config::load(project_root).map_err(|error| error.to_string())?;
      let history = create_history(config.history).map_err(|error| error.to_string())?;
      let recipe_filter = arguments
        .get("recipe")
        .and_then(Value::as_str)
        .map(str::to_owned);
      let success_filter = arguments.get("success").and_then(Value::as_bool);
      let limit = arguments.get("limit").and_then(Value::as_u64).unwrap_or(20) as usize;
      let records = history
        .query(recipe_filter.as_deref(), success_filter, limit)
        .map_err(|error| error.to_string())?;
      json!({ "records": records })
    }
    "add_recipe" => {
      let request = string_argument(arguments, "request")?;
      let write = arguments
        .get("write")
        .and_then(Value::as_bool)
        .unwrap_or(false);
      let context =
        inspect_project_at(just_binary, project_root).map_err(|error| error.to_string())?;
      let client = AiClient::from_env().map_err(|error| error.to_string())?;
      let response = client
        .complete_json::<just_ai::ai_responses::AddRecipeResponse>(
          "Generate a safe just recipe proposal as strict JSON.",
          &prompts::add(
            &serde_json::to_string_pretty(&context).map_err(|e| e.to_string())?,
            &request,
          ),
        )
        .map_err(|error| error.to_string())?;
      let source = context
        .root_source()
        .ok_or("project context does not contain a root justfile source")?;
      let original =
        just_ai::bounded_file::read_utf8(source, just_ai::bounded_file::max_editable_file_bytes())
          .map_err(|error| error.to_string())?;
      let recipe = just_ai::proposal::render_recipe(&response.recipe);
      let proposed = just_ai::proposal::insert_recipe_grouped(
        &original,
        &recipe,
        &context,
        &response.recipe.dependencies,
        &response.recipe.name,
      );
      just_ai::bounded_file::ensure_text_limit(
        &proposed,
        "proposed justfile",
        just_ai::bounded_file::max_editable_file_bytes(),
      )
      .map_err(|error| error.to_string())?;
      let just_binary_path = Path::new(just_binary);
      just_ai::proposal::validate_justfile(just_binary_path, source, &proposed)
        .map_err(|error| error.to_string())?;
      if write {
        just_ai::application::patches::apply_reviewed_change(source, &original, &proposed)
          .map_err(|error| error.to_string())?;
      }
      json!({
        "summary": response.summary,
        "rationale": response.rationale,
        "recipe": {
          "name": response.recipe.name,
          "body": response.recipe.body,
          "dependencies": response.recipe.dependencies,
        },
        "diff": just_ai::proposal::unified_diff(source, &original, &proposed),
        "written": write,
      })
    }
    "fix_recipe" => {
      let recipe_name = string_argument(arguments, "recipe")?;
      let write = arguments
        .get("write")
        .and_then(Value::as_bool)
        .unwrap_or(false);
      let config = Config::load(project_root).map_err(|error| error.to_string())?;
      let history = create_history(config.history).map_err(|error| error.to_string())?;
      let failed_runs = history
        .query(Some(&recipe_name), Some(false), 10)
        .map_err(|error| error.to_string())?;
      let context =
        inspect_project_at(just_binary, project_root).map_err(|error| error.to_string())?;
      let client = AiClient::from_env().map_err(|error| error.to_string())?;
      let response = client
        .complete_json::<just_ai::ai_responses::FixResponse>(
          "Generate a fix proposal for a failing just recipe as strict JSON.",
          &prompts::fix(
            &serde_json::to_string_pretty(&context).map_err(|e| e.to_string())?,
            &recipe_name,
            &serde_json::to_string_pretty(&failed_runs).map_err(|e| e.to_string())?,
          ),
        )
        .map_err(|error| error.to_string())?;
      let source = context
        .root_source()
        .ok_or("project context does not contain a root justfile source")?;
      let original =
        just_ai::bounded_file::read_utf8(source, just_ai::bounded_file::max_editable_file_bytes())
          .map_err(|error| error.to_string())?;
      let recipe = just_ai::proposal::render_fix_recipe(&response.recipe);
      let proposed = just_ai::proposal::replace_recipe(&original, &recipe_name, &recipe);
      just_ai::bounded_file::ensure_text_limit(
        &proposed,
        "proposed justfile",
        just_ai::bounded_file::max_editable_file_bytes(),
      )
      .map_err(|error| error.to_string())?;
      let just_binary_path = Path::new(just_binary);
      just_ai::proposal::validate_justfile(just_binary_path, source, &proposed)
        .map_err(|error| error.to_string())?;
      if write {
        just_ai::application::patches::apply_reviewed_change(source, &original, &proposed)
          .map_err(|error| error.to_string())?;
      }
      json!({
        "summary": response.summary,
        "rationale": response.rationale,
        "recipe": {
          "name": response.recipe.name,
          "body": response.recipe.body,
          "dependencies": response.recipe.dependencies,
        },
        "diff": just_ai::proposal::unified_diff(source, &original, &proposed),
        "written": write,
      })
    }
    "add_workflow" => {
      let request = string_argument(arguments, "request")?;
      let write = arguments
        .get("write")
        .and_then(Value::as_bool)
        .unwrap_or(false);
      let context =
        inspect_project_at(just_binary, project_root).map_err(|error| error.to_string())?;
      let client = AiClient::from_env().map_err(|error| error.to_string())?;
      let response = client
        .complete_json::<just_ai::ai_responses::WorkflowResponse>(
          "Generate a multi-recipe workflow as strict JSON.",
          &prompts::workflow(
            &serde_json::to_string_pretty(&context).map_err(|e| e.to_string())?,
            &request,
          ),
        )
        .map_err(|error| error.to_string())?;
      let source = context
        .root_source()
        .ok_or("project context does not contain a root justfile source")?;
      let original =
        just_ai::bounded_file::read_utf8(source, just_ai::bounded_file::max_editable_file_bytes())
          .map_err(|error| error.to_string())?;

      let mut proposed = original.clone();
      let rendered_recipes: std::collections::HashMap<String, String> = response
        .recipes
        .iter()
        .map(|r| (r.name.clone(), just_ai::proposal::render_recipe(r)))
        .collect();

      for recipe_name in &response.execution_order {
        let recipe = rendered_recipes
          .get(recipe_name)
          .ok_or_else(|| format!("recipe `{recipe_name}` not found in workflow"))?;
        let recipe_proposal = response
          .recipes
          .iter()
          .find(|r| r.name == *recipe_name)
          .ok_or_else(|| format!("recipe proposal for `{recipe_name}` not found"))?;

        proposed = just_ai::proposal::insert_recipe_grouped(
          &proposed,
          recipe,
          &context,
          &recipe_proposal.dependencies,
          recipe_name,
        );
      }

      just_ai::bounded_file::ensure_text_limit(
        &proposed,
        "proposed justfile",
        just_ai::bounded_file::max_editable_file_bytes(),
      )
      .map_err(|error| error.to_string())?;
      let just_binary_path = Path::new(just_binary);
      just_ai::proposal::validate_justfile(just_binary_path, source, &proposed)
        .map_err(|error| error.to_string())?;
      if write {
        just_ai::application::patches::apply_reviewed_change(source, &original, &proposed)
          .map_err(|error| error.to_string())?;
      }
      json!({
        "summary": response.summary,
        "rationale": response.rationale,
        "recipes": response.recipes.iter().map(|r| json!({
          "name": r.name,
          "body": r.body,
          "dependencies": r.dependencies,
        })).collect::<Vec<_>>(),
        "execution_order": response.execution_order,
        "diff": just_ai::proposal::unified_diff(source, &original, &proposed),
        "written": write,
      })
    }
    "fix_batch" => {
      let write = arguments
        .get("write")
        .and_then(Value::as_bool)
        .unwrap_or(false);
      let config = Config::load(project_root).map_err(|error| error.to_string())?;
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
        json!({
          "summary": "No failed recipes found in history.",
          "fixes": [],
          "diff": "",
          "written": false,
        })
      } else {
        let context =
          inspect_project_at(just_binary, project_root).map_err(|error| error.to_string())?;
        let client = AiClient::from_env().map_err(|error| error.to_string())?;
        let source = context
          .root_source()
          .ok_or("project context does not contain a root justfile source")?;
        let original = just_ai::bounded_file::read_utf8(
          source,
          just_ai::bounded_file::max_editable_file_bytes(),
        )
        .map_err(|e| e.to_string())?;
        let mut proposed = original.clone();
        let mut fixes = Vec::new();

        for recipe_name in &failed_recipes {
          let recipe_history = history
            .query(Some(recipe_name), Some(false), 10)
            .map_err(|error| error.to_string())?;
          let history_json =
            serde_json::to_string_pretty(&recipe_history).map_err(|e| e.to_string())?;
          let context_json = serde_json::to_string_pretty(&context).map_err(|e| e.to_string())?;

          let response = client
            .complete_json::<just_ai::ai_responses::FixResponse>(
              "Generate a fix proposal for a failing just recipe as strict JSON.",
              &prompts::fix(&context_json, recipe_name, &history_json),
            )
            .map_err(|error| error.to_string())?;

          just_ai::proposal::validate_fix_proposal(&context, &response.recipe, recipe_name)
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

          fixes.push(json!({
            "name": response.recipe.name,
            "risk": risk.to_string(),
            "rationale": response.rationale,
          }));
        }

        let just_binary_path = Path::new(just_binary);
        let source = context.root_source().unwrap();
        just_ai::proposal::validate_justfile(just_binary_path, source, &proposed)
          .map_err(|error| error.to_string())?;

        let diff = just_ai::proposal::unified_diff(source, &original, &proposed);

        if write {
          just_ai::application::patches::apply_reviewed_change(source, &original, &proposed)
            .map_err(|error| error.to_string())?;
        }

        json!({
          "summary": format!("Fixed {} failed recipes", fixes.len()),
          "fixes": fixes,
          "diff": diff,
          "written": write,
        })
      }
    }
    "explain_batch" => {
      let recipes_filter: Option<Vec<String>> = arguments
        .get("recipes")
        .and_then(|v| serde_json::from_value(v.clone()).ok());
      let module_filter = arguments
        .get("module")
        .and_then(Value::as_str)
        .map(str::to_owned);

      let context =
        inspect_project_at(just_binary, project_root).map_err(|error| error.to_string())?;
      let client = AiClient::from_env().map_err(|error| error.to_string())?;

      let mut explanations = Vec::new();

      for recipe_ctx in &context.recipes {
        if let Some(ref filter) = recipes_filter
          && !filter.contains(&recipe_ctx.namepath)
        {
          continue;
        }
        if let Some(ref module) = module_filter
          && !recipe_ctx.namepath.starts_with(module)
        {
          continue;
        }

        let response = client
          .complete_json::<just_ai::ai_responses::ExplainResponse>(
            "Explain a just recipe using the supplied project context.",
            &prompts::explain(
              &serde_json::to_string_pretty(&context).map_err(|e| e.to_string())?,
              &serde_json::to_string_pretty(recipe_ctx).map_err(|e| e.to_string())?,
            ),
          )
          .map_err(|error| error.to_string())?;

        explanations.push(json!({
          "recipe": recipe_ctx.namepath,
          "summary": response.summary,
          "explanation": response.explanation,
          "parameters": response.parameters,
          "dependencies": response.dependencies,
          "risks": response.risks,
        }));
      }

      json!({
        "explanations": explanations,
      })
    }
    "create_template" => {
      let request = string_argument(arguments, "request")?;
      let write = arguments
        .get("write")
        .and_then(Value::as_bool)
        .unwrap_or(false);
      let context =
        inspect_project_at(just_binary, project_root).map_err(|error| error.to_string())?;
      let client = AiClient::from_env().map_err(|error| error.to_string())?;
      let response = client
        .complete_json::<just_ai::ai_responses::TemplateResponse>(
          "Generate a reusable just recipe template as strict JSON.",
          &prompts::template(
            &serde_json::to_string_pretty(&context).map_err(|e| e.to_string())?,
            &request,
          ),
        )
        .map_err(|error| error.to_string())?;

      let _source = context
        .root_source()
        .ok_or("project context does not contain a root justfile source")?;

      let _template_name = response.template.name.clone();
      let template_json =
        serde_json::to_string_pretty(&response.template).map_err(|e| e.to_string())?;

      if write {
        // Store template as a comment in the justfile or a separate file
        // For now, we just return the template info
      }

      json!({
        "summary": response.summary,
        "template": {
          "name": response.template.name,
          "description": response.template.description,
          "category": response.template.category,
          "parameters": response.template.parameters.iter().map(|p| json!({
            "name": p.name,
            "description": p.description,
            "required": p.required,
            "default": p.default,
          })).collect::<Vec<_>>(),
          "body": response.template.body,
        },
        "template_json": template_json,
        "written": write,
      })
    }
    "instantiate_template" => {
      let template_name = string_argument(arguments, "template")?;
      let values_map: std::collections::HashMap<String, String> = arguments
        .get("values")
        .and_then(|v| serde_json::from_value(v.clone()).ok())
        .unwrap_or_default();
      let write = arguments
        .get("write")
        .and_then(Value::as_bool)
        .unwrap_or(false);

      let context =
        inspect_project_at(just_binary, project_root).map_err(|error| error.to_string())?;
      let client = AiClient::from_env().map_err(|error| error.to_string())?;

      // Generate template from request
      let template_prompt = format!(
        "Find or create a template named '{}' for this project.",
        template_name
      );
      let template_response = client
        .complete_json::<just_ai::ai_responses::TemplateResponse>(
          "Generate a reusable just recipe template as strict JSON.",
          &prompts::template(
            &serde_json::to_string_pretty(&context).map_err(|e| e.to_string())?,
            &template_prompt,
          ),
        )
        .map_err(|error| error.to_string())?;

      // Check required parameters
      for param in &template_response.template.parameters {
        if param.required && !values_map.contains_key(&param.name) {
          if param.default.is_some() {
            // Use default
          } else {
            return Err(format!("required parameter '{}' not provided", param.name));
          }
        }
      }

      // Substitute template parameters
      let mut recipe_body = Vec::new();
      for line in &template_response.template.body {
        let mut substituted = line.clone();
        for (key, value) in &values_map {
          substituted = substituted.replace(&format!("{{{{{key}}}}}"), value);
        }
        recipe_body.push(substituted);
      }

      let recipe = just_ai::ai_responses::RecipeProposal {
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
      let original =
        just_ai::bounded_file::read_utf8(source, just_ai::bounded_file::max_editable_file_bytes())
          .map_err(|error| error.to_string())?;
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
      let just_binary_path = Path::new(just_binary);
      just_ai::proposal::validate_justfile(just_binary_path, source, &proposed)
        .map_err(|error| error.to_string())?;

      let risks = just_ai::domain::risk::RiskFinding::scan_lines(&recipe.body);
      let risk = just_ai::domain::risk::RiskLevel::highest(&risks);
      if risk == just_ai::domain::risk::RiskLevel::Blocked {
        return Err("instantiated template has blocked risk and will not be written".into());
      }

      let diff = just_ai::proposal::unified_diff(source, &original, &proposed);

      if write {
        just_ai::application::patches::apply_reviewed_change(source, &original, &proposed)
          .map_err(|error| error.to_string())?;
      }

      json!({
        "summary": format!("Template '{}' instantiated", template_name),
        "recipe": {
          "name": recipe.name,
          "body": recipe.body,
          "dependencies": recipe.dependencies,
        },
        "diff": diff,
        "written": write,
      })
    }
    "compose_workflow" => {
      let request = string_argument(arguments, "request")?;
      let write = arguments
        .get("write")
        .and_then(Value::as_bool)
        .unwrap_or(false);
      let context =
        inspect_project_at(just_binary, project_root).map_err(|error| error.to_string())?;
      let client = AiClient::from_env().map_err(|error| error.to_string())?;
      let response = client
        .complete_json::<just_ai::ai_responses::ComposeWorkflowResponse>(
          "Compose a multi-recipe workflow by reusing existing recipes as strict JSON.",
          &prompts::compose_workflow(
            &serde_json::to_string_pretty(&context).map_err(|e| e.to_string())?,
            &request,
          ),
        )
        .map_err(|error| error.to_string())?;

      let source = context
        .root_source()
        .ok_or("project context does not contain a root justfile source")?;
      let original =
        just_ai::bounded_file::read_utf8(source, just_ai::bounded_file::max_editable_file_bytes())
          .map_err(|error| error.to_string())?;

      // Validate all recipes
      let existing_recipe_names: std::collections::HashSet<_> =
        context.recipes.iter().map(|r| r.namepath.clone()).collect();
      for recipe in &response.recipes {
        let exists = existing_recipe_names.contains(&recipe.name);
        match recipe.source.as_str() {
          "existing" => {
            if !exists {
              return Err(format!(
                "recipe `{}` marked as existing but not found in project",
                recipe.name
              ));
            }
          }
          "modified" => {
            if !exists {
              return Err(format!(
                "recipe `{}` marked as modified but not found in project",
                recipe.name
              ));
            }
          }
          "new" => {
            if exists {
              return Err(format!(
                "recipe `{}` marked as new but already exists in project",
                recipe.name
              ));
            }
          }
          _ => {
            return Err(format!(
              "invalid source type `{}`, must be existing|new|modified",
              recipe.source
            ));
          }
        }
        for dependency in &recipe.dependencies {
          let in_workflow = response.recipes.iter().any(|r| r.name == *dependency);
          if !context.has_recipe(dependency) && !in_workflow {
            return Err(format!("dependency recipe `{dependency}` does not exist"));
          }
        }
      }

      // Build rendered recipes map
      let mut rendered_recipes: std::collections::HashMap<String, String> =
        std::collections::HashMap::new();
      for recipe in &response.recipes {
        let proposal = just_ai::ai_responses::RecipeProposal {
          name: recipe.name.clone(),
          doc: recipe.doc.clone(),
          parameters: recipe.parameters.clone(),
          dependencies: recipe.dependencies.clone(),
          body: recipe.body.clone(),
        };
        rendered_recipes.insert(
          recipe.name.clone(),
          just_ai::proposal::render_recipe(&proposal),
        );
      }

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

        if recipe_proposal.source != "existing" {
          proposed = just_ai::proposal::insert_recipe_grouped(
            &proposed,
            recipe,
            &context,
            &recipe_proposal.dependencies,
            recipe_name,
          );
        }
      }

      just_ai::bounded_file::ensure_text_limit(
        &proposed,
        "proposed justfile",
        just_ai::bounded_file::max_editable_file_bytes(),
      )
      .map_err(|error| error.to_string())?;
      let just_binary_path = Path::new(just_binary);
      just_ai::proposal::validate_justfile(just_binary_path, source, &proposed)
        .map_err(|error| error.to_string())?;

      let all_risks: Vec<_> = response
        .recipes
        .iter()
        .flat_map(|r| just_ai::domain::risk::RiskFinding::scan_lines(&r.body))
        .collect();
      let risk = just_ai::domain::risk::RiskLevel::highest(&all_risks);
      if risk == just_ai::domain::risk::RiskLevel::Blocked {
        return Err("composed workflow has blocked risk and will not be written".into());
      }

      let diff = just_ai::proposal::unified_diff(source, &original, &proposed);

      if write {
        just_ai::application::patches::apply_reviewed_change(source, &original, &proposed)
          .map_err(|error| error.to_string())?;
      }

      json!({
        "summary": response.summary,
        "recipes": response.recipes.iter().map(|r| json!({
          "name": r.name,
          "source": r.source,
          "body": r.body,
          "dependencies": r.dependencies,
        })).collect::<Vec<_>>(),
        "execution_order": response.execution_order,
        "diff": diff,
        "written": write,
      })
    }
    "migrate_analyze" => {
      let json_output = arguments
        .get("json")
        .and_then(Value::as_bool)
        .unwrap_or(false);

      let context =
        inspect_project_at(just_binary, project_root).map_err(|error| error.to_string())?;

      let unreferenced = context.find_unreferenced_recipes();
      let isolated = context.find_isolated_recipes();
      let cycles = context.detect_cycles();
      let depths = context.calculate_dependency_depths();
      let similar = context.find_similar_recipes(0.8);

      let result = json!({
        "total_recipes": context.recipes.len(),
        "unreferenced_recipes": unreferenced.iter().map(|r| r.namepath.clone()).collect::<Vec<_>>(),
        "isolated_recipes": isolated.iter().map(|r| r.namepath.clone()).collect::<Vec<_>>(),
        "cycles": cycles,
        "dependency_depths": depths,
        "similar_recipes": similar.iter().map(|(a, b, s)| json!({"recipe1": a, "recipe2": b, "similarity": s})).collect::<Vec<_>>(),
      });

      if json_output {
        result
      } else {
        // Human-readable output
        let mut lines = Vec::new();
        lines.push("Project Analysis".to_string());
        lines.push(format!("Total recipes: {}", context.recipes.len()));
        lines.push(format!("Unreferenced recipes: {}", unreferenced.len()));
        for r in &unreferenced {
          lines.push(format!("  - {}", r.namepath));
        }
        lines.push(format!("Isolated recipes: {}", isolated.len()));
        for r in &isolated {
          lines.push(format!("  - {}", r.namepath));
        }
        lines.push(format!("Cycles detected: {}", cycles.len()));
        for cycle in &cycles {
          lines.push(format!("  - {}", cycle.join(" -> ")));
        }
        lines.push("Dependency depths:".to_string());
        for (name, depth) in &depths {
          lines.push(format!("  {}: depth {}", name, depth));
        }
        lines.push(format!("Similar recipe pairs: {}", similar.len()));
        for (name1, name2, sim) in &similar {
          lines.push(format!(
            "  {} <-> {}: {:.1}% similar",
            name1,
            name2,
            sim * 100.0
          ));
        }
        json!({ "output": lines.join("\n") })
      }
    }
    "migrate_modularize" => {
      let write = arguments
        .get("write")
        .and_then(Value::as_bool)
        .unwrap_or(false);

      let context =
        inspect_project_at(just_binary, project_root).map_err(|error| error.to_string())?;

      // Call the modularize_project function from cli.rs
      // We need to run a subcommand-like approach
      let source = context
        .root_source()
        .ok_or("project context does not contain a root justfile source")?;
      let original = read_utf8(source, max_editable_file_bytes()).map_err(|e| e.to_string())?;
      let mut proposed = original.clone();

      // Group recipes by common prefix
      let mut groups: std::collections::HashMap<String, Vec<&just_ai::inspection::ContextRecipe>> =
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

      let source_dir = source.parent().unwrap_or_else(|| Path::new("."));
      let mut import_statements = Vec::new();
      let mut module_names = Vec::new();
      let mut moved_recipes = Vec::new();

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

      if write {
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
        validate_justfile(just_binary, source, &proposed).map_err(|e| e.to_string())?;
        apply_reviewed_change(source, &original, &proposed).map_err(|e| e.to_string())?;
      }

      json!({
        "modules": module_names,
        "imports": import_statements,
        "moved_recipes": moved_recipes,
        "diff": diff,
        "dry_run": !write,
      })
    }
    "migrate_deduplicate" => {
      let write = arguments
        .get("write")
        .and_then(Value::as_bool)
        .unwrap_or(false);
      let merge = arguments
        .get("merge")
        .and_then(Value::as_bool)
        .unwrap_or(false);
      let threshold = arguments
        .get("similarity_threshold")
        .and_then(Value::as_f64)
        .unwrap_or(0.8);

      let context =
        inspect_project_at(just_binary, project_root).map_err(|error| error.to_string())?;

      let similar = context.find_similar_recipes(threshold);

      let source = context
        .root_source()
        .ok_or("project context does not contain a root justfile source")?;
      let original = read_utf8(source, max_editable_file_bytes()).map_err(|e| e.to_string())?;
      let mut proposed = original.clone();

      let mut similar_pairs = Vec::new();
      let mut removed = Vec::new();
      let mut merged = Vec::new();

      for (a, b, sim) in &similar {
        similar_pairs.push(json!({"recipe1": a, "recipe2": b, "similarity": sim}));

        let recipe_a = context.find_recipe(a);
        let recipe_b = context.find_recipe(b);

        if write {
          if merge {
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

      if write {
        validate_justfile(just_binary, source, &proposed).map_err(|e| e.to_string())?;
        apply_reviewed_change(source, &original, &proposed).map_err(|e| e.to_string())?;
      }

      json!({
        "similar_pairs": similar_pairs,
        "removed": removed,
        "merged": merged,
        "diff": diff,
        "dry_run": !write,
      })
    }
    _ => unreachable!("tool name validated before argument parsing"),
  };
  let text = serde_json::to_string(&value).map_err(|error| error.to_string())?;
  Ok(
    json!({ "content": [{ "type": "text", "text": text }], "structuredContent": value, "isError": false }),
  )
}

fn parse_confirmation(confirmation_value: Option<&Value>) -> Result<RunConfirmation, String> {
  let confirmation = confirmation_value
    .cloned()
    .unwrap_or_else(|| json!({ "type": "none" }));
  let conf_type = string_argument(confirmation.as_object().unwrap_or(&Map::new()), "type")?;
  match conf_type.as_str() {
    "none" => Ok(RunConfirmation::None),
    "confirmed" => Ok(RunConfirmation::Confirmed),
    "typed" => {
      let phrase = string_argument(confirmation.as_object().unwrap_or(&Map::new()), "phrase")?;
      Ok(RunConfirmation::Typed { phrase })
    }
    _ => Err(format!("unknown confirmation type `{conf_type}`")),
  }
}

fn string_argument(arguments: &Map<String, Value>, name: &str) -> Result<String, String> {
  arguments
    .get(name)
    .and_then(Value::as_str)
    .map(str::to_owned)
    .ok_or_else(|| format!("`{name}` must be a string"))
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

#[cfg(test)]
mod tests {
  use super::*;

  #[cfg(unix)]
  #[test]
  fn prepare_tool_uses_dry_run_with_internal_binary_seam() {
    use std::{fs, os::unix::fs::PermissionsExt};
    let directory = tempfile::tempdir().unwrap();
    let binary = directory.path().join("fake-just");
    fs::write(
      &binary,
      "#!/bin/sh\nif [ \"$1\" = \"--dump\" ]; then echo '{}'; exit 0; fi\n[ \"$1\" = \"--dry-run\" ] || exit 91\necho 'echo safe'\n",
    )
    .unwrap();
    let mut permissions = fs::metadata(&binary).unwrap().permissions();
    permissions.set_mode(0o700);
    fs::set_permissions(&binary, permissions).unwrap();

    let response = call_tool_at(
      &json!({
        "name":"prepare_run", "arguments": {
          "recipe":"test", "arguments":[]
        }
      }),
      binary.as_path(),
      directory.path(),
    )
    .unwrap();

    assert_eq!(response.get("isError"), Some(&Value::Bool(false)));
    assert_eq!(
      response
        .pointer("/structuredContent/preview/0")
        .and_then(Value::as_str),
      Some("echo safe")
    );
  }

  #[test]
  fn rejects_client_controlled_and_unknown_arguments() {
    let directory = tempfile::tempdir().unwrap();
    for argument in ["project_root", "just_binary", "unexpected"] {
      let response = call_tool_at(
        &json!({"name":"inspect_project", "arguments": {argument:"value"}}),
        Path::new("unused"),
        directory.path(),
      );
      assert_eq!(
        response.unwrap_err(),
        format!("unsupported argument `{argument}` for `inspect_project`")
      );
    }
  }

  #[test]
  fn rejects_non_object_arguments() {
    let directory = tempfile::tempdir().unwrap();
    let response = call_tool_at(
      &json!({"name":"doctor", "arguments": []}),
      Path::new("unused"),
      directory.path(),
    );
    assert_eq!(response.unwrap_err(), "`arguments` must be an object");
  }
}
