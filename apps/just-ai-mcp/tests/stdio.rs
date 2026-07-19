use {
  serde_json::{Value, json},
  std::{
    io::Write,
    process::{Command, Stdio},
  },
};

fn run_server(lines: &[String]) -> Vec<Value> {
  let mut child = Command::new(env!("CARGO_BIN_EXE_just-ai-mcp"))
    .stdin(Stdio::piped())
    .stdout(Stdio::piped())
    .stderr(Stdio::piped())
    .spawn()
    .unwrap();
  {
    let mut stdin = child.stdin.take().unwrap();
    for line in lines {
      writeln!(stdin, "{line}").unwrap();
    }
  }
  let output = child.wait_with_output().unwrap();
  assert!(
    output.status.success(),
    "server stderr: {}",
    String::from_utf8_lossy(&output.stderr)
  );
  let stdout = String::from_utf8(output.stdout).unwrap();
  stdout
    .lines()
    .map(|line| serde_json::from_str(line).unwrap())
    .collect()
}

#[test]
fn stdio_is_protocol_only_and_notifications_are_silent() {
  let requests = [
    json!({
      "jsonrpc":"2.0", "id":"init", "method":"initialize", "params": {
        "protocolVersion":"2025-11-25", "capabilities": {},
        "clientInfo":{"name":"integration-test", "version":"1"}
      }
    }),
    json!({"jsonrpc":"2.0", "method":"notifications/initialized"}),
    json!({"jsonrpc":"2.0", "id":2, "method":"ping"}),
    json!({"jsonrpc":"2.0", "id":3, "method":"resources/list"}),
  ]
  .map(|request| serde_json::to_string(&request).unwrap());

  let responses = run_server(&requests);

  assert_eq!(responses.len(), 3);
  assert_eq!(responses[0].get("id"), Some(&json!("init")));
  assert_eq!(responses[1].get("id"), Some(&json!(2)));
  assert_eq!(responses[2].get("id"), Some(&json!(3)));
  assert_eq!(
    responses[2]
      .pointer("/result/resources")
      .and_then(Value::as_array)
      .map(Vec::len),
    Some(6)
  );
}

#[test]
fn stdio_recovers_after_a_parse_error() {
  let requests = [
    "{".to_owned(),
    serde_json::to_string(&json!({"jsonrpc":"2.0", "id":9, "method":"ping"})).unwrap(),
  ];

  let responses = run_server(&requests);

  assert_eq!(responses.len(), 2);
  assert_eq!(
    responses[0].pointer("/error/code").and_then(Value::as_i64),
    Some(-32700)
  );
  assert_eq!(responses[1].get("id"), Some(&json!(9)));
  assert_eq!(responses[1].get("result"), Some(&json!({})));
}
