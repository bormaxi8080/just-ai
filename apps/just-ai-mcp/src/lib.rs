use {
  just_ai::{
    application::execution::{RecipeExecutor, RunRequest},
    inspect_project_at,
  },
  serde_json::{Value, json},
  std::{
    io::{self, BufRead, Write},
    path::PathBuf,
  },
};

const PROTOCOL_VERSION: &str = "2025-11-25";
const SUPPORTED_PROTOCOL_VERSIONS: &[&str] =
  &["2024-11-05", "2025-03-26", "2025-06-18", PROTOCOL_VERSION];

struct PromptDefinition {
  name: &'static str,
  description: &'static str,
  text: &'static str,
}

struct ResourceDefinition {
  uri: &'static str,
  name: &'static str,
  description: &'static str,
  text: &'static str,
}

const PROMPTS: &[PromptDefinition] = &[
  PromptDefinition {
    name: "implement",
    description: "Implement one small, tested architecture increment.",
    text: include_str!("../../../agent/commands/implement.md"),
  },
  PromptDefinition {
    name: "review-architecture",
    description: "Review dependency direction and safety invariants.",
    text: include_str!("../../../agent/commands/review-architecture.md"),
  },
  PromptDefinition {
    name: "refresh-index",
    description: "Refresh and verify the Codebase Memory MCP graph.",
    text: include_str!("../../../agent/commands/refresh-index.md"),
  },
  PromptDefinition {
    name: "system",
    description: "Apply the just-ai maintainer invariants.",
    text: include_str!("../../../agent/prompts/system.md"),
  },
  PromptDefinition {
    name: "verify",
    description: "Run the layered verification and architecture gates.",
    text: include_str!("../../../agent/commands/verify.md"),
  },
];

const RESOURCES: &[ResourceDefinition] = &[
  ResourceDefinition {
    uri: "just-ai://docs/architecture",
    name: "Architecture",
    description: "Layer boundaries, dependency rules, and verification gates.",
    text: include_str!("../../../docs/architecture/README.md"),
  },
  ResourceDefinition {
    uri: "just-ai://docs/roadmap",
    name: "Implementation roadmap",
    description: "Completed foundation and intentionally deferred increments.",
    text: include_str!("../../../docs/architecture/roadmap.md"),
  },
  ResourceDefinition {
    uri: "just-ai://docs/adr/0001-companion-layers",
    name: "ADR 0001: Companion layers",
    description: "Decision to preserve just and build separate companion layers.",
    text: include_str!("../../../docs/architecture/adr/0001-companion-layers.md"),
  },
  ResourceDefinition {
    uri: "just-ai://docs/adr/0002-ai-is-proposal-only",
    name: "ADR 0002: AI is proposal-only",
    description: "Decision to validate AI proposals locally before applying them.",
    text: include_str!("../../../docs/architecture/adr/0002-ai-is-proposal-only.md"),
  },
  ResourceDefinition {
    uri: "just-ai://docs/adr/0003-two-phase-execution",
    name: "ADR 0003: Two-phase execution",
    description: "Decision to separate run preparation from confirmed execution.",
    text: include_str!("../../../docs/architecture/adr/0003-two-phase-execution.md"),
  },
  ResourceDefinition {
    uri: "just-ai://docs/adr/0004-native-provider-and-response-contracts",
    name: "ADR 0004: Native provider and response contracts",
    description: "Decision for native transports and schema-validated AI responses.",
    text: include_str!(
      "../../../docs/architecture/adr/0004-native-provider-and-response-contracts.md"
    ),
  },
];

pub fn run_stdio() -> io::Result<()> {
  let stdin = io::stdin();
  let mut stdout = io::stdout().lock();
  for line in stdin.lock().lines() {
    let line = line?;
    if line.trim().is_empty() {
      continue;
    }
    if let Some(response) = handle_line(&line) {
      serde_json::to_writer(&mut stdout, &response)?;
      stdout.write_all(b"\n")?;
      stdout.flush()?;
    }
  }
  Ok(())
}

fn handle_line(line: &str) -> Option<Value> {
  match serde_json::from_str::<Value>(line) {
    Ok(request) if !request.is_object() => {
      Some(protocol_error(Value::Null, -32600, "invalid request"))
    }
    Ok(request) => {
      let id = request.get("id").cloned();
      handle_request(&request)
        .or_else(|| id.map(|id| protocol_error(id, -32600, "invalid request")))
    }
    Err(error) => Some(protocol_error(
      Value::Null,
      -32700,
      &format!("parse error: {error}"),
    )),
  }
}

