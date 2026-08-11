use {
  just_ai::{
    application::{
      execution::{RecipeExecutor, RunConfirmation, RunRequest},
      history::create_history,
    },
    cli::AiClient,
    config::Config,
    inspection::{inspect_project_at, ProjectContext},
    proposal::{handle_add, handle_fix},
    prompts,
  },
  serde_json::{Map, Value, json},
  std::{
    env,
    ffi::OsStr,
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

pub(super) fn call_tool(params: &Value) -> Result<Value, String> {
  let project_root = env::current_dir().map_err(|error| error.to_string())?;
  call_tool_at(params, OsStr::new("just"), &project_root)
}

fn call_tool_at(params: &Value, just_binary: &OsStr, project_root: &Path) -> Result<Value, String> {
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
      let recipe_filter = arguments.get("recipe").and_then(Value::as_str).map(str::to_owned);
      let success_filter = arguments.get("success").and_then(Value::as_bool);
      let limit = arguments
        .get("limit")
        .and_then(Value::as_u64)
        .unwrap_or(20) as usize;
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
      let context = inspect_project_at(just_binary, project_root)
        .map_err(|error| error.to_string())?;
      let client = AiClient::from_env().map_err(|error| error.to_string())?;
      let response = client
        .complete_json::<just_ai::ai_responses::AddRecipeResponse>(
          "Generate a safe just recipe proposal as strict JSON.",
          &prompts::add(&serde_json::to_string_pretty(&context).map_err(|e| e.to_string())?, &request),
        )
        .map_err(|error| error.to_string())?;
      let source = context
        .root_source()
        .ok_or("project context does not contain a root justfile source")?;
      let original = just_ai::bounded_file::read_utf8(source, just_ai::bounded_file::max_editable_file_bytes())
        .map_err(|error| error.to_string())?;
      let recipe = just_ai::proposal::render_recipe(&response.recipe);
      let proposed = just_ai::proposal::insert_recipe_grouped(
        &original,
        &recipe,
        &context,
        &response.recipe.dependencies,
        &response.recipe.name,
      );
      just_ai::bounded_file::ensure_text_limit(&proposed, "proposed justfile", just_ai::bounded_file::max_editable_file_bytes())
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
      let context = inspect_project_at(just_binary, project_root)
        .map_err(|error| error.to_string())?;
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
      let original = just_ai::bounded_file::read_utf8(source, just_ai::bounded_file::max_editable_file_bytes())
        .map_err(|error| error.to_string())?;
      let recipe = just_ai::proposal::render_fix_recipe(&response.recipe);
      let proposed = just_ai::proposal::replace_recipe(&original, &recipe_name, &recipe);
      just_ai::bounded_file::ensure_text_limit(&proposed, "proposed justfile", just_ai::bounded_file::max_editable_file_bytes())
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
    _ => unreachable!("tool name validated before argument parsing"),
  };
  let text = serde_json::to_string(&value).map_err(|error| error.to_string())?;
  Ok(
    json!({ "content": [{ "type": "text", "text": text }], "structuredContent": value, "isError": false }),
  )
}

fn parse_confirmation(confirmation_value: Option<&Value>) -> Result<RunConfirmation, String> {
  let confirmation = confirmation_value.cloned().unwrap_or_else(|| json!({ "type": "none" }));
  let conf_type = string_argument(&confirmation.as_object().unwrap_or(&Map::new()), "type")?;
  match conf_type.as_str() {
    "none" => Ok(RunConfirmation::None),
    "confirmed" => Ok(RunConfirmation::Confirmed),
    "typed" => {
      let phrase = string_argument(&confirmation.as_object().unwrap_or(&Map::new()), "phrase")?;
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
      binary.as_os_str(),
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
        OsStr::new("unused"),
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
      OsStr::new("unused"),
      directory.path(),
    );
    assert_eq!(response.unwrap_err(), "`arguments` must be an object");
  }
}
