use {
  just_ai::{
    application::execution::{RecipeExecutor, RunRequest},
    inspect_project_at,
  },
  serde_json::{Value, json},
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

pub(super) fn call_tool(params: &Value) -> Result<Value, String> {
  let project_root = env::current_dir().map_err(|error| error.to_string())?;
  call_tool_at(params, OsStr::new("just"), &project_root)
}

fn call_tool_at(params: &Value, just_binary: &OsStr, project_root: &Path) -> Result<Value, String> {
  let name = params
    .get("name")
    .and_then(Value::as_str)
    .ok_or("tool name is required")?;
  let arguments = params.get("arguments").unwrap_or(&Value::Null);
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
    _ => return Err(format!("unknown tool `{name}`")),
  };
  let text = serde_json::to_string(&value).map_err(|error| error.to_string())?;
  Ok(
    json!({ "content": [{ "type": "text", "text": text }], "structuredContent": value, "isError": false }),
  )
}

fn string_argument(arguments: &Value, name: &str) -> Result<String, String> {
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
      "#!/bin/sh\n[ \"$1\" = \"--dry-run\" ] || exit 91\necho 'echo safe'\n",
    )
    .unwrap();
    let mut permissions = fs::metadata(&binary).unwrap().permissions();
    permissions.set_mode(0o700);
    fs::set_permissions(&binary, permissions).unwrap();

    let response = call_tool_at(
      &json!({
        "name":"prepare_run", "arguments": {
          "project_root":"/client/cannot/select/this", "just_binary":"/client/cannot/select/this",
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
}