fn handle_request(request: &Value) -> Option<Value> {
  if request.get("jsonrpc").and_then(Value::as_str) != Some("2.0") {
    return None;
  }
  let id = request.get("id")?.clone();
  let method = request.get("method").and_then(Value::as_str)?;
  let params = request.get("params").unwrap_or(&Value::Null);
  if method == "prompts/get" {
    return Some(match get_prompt(params) {
      Ok(result) => json!({ "jsonrpc": "2.0", "id": id, "result": result }),
      Err(message) => protocol_error(id, -32602, &message),
    });
  }
  if method == "resources/read" {
    return Some(match read_resource(params) {
      Ok(result) => json!({ "jsonrpc": "2.0", "id": id, "result": result }),
      Err(ResourceReadError::InvalidParams(message)) => protocol_error(id, -32602, &message),
      Err(ResourceReadError::NotFound(message)) => protocol_error(id, -32002, &message),
    });
  }
  let result = match method {
    "initialize" => Ok(json!({
      "protocolVersion": negotiate_protocol_version(params),
      "capabilities": {
        "prompts": { "listChanged": false },
        "resources": { "subscribe": false, "listChanged": false },
        "tools": { "listChanged": false }
      },
      "serverInfo": { "name": "just-ai-mcp", "version": env!("CARGO_PKG_VERSION") }
    })),
    "ping" => Ok(json!({})),
    "prompts/list" => Ok(json!({ "prompts": prompt_definitions() })),
    "resources/list" => Ok(json!({ "resources": resource_definitions() })),
    "resources/templates/list" => Ok(json!({ "resourceTemplates": [] })),
    "tools/list" => Ok(json!({ "tools": tool_definitions() })),
    "tools/call" => call_tool(params),
    _ => return Some(protocol_error(id, -32601, "method not found")),
  };
  Some(match result {
    Ok(result) => json!({ "jsonrpc": "2.0", "id": id, "result": result }),
    Err(message) => json!({
      "jsonrpc": "2.0",
      "id": id,
      "result": { "content": [{ "type": "text", "text": message }], "isError": true }
    }),
  })
}

enum ResourceReadError {
  InvalidParams(String),
  NotFound(String),
}

fn resource_definitions() -> Value {
  Value::Array(
    RESOURCES
      .iter()
      .map(|resource| {
        json!({
          "uri": resource.uri,
          "name": resource.name,
          "description": resource.description,
          "mimeType": "text/markdown",
          "size": resource.text.len(),
          "annotations": { "audience": ["assistant"], "priority": 1.0 }
        })
      })
      .collect(),
  )
}

fn read_resource(params: &Value) -> Result<Value, ResourceReadError> {
  let uri = params
    .get("uri")
    .and_then(Value::as_str)
    .ok_or_else(|| ResourceReadError::InvalidParams("resource URI is required".into()))?;
  let resource = RESOURCES
    .iter()
    .find(|resource| resource.uri == uri)
    .ok_or_else(|| ResourceReadError::NotFound(format!("resource not found: `{uri}`")))?;
  Ok(json!({
    "contents": [{
      "uri": resource.uri,
      "mimeType": "text/markdown",
      "text": resource.text
    }]
  }))
}

fn prompt_definitions() -> Value {
  Value::Array(
    PROMPTS
      .iter()
      .map(|prompt| {
        json!({
          "name": prompt.name,
          "description": prompt.description
        })
      })
      .collect(),
  )
}

fn get_prompt(params: &Value) -> Result<Value, String> {
  let name = params
    .get("name")
    .and_then(Value::as_str)
    .ok_or("prompt name is required")?;
  if let Some(arguments) = params.get("arguments")
    && (!arguments.is_object() || arguments.as_object().is_some_and(|value| !value.is_empty()))
  {
    return Err(format!("prompt `{name}` does not accept arguments"));
  }
  let prompt = PROMPTS
    .iter()
    .find(|prompt| prompt.name == name)
    .ok_or_else(|| format!("unknown prompt `{name}`"))?;
  Ok(json!({
    "description": prompt.description,
    "messages": [{
      "role": "user",
      "content": { "type": "text", "text": prompt.text }
    }]
  }))
}

fn negotiate_protocol_version(params: &Value) -> &str {
  let requested = params.get("protocolVersion").and_then(Value::as_str);
  requested
    .filter(|version| SUPPORTED_PROTOCOL_VERSIONS.contains(version))
    .unwrap_or(PROTOCOL_VERSION)
}

