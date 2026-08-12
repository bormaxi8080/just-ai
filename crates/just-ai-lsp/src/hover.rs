//! Hover provider for justfiles

use crate::analysis::ProjectAnalysis;
use lsp_types::*;

/// Provide hover information for justfiles
pub fn provide_hover(
  text: &str,
  position: Position,
  analysis: Option<&ProjectAnalysis>,
) -> Option<Hover> {
  let line = text.lines().nth(position.line as usize)?;
  let char_idx = position.character as usize;
  let word = extract_word_at(line, char_idx)?;

  // 1. Recipe hover
  if let Some(analysis) = analysis {
    if let Some(recipe) = analysis.recipes.get(&word) {
      return Some(recipe_hover(recipe));
    }

    // Check namepath matches
    for (namepath, recipe) in &analysis.recipes {
      if namepath.as_str() == word.as_str() || recipe.name == word {
        return Some(recipe_hover(recipe));
      }
    }
  }

  // 2. Built-in function hover
  if let Some(func_hover) = builtin_function_hover(&word) {
    return Some(func_hover);
  }

  // 3. Setting hover
  if let Some(setting_hover) = setting_hover(&word) {
    return Some(setting_hover);
  }

  // 4. Variable hover
  if let Some(var_hover) = variable_hover(&word, analysis) {
    return Some(var_hover);
  }

  // 5. Parameter kind hover
  if matches!(word.as_str(), "plus" | "star" | "singular") {
    return parameter_kind_hover(&word);
  }

  None
}

/// Generate hover for a recipe
fn recipe_hover(recipe: &crate::analysis::RecipeInfo) -> Hover {
  let mut markdown = String::new();

  // Title
  markdown.push_str(&format!("## Recipe: `{}`\n\n", recipe.namepath));

  // Documentation
  if let Some(doc) = &recipe.doc {
    markdown.push_str(doc);
    markdown.push_str("\n\n");
  }

  // Risk
  markdown.push_str(&format!("**Risk:** `{}`\n\n", recipe.risk));

  // Parameters
  if !recipe.parameters.is_empty() {
    markdown.push_str("**Parameters:**\n\n");
    for param in &recipe.parameters {
      let kind_str = if param.is_variadic {
        if param.kind == "plus" {
          "one or more"
        } else {
          "zero or more"
        }
      } else if let Some(default) = &param.default {
        let default_preview = if default.len() > 30 {
          &default[..30]
        } else {
          default
        };
        &format!("default: `{}`", default_preview)
      } else {
        "required"
      };
      markdown.push_str(&format!("- `{}:** {}\n", param.name, kind_str));
    }
    markdown.push('\n');
  }

  // Dependencies
  if !recipe.dependencies.is_empty() {
    markdown.push_str("**Dependencies:** ");
    markdown.push_str(&recipe.dependencies.join(", "));
    markdown.push_str("\n\n");
  }

  // Body
  if !recipe.body.is_empty() {
    markdown.push_str("**Body:**\n```just\n");
    markdown.push_str(&recipe.body.join("\n"));
    markdown.push('\n');
    markdown.push_str("```\n\n");
  }

  // Risks
  if !recipe.risks.is_empty() {
    markdown.push_str("**Risk Findings:**\n\n");
    for risk in &recipe.risks {
      markdown.push_str(&format!(
        "- **{}**: {}\n",
        risk.level.to_uppercase(),
        risk.reason
      ));
    }
  }

  Hover {
    contents: HoverContents::Markup(MarkupContent {
      kind: MarkupKind::Markdown,
      value: markdown,
    }),
    range: None,
  }
}

/// Hover for built-in functions
fn builtin_function_hover(word: &str) -> Option<Hover> {
  let docs = [
        ("arch", "Returns the target architecture (e.g., `x86_64`, `aarch64`)."),
        ("env_var", "Get an environment variable. Returns empty string if not set.\n\n`env_var(\"NAME\")`"),
        ("env_var_or_default", "Get an environment variable with a default value.\n\n`env_var_or_default(\"NAME\", \"default\")`"),
        ("num_cpus", "Returns the number of logical CPUs."),
        ("os", "Returns the operating system name (e.g., `linux`, `macos`, `windows`)."),
        ("os_family", "Returns the OS family: `unix` or `windows`."),
        ("quote", "Shell-quote a string for safe interpolation."),
        ("shell", "Run a shell command and capture its output.\n\n`shell(\"command\", \"arg1\", \"arg2\")`"),
        ("which", "Find an executable in PATH. Returns empty string if not found.\n\n`which(\"command\")`"),
    ];

  for (name, doc) in docs {
    if name == word {
      return Some(Hover {
        contents: HoverContents::Markup(MarkupContent {
          kind: MarkupKind::Markdown,
          value: format!("## Function: `{}`\n\n{}", name, doc),
        }),
        range: None,
      });
    }
  }
  None
}

