//! Versioned product prompts.
//!
//! Keeping prompts out of provider adapters makes them directly testable and
//! allows CLI, GUI, and agent integrations to share identical contracts.

#[must_use]
pub fn suggest(project_context: &str) -> String {
  format!(
    r#"Return strict JSON with this exact shape:
{{
  "summary": "short summary",
  "recommendations": [{{
    "name": "recipe-name",
    "body": ["command line"],
    "rationale": "why this recipe is useful",
    "risk": "low|medium|high|blocked"
  }}]
}}

Recommend at most five missing just recipes. Prefer test, lint, fmt, build,
CI, dev, clean, coverage, or docs workflows. Never recommend an existing
recipe. Return JSON only. Do not execute anything.

Project context:
{project_context}"#
  )
}

#[must_use]
pub fn explain(project_context: &str, selected_recipe: &str) -> String {
  format!(
    r#"Return strict JSON with this exact shape:
{{
  "summary": "one sentence",
  "explanation": "clear explanation",
  "parameters": ["parameter explanation"],
  "dependencies": ["dependency explanation"],
  "risks": ["risk explanation"]
}}

Explain only the selected recipe. Return JSON only. Do not execute anything.

Selected recipe:
{selected_recipe}

Project context:
{project_context}"#
  )
}

#[must_use]
pub fn add(project_context: &str, request: &str) -> String {
  format!(
    r#"Return strict JSON with this exact shape:
{{
  "summary": "short summary",
  "recipe": {{
    "name": "recipe-name",
    "doc": "short doc comment or null",
    "parameters": [{{"name": "PARAMETER", "default": null}}],
    "dependencies": ["existing dependency recipe name"],
    "body": ["command line"]
  }},
  "rationale": ["reason"]
}}

Generate exactly one proposal for the user request. Use plain just syntax,
prefer low-risk commands, and reuse existing recipes only when useful. Return
JSON only. This is a proposal: do not execute anything.

User request:
{request}

Project context:
{project_context}"#
  )
}

#[must_use]
pub fn fix(project_context: &str, recipe_name: &str, history_records: &str) -> String {
  format!(
    r#"Return strict JSON with this exact shape:
{{
  "summary": "short summary",
  "recipe": {{
    "name": "recipe-name",
    "doc": "short doc comment or null",
    "parameters": [{{"name": "PARAMETER", "default": null}}],
    "dependencies": ["existing dependency recipe name"],
    "body": ["command line"]
  }},
  "rationale": ["reason"]
}}

Propose a fix for the failing recipe by modifying its body, parameters, or dependencies.
The fix should address the error patterns seen in the run history.
Return JSON only. This is a proposal: do not execute anything.

Recipe to fix: {recipe_name}

Recent failed runs (with stderr tails):
{history_records}

Project context:
{project_context}"#
  )
}

#[must_use]
pub fn workflow(project_context: &str, request: &str) -> String {
  format!(
    r#"Return strict JSON with this exact shape:
{{
  "summary": "short summary",
  "recipes": [{{
    "name": "recipe-name",
    "doc": "short doc comment or null",
    "parameters": [{{"name": "PARAMETER", "default": null}}],
    "dependencies": ["existing dependency recipe name"],
    "body": ["command line"]
  }}],
  "rationale": ["reason"],
  "execution_order": ["recipe-name"]
}}

Design a multi-recipe workflow for the user request. Create related recipes that work together with proper dependencies between them. Each recipe should have a unique name, and dependencies should reference other recipes in this workflow or existing recipes in the project. The execution_order should list recipe names in the order they should be run. Return JSON only. This is a proposal: do not execute anything.

User request:
{request}

Project context:
{project_context}"#
  )
}

#[must_use]
pub fn template(project_context: &str, request: &str) -> String {
  format!(
    r#"Return strict JSON with this exact shape:
{{
  "summary": "short summary",
  "template": {{
    "name": "template-name",
    "description": "what this template does",
    "category": "ci|build|test|deploy|docs|dev|utility",
    "parameters": [{{
      "name": "PARAMETER_NAME",
      "description": "what this parameter controls",
      "required": true,
      "default": null
    }}],
    "body": ["command line with {{PARAMETER_NAME}} placeholders"]
  }}
}}

Design a reusable just recipe template for the user request. Templates use {{PARAMETER_NAME}} placeholders in the body that get replaced when instantiated. Choose a clear category: ci, build, test, deploy, docs, dev, or utility. Parameters should have descriptions and mark if required. Return JSON only. This is a proposal: do not execute anything.

User request:
{request}

Project context:
{project_context}"#
  )
}

#[must_use]
pub fn instantiate_template(
  project_context: &str,
  template_json: &str,
  values_json: &str,
) -> String {
  format!(
    r#"Return strict JSON with this exact shape:
{{
  "summary": "short summary",
  "recipe": {{
    "name": "recipe-name",
    "doc": "short doc comment or null",
    "parameters": [{{"name": "PARAMETER", "default": null}}],
    "dependencies": ["existing dependency recipe name"],
    "body": ["command line"]
  }}
}}

Instantiate the template by replacing all {{PARAMETER_NAME}} placeholders with the provided values. The output should be a complete RecipeProposal with all placeholders resolved. Use the template body as the recipe body, substituting values. Return JSON only. This is a proposal: do not execute anything.

Template:
{template_json}

Values to substitute:
{values_json}

Project context:
{project_context}"#
  )
}

#[must_use]
pub fn compose_workflow(project_context: &str, request: &str) -> String {
  format!(
    r#"Return strict JSON with this exact shape:
{{
  "summary": "short summary",
  "recipes": [{{
    "name": "recipe-name",
    "source": "existing|new|modified",
    "doc": "short doc comment or null",
    "parameters": [{{"name": "PARAMETER", "default": null}}],
    "dependencies": ["existing dependency recipe name"],
    "body": ["command line"]
  }}],
  "rationale": ["reason"],
  "execution_order": ["recipe-name"]
}}

Compose a multi-recipe workflow from the user request by reusing EXISTING recipes when possible, modifying them if needed, and creating NEW recipes only for gaps. The "source" field must be one of: "existing" (use recipe as-is), "modified" (adapt an existing recipe), or "new" (create entirely new). Dependencies can reference other recipes in this workflow or existing recipes in the project. Return JSON only. This is a proposal: do not execute anything.

User request:
{request}

Project context:
{project_context}"#
  )
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn prompts_include_context_and_non_execution_constraint() {
    let prompt = add("{\"recipes\":[]}", "add coverage");
    assert!(prompt.contains("add coverage"));
    assert!(prompt.contains("do not execute anything"));
    assert!(prompt.contains("{\"recipes\":[]}"));
  }
}