fn tool_definitions() -> Value {
  json!([
    {
      "name": "inspect_project",
      "description": "Inspect recipes and deterministic risk findings through just's JSON dump without executing recipes.",
      "inputSchema": path_schema(false),
      "annotations": { "readOnlyHint": true, "destructiveHint": false }
    },
    {
      "name": "doctor",
      "description": "Return deterministic risk reports for recipes without executing them.",
      "inputSchema": path_schema(false),
      "annotations": { "readOnlyHint": true, "destructiveHint": false }
    },
    {
      "name": "prepare_run",
      "description": "Dry-run a recipe and return preview, risk, and confirmation policy. Never executes the recipe.",
      "inputSchema": path_schema(true),
      "annotations": { "readOnlyHint": true, "destructiveHint": false }
    }
  ])
}

fn path_schema(include_recipe: bool) -> Value {
  let mut properties = json!({
    "project_root": { "type": "string" },
    "just_binary": { "type": "string", "default": "just" }
  });
  let mut required = vec!["project_root"];
  if include_recipe {
    properties["recipe"] = json!({ "type": "string" });
    properties["arguments"] =
      json!({ "type": "array", "items": { "type": "string" }, "default": [] });
    required.push("recipe");
  }
  json!({ "type": "object", "properties": properties, "required": required, "additionalProperties": false })
}

