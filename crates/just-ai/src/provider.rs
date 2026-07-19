//! Provider-neutral AI boundary with native OpenAI, Ollama, and compatible adapters.

use {
  serde_json::Value,
  std::{
    error::Error,
    fmt::{self, Display, Formatter},
    time::Duration,
  },
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AiRequest {
  pub system: String,
  pub user: String,
  pub schema_name: String,
  pub schema: Value,
}

#[derive(Clone, Debug)]
pub struct OpenAiResponsesProvider {
  agent: ureq::Agent,
  api_key: String,
  base_url: String,
  model: String,
}

#[derive(Clone, Debug)]
pub struct OllamaProvider {
  agent: ureq::Agent,
  api_key: Option<String>,
  base_url: String,
  model: String,
}

impl OllamaProvider {
  #[must_use]
  pub fn new(
    base_url: impl Into<String>,
    model: impl Into<String>,
    api_key: Option<String>,
  ) -> Self {
    Self {
      agent: provider_agent(),
      api_key,
      base_url: base_url.into(),
      model: model.into(),
    }
  }
}

impl OpenAiResponsesProvider {
  #[must_use]
  pub fn new(
    base_url: impl Into<String>,
    model: impl Into<String>,
    api_key: impl Into<String>,
  ) -> Self {
    Self {
      agent: provider_agent(),
      api_key: api_key.into(),
      base_url: base_url.into(),
      model: model.into(),
    }
  }
}

pub trait AiProvider {
  fn complete(&self, request: &AiRequest) -> Result<String, ProviderError>;
}

#[derive(Clone, Debug)]
pub struct OpenAiCompatibleProvider {
  agent: ureq::Agent,
  api_key: Option<String>,
  base_url: String,
  model: String,
}

impl OpenAiCompatibleProvider {
  #[must_use]
  pub fn new(
    base_url: impl Into<String>,
    model: impl Into<String>,
    api_key: Option<String>,
  ) -> Self {
    Self {
      agent: provider_agent(),
      api_key,
      base_url: base_url.into(),
      model: model.into(),
    }
  }
}

impl AiProvider for OpenAiResponsesProvider {
  fn complete(&self, request: &AiRequest) -> Result<String, ProviderError> {
    let url = format!("{}/responses", self.base_url.trim_end_matches('/'));
    let body = serde_json::json!({
      "model": self.model,
      "reasoning": { "effort": "none" },
      "instructions": request.system,
      "input": request.user,
      "text": { "format": {
        "type": "json_schema",
        "name": request.schema_name,
        "strict": true,
        "schema": request.schema
      }}
    });
    let response = post_json(&self.agent, &url, Some(&self.api_key), &body)?;
    if response.get("status").and_then(Value::as_str) != Some("completed") {
      return Err(ProviderError::new(format!(
        "OpenAI response did not complete: {}",
        response
          .get("status")
          .and_then(Value::as_str)
          .unwrap_or("missing status")
      )));
    }
    response
      .get("output")
      .and_then(Value::as_array)
      .into_iter()
      .flatten()
      .filter_map(|item| item.get("content").and_then(Value::as_array))
      .flatten()
      .find_map(|content| {
        (content.get("type").and_then(Value::as_str) == Some("output_text"))
          .then(|| content.get("text").and_then(Value::as_str))
          .flatten()
      })
      .map(str::to_owned)
      .ok_or_else(|| ProviderError::new("OpenAI response contains no output_text content"))
  }
}

impl AiProvider for OllamaProvider {
  fn complete(&self, request: &AiRequest) -> Result<String, ProviderError> {
    let url = format!("{}/api/chat", self.base_url.trim_end_matches('/'));
    let body = serde_json::json!({
      "model": self.model,
      "messages": [
        { "role": "system", "content": request.system },
        { "role": "user", "content": request.user }
      ],
      "format": request.schema,
      "stream": false,
      "options": { "temperature": 0 }
    });
    let response = post_json(&self.agent, &url, self.api_key.as_deref(), &body)?;
    if response.get("done").and_then(Value::as_bool) != Some(true) {
      return Err(ProviderError::new(
        "Ollama response did not finish as a non-streaming response",
      ));
    }
    response
      .pointer("/message/content")
      .and_then(Value::as_str)
      .map(str::to_owned)
      .ok_or_else(|| ProviderError::new("Ollama response contains no message content"))
  }
}

impl AiProvider for OpenAiCompatibleProvider {
  fn complete(&self, request: &AiRequest) -> Result<String, ProviderError> {
    let url = format!("{}/chat/completions", self.base_url.trim_end_matches('/'));
    let body = serde_json::json!({
      "model": self.model,
      "messages": [
        { "role": "system", "content": request.system },
        { "role": "user", "content": request.user }
      ],
      "response_format": { "type": "json_object" }
    });
    let response = post_json(&self.agent, &url, self.api_key.as_deref(), &body)?;
    response
      .pointer("/choices/0/message/content")
      .and_then(Value::as_str)
      .map(str::to_owned)
      .ok_or_else(|| ProviderError::new("provider response contains no message content"))
  }
}

fn provider_agent() -> ureq::Agent {
  ureq::Agent::config_builder()
    .timeout_global(Some(Duration::from_secs(120)))
    .build()
    .into()
}

fn post_json(
  agent: &ureq::Agent,
  url: &str,
  api_key: Option<&str>,
  body: &Value,
) -> Result<Value, ProviderError> {
  let mut request = agent.post(url).header("Content-Type", "application/json");
  if let Some(api_key) = api_key {
    request = request.header("Authorization", &format!("Bearer {api_key}"));
  }
  let mut response = request
    .send_json(body)
    .map_err(|error| ProviderError::new(format!("provider request failed: {error}")))?;
  response
    .body_mut()
    .read_json()
    .map_err(|error| ProviderError::new(format!("invalid provider response: {error}")))
}

#[derive(Debug)]
pub struct ProviderError {
  message: String,
}

impl ProviderError {
  #[must_use]
  pub fn new(message: impl Into<String>) -> Self {
    Self {
      message: message.into(),
    }
  }
}

impl Display for ProviderError {
  fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
    f.write_str(&self.message)
  }
}

