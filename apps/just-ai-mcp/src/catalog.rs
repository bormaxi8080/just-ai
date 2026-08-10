use serde_json::{Value, json};

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
  ResourceDefinition {
    uri: "just-ai://docs/adr/0005-cross-platform-process-tree-cancellation",
    name: "ADR 0005: Cross-platform process-tree cancellation",
    description: "Decision for Unix process groups and Windows Job Objects.",
    text: include_str!(
      "../../../docs/architecture/adr/0005-cross-platform-process-tree-cancellation.md"
    ),
  },
];

pub(super) enum ResourceReadError {
  InvalidParams(String),
  NotFound(String),
}

pub(super) fn resource_definitions() -> Value {
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

pub(super) fn read_resource(params: &Value) -> Result<Value, ResourceReadError> {
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

pub(super) fn prompt_definitions() -> Value {
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

pub(super) fn get_prompt(params: &Value) -> Result<Value, String> {
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