fn call_tool(params: &Value) -> Result<Value, String> {
  let name = params
    .get("name")
    .and_then(Value::as_str)
    .ok_or("tool name is required")?;
  let arguments = params.get("arguments").unwrap_or(&Value::Null);
  let project_root = string_argument(arguments, "project_root")?;
  let just_binary = arguments
    .get("just_binary")
    .and_then(Value::as_str)
    .unwrap_or("just");
  let value = match name {
    "inspect_project" => serde_json::to_value(
      inspect_project_at(just_binary, &project_root).map_err(|error| error.to_string())?,
    )
    .map_err(|error| error.to_string())?,
    "doctor" => {
      let context =
        inspect_project_at(just_binary, &project_root).map_err(|error| error.to_string())?;
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

fn protocol_error(id: Value, code: i32, message: &str) -> Value {
  json!({ "jsonrpc": "2.0", "id": id, "error": { "code": code, "message": message } })
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn tool_list_contains_only_read_only_operations() {
    let response = handle_request(&json!({"jsonrpc":"2.0","id":1,"method":"tools/list"})).unwrap();
    let tools = response
      .pointer("/result/tools")
      .unwrap()
      .as_array()
      .unwrap();
    assert_eq!(tools.len(), 3);
    assert!(
      tools
        .iter()
        .all(|tool| tool.pointer("/annotations/readOnlyHint") == Some(&Value::Bool(true)))
    );
    assert!(
      tools
        .iter()
        .all(|tool| tool.get("name").and_then(Value::as_str) != Some("execute_run"))
    );
  }

  #[test]
  fn notifications_produce_no_stdout_message() {
    assert!(
      handle_request(&json!({"jsonrpc":"2.0","method":"notifications/initialized"})).is_none()
    );
  }

  #[test]
  fn initialize_negotiates_supported_protocol_and_declares_capabilities() {
    let response = handle_request(&json!({
      "jsonrpc":"2.0", "id":"init", "method":"initialize", "params": {
        "protocolVersion":"2024-11-05", "capabilities": {},
        "clientInfo": {"name":"test", "version":"1"}
      }
    }))
    .unwrap();
    assert_eq!(
      response
        .pointer("/result/protocolVersion")
        .and_then(Value::as_str),
      Some("2024-11-05")
    );
    assert_eq!(
      response.pointer("/result/capabilities/tools/listChanged"),
      Some(&Value::Bool(false))
    );
    assert_eq!(
      response.pointer("/result/capabilities/prompts/listChanged"),
      Some(&Value::Bool(false))
    );
    assert_eq!(
      response.pointer("/result/capabilities/resources/subscribe"),
      Some(&Value::Bool(false))
    );
  }

  #[test]
  fn resource_list_exposes_only_canonical_architecture_documents() {
    let response =
      handle_request(&json!({"jsonrpc":"2.0","id":1,"method":"resources/list"})).unwrap();
    let resources = response
      .pointer("/result/resources")
      .and_then(Value::as_array)
      .unwrap();
    assert_eq!(resources.len(), 6);
    assert!(resources.iter().all(|resource| {
      resource
        .get("uri")
        .and_then(Value::as_str)
        .is_some_and(|uri| uri.starts_with("just-ai://docs/"))
    }));
    assert!(resources.iter().all(|resource| {
      resource.get("mimeType").and_then(Value::as_str) == Some("text/markdown")
    }));
  }

  #[test]
  fn resource_read_returns_embedded_canonical_text() {
    let response = handle_request(&json!({
      "jsonrpc":"2.0", "id":1, "method":"resources/read",
      "params":{"uri":"just-ai://docs/architecture"}
    }))
    .unwrap();
    assert_eq!(
      response
        .pointer("/result/contents/0/text")
        .and_then(Value::as_str),
      Some(include_str!("../../../docs/architecture/README.md"))
    );
  }

  #[test]
  fn resource_templates_are_explicitly_empty() {
    let response =
      handle_request(&json!({"jsonrpc":"2.0","id":1,"method":"resources/templates/list"})).unwrap();
    assert_eq!(
      response.pointer("/result/resourceTemplates"),
      Some(&json!([]))
    );
  }

  #[test]
  fn unknown_resource_cannot_escape_static_allowlist() {
    let response = handle_request(&json!({
      "jsonrpc":"2.0", "id":1, "method":"resources/read",
      "params":{"uri":"file:///etc/passwd"}
    }))
    .unwrap();
    assert_eq!(
      response.pointer("/error/code").and_then(Value::as_i64),
      Some(-32002)
    );
    assert!(response.get("result").is_none());
  }

  #[test]
  fn prompt_list_exposes_canonical_agent_commands() {
    let response =
      handle_request(&json!({"jsonrpc":"2.0","id":1,"method":"prompts/list"})).unwrap();
    let prompts = response
      .pointer("/result/prompts")
      .and_then(Value::as_array)
      .unwrap();
    let names = prompts
      .iter()
      .filter_map(|prompt| prompt.get("name").and_then(Value::as_str))
      .collect::<Vec<_>>();
    assert_eq!(
      names,
      [
        "implement",
        "review-architecture",
        "refresh-index",
        "system",
        "verify"
      ]
    );
    assert!(
      prompts
        .iter()
        .all(|prompt| prompt.get("arguments").is_none())
    );
  }

  #[test]
  fn prompt_get_returns_embedded_canonical_text() {
    for (name, expected) in [
      (
        "implement",
        include_str!("../../../agent/commands/implement.md"),
      ),
      ("verify", include_str!("../../../agent/commands/verify.md")),
    ] {
      let response = handle_request(&json!({
        "jsonrpc":"2.0", "id":1, "method":"prompts/get",
        "params":{"name":name, "arguments":{}}
      }))
      .unwrap();
      assert_eq!(
        response
          .pointer("/result/messages/0/content/text")
          .and_then(Value::as_str),
        Some(expected)
      );
      assert_eq!(
        response
          .pointer("/result/messages/0/role")
          .and_then(Value::as_str),
        Some("user")
      );
    }
  }

  #[test]
  fn invalid_prompt_request_returns_invalid_params() {
    for params in [
      json!({"name":"missing"}),
      json!({"name":"implement", "arguments":{"untrusted":"text"}}),
    ] {
      let response =
        handle_request(&json!({"jsonrpc":"2.0","id":1,"method":"prompts/get","params":params}))
          .unwrap();
      assert_eq!(
        response.pointer("/error/code").and_then(Value::as_i64),
        Some(-32602)
      );
      assert!(response.get("result").is_none());
    }
  }

  #[test]
  fn unsupported_protocol_falls_back_to_latest() {
    let response = handle_request(&json!({
      "jsonrpc":"2.0", "id":1, "method":"initialize",
      "params":{"protocolVersion":"2099-01-01"}
    }))
    .unwrap();
    assert_eq!(
      response
        .pointer("/result/protocolVersion")
        .and_then(Value::as_str),
      Some(PROTOCOL_VERSION)
    );
  }

  #[test]
  fn malformed_request_returns_json_rpc_error() {
    let response = handle_line(r#"{"jsonrpc":"2.0","id":7}"#).unwrap();
    assert_eq!(
      response.pointer("/error/code").and_then(Value::as_i64),
      Some(-32600)
    );
    assert_eq!(response.get("id"), Some(&json!(7)));
    let scalar = handle_line("42").unwrap();
    assert_eq!(
      scalar.pointer("/error/code").and_then(Value::as_i64),
      Some(-32600)
    );
  }

  #[cfg(unix)]
  #[test]
  fn prepare_tool_uses_dry_run() {
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
    let response = handle_request(&json!({
      "jsonrpc":"2.0", "id":2, "method":"tools/call", "params": {
        "name":"prepare_run", "arguments": {
          "project_root": directory.path(), "just_binary": binary, "recipe":"test", "arguments":[]
        }
      }
    }))
    .unwrap();
    assert_eq!(
      response.pointer("/result/isError"),
      Some(&Value::Bool(false))
    );
    assert_eq!(
      response
        .pointer("/result/structuredContent/preview/0")
        .and_then(Value::as_str),
      Some("echo safe")
    );
  }
}