impl Error for ProviderError {}

#[cfg(test)]
mod tests {
  use super::*;
  use std::{
    io::{Read, Write},
    net::TcpListener,
    thread,
  };

  #[test]
  fn missing_choices_is_an_error() {
    let value = serde_json::json!({"choices": []});
    assert!(value.pointer("/choices/0/message/content").is_none());
  }

  #[test]
  fn native_provider_sends_authorization_and_reads_content() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let server = thread::spawn(move || {
      let (mut stream, _) = listener.accept().unwrap();
      let mut request = Vec::new();
      let mut buffer = [0_u8; 1024];
      let header_end = loop {
        let read = stream.read(&mut buffer).unwrap();
        request.extend_from_slice(&buffer[..read]);
        if let Some(position) = request.windows(4).position(|part| part == b"\r\n\r\n") {
          break position + 4;
        }
      };
      let headers = String::from_utf8_lossy(&request[..header_end]);
      let content_length = headers
        .lines()
        .find_map(|line| {
          line
            .to_ascii_lowercase()
            .strip_prefix("content-length: ")
            .map(str::to_owned)
        })
        .unwrap()
        .parse::<usize>()
        .unwrap();
      while request.len() - header_end < content_length {
        let read = stream.read(&mut buffer).unwrap();
        request.extend_from_slice(&buffer[..read]);
      }
      let response = r#"{"choices":[{"message":{"content":"{\"summary\":\"ok\"}"}}]}"#;
      write!(
        stream,
        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        response.len(), response
      )
      .unwrap();
      String::from_utf8(request).unwrap()
    });

