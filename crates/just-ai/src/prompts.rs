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
