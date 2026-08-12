use {
  crate::domain::risk::RiskLevel,
  serde::{Deserialize, Serialize},
  serde_json::Value,
};

pub trait ResponseContract {
  fn schema() -> Value;
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SuggestResponse {
  pub recommendations: Vec<SuggestRecommendation>,
  pub summary: String,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SuggestRecommendation {
  pub body: Vec<String>,
  pub name: String,
  pub rationale: String,
  pub risk: RiskLevel,
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

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ExplainResponse {
  pub dependencies: Vec<String>,
  pub explanation: String,
  pub parameters: Vec<String>,
  pub risks: Vec<String>,
  pub summary: String,
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

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AddRecipeResponse {
  pub rationale: Vec<String>,
  pub recipe: RecipeProposal,
  pub summary: String,
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

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RecipeProposal {
  pub body: Vec<String>,
  #[serde(default)]
  pub dependencies: Vec<String>,
  pub doc: Option<String>,
  pub name: String,
  #[serde(default)]
  pub parameters: Vec<RecipeParameterProposal>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RecipeParameterProposal {
  #[serde(default)]
  pub default: Option<String>,
  pub name: String,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct FixResponse {
  pub rationale: Vec<String>,
  pub recipe: FixProposal,
  pub summary: String,
}

impl ResponseContract for FixResponse {
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

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct FixProposal {
  pub body: Vec<String>,
  #[serde(default)]
  pub dependencies: Vec<String>,
  pub doc: Option<String>,
  pub name: String,
  #[serde(default)]
  pub parameters: Vec<RecipeParameterProposal>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct WorkflowResponse {
  pub execution_order: Vec<String>,
  pub rationale: Vec<String>,
  pub recipes: Vec<RecipeProposal>,
  pub summary: String,
}

impl ResponseContract for WorkflowResponse {
  fn schema() -> Value {
    serde_json::json!({
      "type": "object", "additionalProperties": false,
      "required": ["summary", "recipes", "rationale", "execution_order"],
      "properties": {
        "summary": {"type": "string"},
        "rationale": {"type": "array", "items": {"type": "string"}},
        "execution_order": {"type": "array", "items": {"type": "string"}},
        "recipes": {"type": "array", "maxItems": 10, "items": {
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
        }}
      }
    })
  }
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct TemplateResponse {
  pub template: TemplateProposal,
  pub summary: String,
}

impl ResponseContract for TemplateResponse {
  fn schema() -> Value {
    serde_json::json!({
      "type": "object", "additionalProperties": false,
      "required": ["summary", "template"],
      "properties": {
        "summary": {"type": "string"},
        "template": {
          "type": "object", "additionalProperties": false,
          "required": ["name", "description", "category", "parameters", "body"],
          "properties": {
            "name": {"type": "string", "pattern": "^[A-Za-z0-9_-]+$"},
            "description": {"type": "string"},
            "category": {"type": "string"},
            "parameters": {"type": "array", "items": {
              "type": "object", "additionalProperties": false,
              "required": ["name", "description", "required"],
              "properties": {
                "name": {"type": "string", "pattern": "^[A-Za-z0-9_-]+$"},
                "description": {"type": "string"},
                "required": {"type": "boolean"},
                "default": {"type": ["string", "null"]}
              }
            }},
            "body": {"type": "array", "minItems": 1, "items": {"type": "string"}}
          }
        }
      }
    })
  }
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct TemplateProposal {
  pub body: Vec<String>,
  pub category: String,
  pub description: String,
  pub name: String,
  #[serde(default)]
  pub parameters: Vec<TemplateParameter>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct TemplateParameter {
  #[serde(default)]
  pub default: Option<String>,
  pub description: String,
  pub name: String,
  pub required: bool,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct InstantiateTemplateResponse {
  pub recipe: RecipeProposal,
  pub summary: String,
}

impl ResponseContract for InstantiateTemplateResponse {
  fn schema() -> Value {
    serde_json::json!({
      "type": "object", "additionalProperties": false,
      "required": ["summary", "recipe"],
      "properties": {
        "summary": {"type": "string"},
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

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ComposeWorkflowResponse {
  pub execution_order: Vec<String>,
  pub rationale: Vec<String>,
  pub recipes: Vec<ComposeRecipe>,
  pub summary: String,
}

impl ResponseContract for ComposeWorkflowResponse {
  fn schema() -> Value {
    serde_json::json!({
      "type": "object", "additionalProperties": false,
      "required": ["summary", "recipes", "rationale", "execution_order"],
      "properties": {
        "summary": {"type": "string"},
        "rationale": {"type": "array", "items": {"type": "string"}},
        "execution_order": {"type": "array", "items": {"type": "string"}},
        "recipes": {"type": "array", "maxItems": 10, "items": {
          "type": "object", "additionalProperties": false,
          "required": ["name", "source", "doc", "parameters", "dependencies", "body"],
          "properties": {
            "name": {"type": "string", "pattern": "^[A-Za-z0-9_-]+$"},
            "source": {"type": "string", "enum": ["existing", "new", "modified"]},
            "doc": {"type": ["string", "null"]},
            "parameters": {"type": "array", "items": {
              "type": "object", "additionalProperties": false,
              "required": ["name", "default"],
              "properties": {"name": {"type": "string"}, "default": {"type": ["string", "null"]}}
            }},
            "dependencies": {"type": "array", "items": {"type": "string"}},
            "body": {"type": "array", "minItems": 1, "items": {"type": "string"}}
          }
        }}
      }
    })
  }
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ComposeRecipe {
  pub body: Vec<String>,
  pub dependencies: Vec<String>,
  pub doc: Option<String>,
  pub name: String,
  pub parameters: Vec<RecipeParameterProposal>,
  #[serde(default)]
  pub source: String, // "existing", "new", or "modified"
}
