use {
  serde_json::Value,
  std::{
    error::Error,
    fmt::{self, Display, Formatter},
    path::Path,
    process::Command,
  },
};

pub(crate) fn load_at(
  just_binary: &Path,
  project_root: Option<&Path>,
) -> Result<Value, Box<dyn Error>> {
  let mut command = Command::new(just_binary);
  command.args(["--dump", "--dump-format", "json"]);
  if let Some(project_root) = project_root {
    command.current_dir(project_root);
  }
  let output = command.output()?;
  if !output.status.success() {
    return Err(
      DumpError {
        status: output.status.to_string(),
        stderr: String::from_utf8_lossy(&output.stderr).trim().to_owned(),
      }
      .into(),
    );
  }
  Ok(serde_json::from_slice(&output.stdout)?)
}

pub(crate) fn first_function_call(value: &Value) -> Option<&str> {
  match value {
    Value::Array(items) => {
      if items.first().and_then(Value::as_str) == Some("call") {
        return items.get(1).and_then(Value::as_str);
      }
      items.iter().find_map(first_function_call)
    }
    Value::Object(fields) => fields.values().find_map(first_function_call),
    Value::Null | Value::Bool(_) | Value::Number(_) | Value::String(_) => None,
  }
}

pub(crate) fn contains_non_empty_field(value: &Value, name: &str) -> bool {
  match value {
    Value::Object(fields) => {
      fields.get(name).is_some_and(|value| match value {
        Value::Null | Value::Bool(false) => false,
        Value::String(value) => !value.is_empty(),
        Value::Array(value) => !value.is_empty(),
        Value::Object(value) => !value.is_empty(),
        Value::Bool(true) | Value::Number(_) => true,
      }) || fields
        .values()
        .any(|value| contains_non_empty_field(value, name))
    }
    Value::Array(items) => items
      .iter()
      .any(|value| contains_non_empty_field(value, name)),
    Value::Null | Value::Bool(_) | Value::Number(_) | Value::String(_) => false,
  }
}

#[derive(Debug)]
pub(crate) struct DumpError {
  pub(crate) status: String,
  pub(crate) stderr: String,
}

impl Display for DumpError {
  fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
    if self.stderr.is_empty() {
      write!(f, "just dump failed with {}", self.status)
    } else {
      write!(f, "just dump failed with {}: {}", self.status, self.stderr)
    }
  }
}

impl Error for DumpError {}

#[cfg(test)]
mod tests {
  use {super::*, serde_json::json};

  #[test]
  fn finds_nested_function_calls_but_not_function_text() {
    assert_eq!(
      first_function_call(
        &json!({"modules":{"tools":{"assignments":{"x":{"value":["call","custom","argument"]}}}}})
      ),
      Some("custom")
    );
    assert_eq!(
      first_function_call(
        &json!({"recipes":{"safe":{"body":[["echo shell()"], ["list", "call", "shell"]]}}})
      ),
      None
    );
  }

  #[test]
  fn finds_nested_non_empty_dotenv_command() {
    assert!(contains_non_empty_field(
      &json!({"modules":{"tools":{"settings":{"dotenv_command":["generate-env"]}}}}),
      "dotenv_command"
    ));
    assert!(!contains_non_empty_field(
      &json!({"settings":{"dotenv_command":[]}}),
      "dotenv_command"
    ));
  }
}
