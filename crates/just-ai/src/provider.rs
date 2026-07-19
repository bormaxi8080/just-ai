//! Provider-neutral AI request boundary and native OpenAI-compatible adapter.

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
    let config = ureq::Agent::config_builder()
      .timeout_global(Some(Duration::from_secs(120)))
      .build();
    Self {
      agent: config.into(),
      api_key,
      base_url: base_url.into(),
      model: model.into(),
    }
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
    let mut request_builder = self
      .agent
      .post(url)
      .header("Content-Type", "application/json");
    if let Some(api_key) = &self.api_key {
      request_builder = request_builder.header("Authorization", &format!("Bearer {api_key}"));
    }
    let mut response = request_builder
      .send_json(&body)
      .map_err(|error| ProviderError::new(format!("provider request failed: {error}")))?;
    let response: Value = response
      .body_mut()
      .read_json()
      .map_err(|error| ProviderError::new(format!("invalid provider response: {error}")))?;
    response
      .pointer("/choices/0/message/content")
      .and_then(Value::as_str)
      .map(str::to_owned)
      .ok_or_else(|| ProviderError::new("provider response contains no message content"))
  }
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
}
