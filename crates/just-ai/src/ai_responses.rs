use {crate::domain::risk::RiskLevel, serde::Deserialize, serde_json::Value};

pub(crate) trait ResponseContract {
  fn schema() -> Value;
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct SuggestResponse {
  pub(crate) recommendations: Vec<SuggestRecommendation>,
  pub(crate) summary: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct SuggestRecommendation {
  pub(crate) body: Vec<String>,
  pub(crate) name: String,
  pub(crate) rationale: String,
  pub(crate) risk: RiskLevel,
}

impl ResponseContract for SuggestResponse {
  fn schema() -> Value {
    serde_json::json!({
      "type": "object", "additionalProperties": false,
      "required": ["summary", "recommendations"],
      "properties": {
        "summary": {"type": "string"},
        "recommendations": {"type": "array", "maxItems": 5, "items": {
          "type": "object", "additionalProperties": false,
          "required": ["name", "body", "rationale", "risk"],
          "properties": {
            "name": {"type": "string", "minLength": 1},
            "body": {"type": "array", "minItems": 1, "items": {"type": "string"}},
            "rationale": {"type": "string"},
            "risk": {"enum": ["low", "medium", "high", "blocked"]}
          }
        }}
      }
    })
  }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ExplainResponse {
  pub(crate) dependencies: Vec<String>,
  pub(crate) explanation: String,
  pub(crate) parameters: Vec<String>,
  pub(crate) risks: Vec<String>,
  pub(crate) summary: String,
}

impl ResponseContract for ExplainResponse {
  fn schema() -> Value {
    serde_json::json!({
      "type": "object", "additionalProperties": false,
      "required": ["summary", "explanation", "parameters", "dependencies", "risks"],
      "properties": {
        "summary": {"type": "string"}, "explanation": {"type": "string"},
        "parameters": {"type": "array", "items": {"type": "string"}},
        "dependencies": {"type": "array", "items": {"type": "string"}},
        "risks": {"type": "array", "items": {"type": "string"}}
      }
    })
  }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct AddRecipeResponse {
  pub(crate) rationale: Vec<String>,
  pub(crate) recipe: RecipeProposal,
  pub(crate) summary: String,
}

impl ResponseContract for AddRecipeResponse {
  fn schema() -> Value {
    serde_json::json!({
      "type": "object", "additionalProperties": false,
      "required": ["summary", "recipe", "rationale"],
      "properties": {
        "summary": {"type": "string"},
        "rationale": {"type": "array", "items": {"type": "string"}},
        "recipe": {
          "type": "object", "additionalProperties": false,
          "required": ["name", "doc", "parameters", "dependencies", "body"],
          "properties": {
            "name": {"type": "string", "pattern": "^[A-Za-z0-9_-]+$"},
            "doc": {"type": ["string", "null"]},
            "parameters": {"type": "array", "items": {
              "type": "object", "additionalProperties": false,
              "required": ["name", "default"],
              "properties": {"name": {"type": "string"}, "default": {"type": ["string", "null"]}}
            }},
            "dependencies": {"type": "array", "items": {"type": "string"}},
            "body": {"type": "array", "minItems": 1, "items": {"type": "string"}}
          }
        }
      }
    })
  }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct RecipeProposal {
  pub(crate) body: Vec<String>,
  #[serde(default)]
  pub(crate) dependencies: Vec<String>,
  pub(crate) doc: Option<String>,
  pub(crate) name: String,
  #[serde(default)]
  pub(crate) parameters: Vec<RecipeParameterProposal>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct RecipeParameterProposal {
  #[serde(default)]
  pub(crate) default: Option<String>,
  pub(crate) name: String,
}