    let provider = OpenAiCompatibleProvider::new(
      format!("http://{address}/v1"),
      "test-model",
      Some("test-secret".into()),
    );
    let content = provider
      .complete(&AiRequest {
        system: "system".into(),
        user: "user".into(),
        schema_name: "test_response".into(),
        schema: serde_json::json!({"type": "object"}),
      })
      .unwrap();
    assert_eq!(content, "{\"summary\":\"ok\"}");
    let request = server.join().unwrap();
    assert!(request.contains("POST /v1/chat/completions"));
    assert!(
      request.contains("authorization: Bearer test-secret")
        || request.contains("Authorization: Bearer test-secret")
    );
    assert!(request.contains("test-model"));
  }

  #[test]
  fn responses_provider_sends_strict_schema_and_reads_output_text() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let server = thread::spawn(move || {
      let (mut stream, _) = listener.accept().unwrap();
      let request = read_http_request(&mut stream);
      let response = r#"{"status":"completed","output":[{"type":"reasoning","content":[]},{"type":"message","content":[{"type":"output_text","text":"{\"summary\":\"ok\"}"}]}]}"#;
      write_http_response(&mut stream, response);
      request
    });

    let provider =
      OpenAiResponsesProvider::new(format!("http://{address}/v1"), "gpt-test", "test-secret");
    let content = provider
      .complete(&AiRequest {
        system: "system".into(),
        user: "user".into(),
        schema_name: "suggest_response".into(),
        schema: serde_json::json!({
          "type": "object",
          "additionalProperties": false,
          "properties": {"summary": {"type": "string"}},
          "required": ["summary"]
        }),
      })
      .unwrap();
    assert_eq!(content, "{\"summary\":\"ok\"}");
    let request = server.join().unwrap();
    assert!(request.contains("POST /v1/responses"));
    let body: Value = serde_json::from_str(request.split("\r\n\r\n").nth(1).unwrap()).unwrap();
    assert_eq!(
      body.pointer("/text/format/type").and_then(Value::as_str),
      Some("json_schema")
    );
    assert_eq!(
      body.pointer("/text/format/strict").and_then(Value::as_bool),
      Some(true)
    );
    assert_eq!(
      body.pointer("/text/format/name").and_then(Value::as_str),
      Some("suggest_response")
    );
    assert_eq!(
      body.pointer("/reasoning/effort").and_then(Value::as_str),
      Some("none")
    );
    assert!(
      request.contains("authorization: Bearer test-secret")
        || request.contains("Authorization: Bearer test-secret")
    );
  }

  #[test]
  fn ollama_provider_sends_native_schema_and_disables_streaming() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let server = thread::spawn(move || {
      let (mut stream, _) = listener.accept().unwrap();
      let request = read_http_request(&mut stream);
      write_http_response(
        &mut stream,
        r#"{"model":"test","message":{"role":"assistant","content":"{\"summary\":\"local\"}"},"done":true}"#,
      );
      request
    });

    let provider = OllamaProvider::new(format!("http://{address}"), "local-model", None);
    let content = provider
      .complete(&AiRequest {
        system: "system".into(),
        user: "user".into(),
        schema_name: "unused_by_ollama".into(),
        schema: serde_json::json!({
          "type": "object",
          "properties": {"summary": {"type": "string"}},
          "required": ["summary"]
        }),
      })
      .unwrap();
    assert_eq!(content, "{\"summary\":\"local\"}");

    let request = server.join().unwrap();
    assert!(request.contains("POST /api/chat"));
    let body: Value = serde_json::from_str(request.split("\r\n\r\n").nth(1).unwrap()).unwrap();
    assert_eq!(body.get("stream").and_then(Value::as_bool), Some(false));
    assert_eq!(
      body
        .pointer("/format/properties/summary/type")
        .and_then(Value::as_str),
      Some("string")
    );
    assert_eq!(
      body.pointer("/options/temperature").and_then(Value::as_i64),
      Some(0)
    );
  }

  fn read_http_request(stream: &mut std::net::TcpStream) -> String {
    let mut request = Vec::new();
    let mut buffer = [0_u8; 1024];
    let header_end = loop {
      let read = stream.read(&mut buffer).unwrap();
      request.extend_from_slice(&buffer[..read]);
      if let Some(position) = request.windows(4).position(|part| part == b"\r\n\r\n") {
        break position + 4;
      }
    };
    let headers = String::from_utf8_lossy(&request[..header_end]);
    let content_length = headers
      .lines()
      .find_map(|line| {
        line
          .to_ascii_lowercase()
          .strip_prefix("content-length: ")
          .map(str::to_owned)
      })
      .unwrap()
      .parse::<usize>()
      .unwrap();
    while request.len() - header_end < content_length {
      let read = stream.read(&mut buffer).unwrap();
      request.extend_from_slice(&buffer[..read]);
    }
    String::from_utf8(request).unwrap()
  }

  fn write_http_response(stream: &mut std::net::TcpStream, response: &str) {
    write!(
      stream,
      "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
      response.len(), response
    )
    .unwrap();
  }
}