/// Hover for settings
fn setting_hover(word: &str) -> Option<Hover> {
  let settings = [
        ("shell", "Set the default shell for recipes.\n\nExample: `set shell := [\"bash\", \"-cu\"]`"),
        ("windows_shell", "Set the shell on Windows.\n\nExample: `set windows_shell := [\"powershell.exe\", \"-c\"]`"),
        ("script_interpreter", "Set the interpreter for script recipes.\n\nExample: `set script_interpreter := [\"python3\"]`"),
        ("positional_arguments", "Use positional arguments ($1, $2, ...) instead of named parameters."),
        ("export", "Export all variables as environment variables."),
        ("dotenv_load", "Load .env files (default: true)."),
        ("dotenv_filename", "Custom filename for .env file."),
        ("dotenv_path", "Custom path to .env file."),
        ("dotenv_required", "Require .env file to exist."),
        ("allow_duplicate_recipes", "Allow multiple recipes with the same name."),
        ("allow_duplicate_variables", "Allow multiple variables with the same name."),
        ("unstable", "Enable unstable features."),
        ("lists", "Enable list support (unstable)."),
        ("temporary_directory", "Set the temporary directory for script/shebang recipes."),
    ];

  for (name, doc) in settings {
    if name == word || format!("set {}", name) == word {
      return Some(Hover {
        contents: HoverContents::Markup(MarkupContent {
          kind: MarkupKind::Markdown,
          value: format!("## Setting: `set {}`\n\n{}", name, doc),
        }),
        range: None,
      });
    }
  }
  None
}

/// Hover for variables
fn variable_hover(word: &str, analysis: Option<&ProjectAnalysis>) -> Option<Hover> {
  // Built-in variables
  let builtin_vars = [
    ("recipe", "Current recipe name"),
    ("args", "All arguments passed to the recipe"),
    ("arg0", "Alias for `recipe`"),
    ("justfile", "Path to the justfile"),
    ("justfile_directory", "Directory containing the justfile"),
    ("invocation_directory", "Directory where just was invoked"),
    (
      "invocation_directory_native",
      "Native invocation directory (platform-specific)",
    ),
  ];

  for (name, doc) in builtin_vars {
    if name == word {
      return Some(Hover {
        contents: HoverContents::Markup(MarkupContent {
          kind: MarkupKind::Markdown,
          value: format!("## Built-in Variable: `{}`\n\n{}", name, doc),
        }),
        range: None,
      });
    }
  }

  // User-defined variables from analysis
  if let Some(analysis) = analysis {
    if let Some(var_info) = analysis.variables.get(word) {
      return Some(Hover {
        contents: HoverContents::Markup(MarkupContent {
          kind: MarkupKind::Markdown,
          value: format!(
            "## Variable: `{}`\n\n**Value:** `{}`\n**Exported:** {}",
            word, var_info.value, var_info.exported
          ),
        }),
        range: None,
      });
    }
  }

  None
}

/// Hover for parameter kinds
fn parameter_kind_hover(kind: &str) -> Option<Hover> {
  let doc = match kind {
        "plus" => "Variadic parameter accepting **one or more** arguments. Expands to space-separated string.",
        "star" => "Variadic parameter accepting **zero or more** arguments. Expands to space-separated string (empty if none).",
        "singular" => "Single positional parameter.",
        _ => return None,
    };

  Some(Hover {
    contents: HoverContents::Markup(MarkupContent {
      kind: MarkupKind::Markdown,
      value: format!("## Parameter Kind: `{}`\n\n{}", kind, doc),
    }),
    range: None,
  })
}

/// Extract word at character position
fn extract_word_at(line: &str, char_idx: usize) -> Option<String> {
  if char_idx >= line.len() {
    return None;
  }

  let chars: Vec<char> = line.chars().collect();
  if char_idx >= chars.len() {
    return None;
  }

  let mut start = char_idx;
  let mut end = char_idx;

  while start > 0 && is_identifier_char(chars[start - 1]) {
    start -= 1;
  }

  while end < chars.len() && is_identifier_char(chars[end]) {
    end += 1;
  }

  if start < end {
    Some(chars[start..end].iter().collect())
  } else {
    None
  }
}

fn is_identifier_char(c: char) -> bool {
  c.is_alphanumeric() || c == '_' || c == '-' || c == ':' || c == '$' || c == '@'
}
