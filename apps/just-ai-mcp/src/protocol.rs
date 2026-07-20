use {
  crate::{
    catalog::{
      ResourceReadError, get_prompt, prompt_definitions, read_resource, resource_definitions,
    },
    tools::{call_tool, tool_definitions},
  },
  serde_json::{Value, json},
};

const PROTOCOL_VERSION: &str = "2025-11-25";
const SUPPORTED_PROTOCOL_VERSIONS: &[&str] =
  &["2024-11-05", "2025-03-26", "2025-06-18", PROTOCOL_VERSION];

pub(super) fn handle_message(message: &[u8]) -> Option<Value> {
  match serde_json::from_slice::<Value>(message) {
    Ok(request) if !request.is_object() => {
      Some(protocol_error(Value::Null, -32600, "invalid request"))
    }
    Ok(request) => handle_request(&request),
    Err(error) => Some(protocol_error(
      Value::Null,
      -32700,
      &format!("parse error: {error}"),
    )),
  }
}

pub(super) fn oversized_request() -> Value {
  protocol_error(Value::Null, -32600, "request exceeds maximum size")
}

fn handle_request(request: &Value) -> Option<Value> {
  let id = match request.get("id") {
    None => None,
    Some(id @ (Value::Null | Value::String(_) | Value::Number(_))) => Some(id.clone()),
    Some(_) => return Some(protocol_error(Value::Null, -32600, "invalid request")),
  };
  let Some(method) = request.get("method").and_then(Value::as_str) else {
    return Some(protocol_error(
      id.unwrap_or(Value::Null),
      -32600,
      "invalid request",
    ));
  };
  if request.get("jsonrpc").and_then(Value::as_str) != Some("2.0")
    || request
      .get("params")
      .is_some_and(|params| !params.is_object() && !params.is_array())
  {
    return Some(protocol_error(
      id.unwrap_or(Value::Null),
      -32600,
      "invalid request",
    ));
  }
  let id = id?;
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

fn negotiate_protocol_version(params: &Value) -> &str {
  let requested = params.get("protocolVersion").and_then(Value::as_str);
  requested
    .filter(|version| SUPPORTED_PROTOCOL_VERSIONS.contains(version))
    .unwrap_or(PROTOCOL_VERSION)
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
    assert!(tools.iter().all(|tool| {
      tool
        .pointer("/inputSchema/properties/just_binary")
        .is_none()
    }));
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
    assert_eq!(resources.len(), 7);
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
  fn malformed_requests_return_json_rpc_errors() {
    let missing_method = handle_message(br#"{"jsonrpc":"2.0","id":7}"#).unwrap();
    assert_eq!(
      missing_method
        .pointer("/error/code")
        .and_then(Value::as_i64),
      Some(-32600)
    );
    assert_eq!(missing_method.get("id"), Some(&json!(7)));

    let invalid_notification = handle_message(br#"{"jsonrpc":"1.0","method":"ping"}"#).unwrap();
    assert_eq!(
      invalid_notification
        .pointer("/error/code")
        .and_then(Value::as_i64),
      Some(-32600)
    );
    assert_eq!(invalid_notification.get("id"), Some(&Value::Null));

    let invalid_id = handle_message(br#"{"jsonrpc":"2.0","id":true,"method":"ping"}"#).unwrap();
    assert_eq!(
      invalid_id.pointer("/error/code").and_then(Value::as_i64),
      Some(-32600)
    );
    assert_eq!(invalid_id.get("id"), Some(&Value::Null));

    let scalar = handle_message(b"42").unwrap();
    assert_eq!(
      scalar.pointer("/error/code").and_then(Value::as_i64),
      Some(-32600)
    );

    let parse_error = handle_message(b"{").unwrap();
    assert_eq!(
      parse_error.pointer("/error/code").and_then(Value::as_i64),
      Some(-32700)
    );
    assert_eq!(parse_error.get("id"), Some(&Value::Null));
  }

  #[test]
  fn unknown_method_preserves_request_id() {
    let response =
      handle_message(br#"{"jsonrpc":"2.0","id":"unknown","method":"missing"}"#).unwrap();
    assert_eq!(
      response.pointer("/error/code").and_then(Value::as_i64),
      Some(-32601)
    );
    assert_eq!(response.get("id"), Some(&json!("unknown")));
  }
}
