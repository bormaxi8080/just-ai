use {
  serde::{Deserialize, Serialize},
  std::{fs, path::Path},
};

const DEFAULT_TOTAL_BUDGET: usize = 64 * 1024;
const DEFAULT_FILE_BUDGET: usize = 16 * 1024;
const ALLOWLIST: &[&str] = &[
  "Cargo.toml",
  "pyproject.toml",
  "package.json",
  "compose.yaml",
  "compose.yml",
  "docker-compose.yaml",
  "docker-compose.yml",
  "Makefile",
  "README.md",
];

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ScannedFile {
  pub path: String,
  pub content: String,
  pub truncated: bool,
  pub redactions: usize,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct ProjectFacts {
  pub files: Vec<ScannedFile>,
  pub omitted_by_budget: Vec<String>,
}

#[derive(Clone, Debug)]
pub struct ProjectScanner {
  file_budget: usize,
  total_budget: usize,
}

impl Default for ProjectScanner {
  fn default() -> Self {
    Self {
      file_budget: DEFAULT_FILE_BUDGET,
      total_budget: DEFAULT_TOTAL_BUDGET,
    }
  }
}

impl ProjectScanner {
  #[must_use]
  pub fn with_budgets(file_budget: usize, total_budget: usize) -> Self {
    Self {
      file_budget,
      total_budget,
    }
  }

  #[must_use]
  pub fn scan(&self, root: &Path) -> ProjectFacts {
    let mut facts = ProjectFacts::default();
    let mut remaining = self.total_budget;
    for relative in ALLOWLIST {
      let path = root.join(relative);
      if !path.is_file() {
        continue;
      }
      if remaining == 0 {
        facts.omitted_by_budget.push((*relative).into());
        continue;
      }
      let Ok(bytes) = fs::read(path) else {
        continue;
      };
      let limit = bytes.len().min(self.file_budget).min(remaining);
      let truncated = limit < bytes.len();
      let (mut content, redactions) = redact_text(&String::from_utf8_lossy(&bytes[..limit]));
      truncate_utf8(&mut content, remaining.min(self.file_budget));
      remaining = remaining.saturating_sub(content.len());
      facts.files.push(ScannedFile {
        path: (*relative).into(),
        content,
        truncated,
        redactions,
      });
    }
    facts
  }
}

fn truncate_utf8(content: &mut String, limit: usize) {
  if content.len() <= limit {
    return;
  }
  let mut boundary = limit;
  while !content.is_char_boundary(boundary) {
    boundary -= 1;
  }
  content.truncate(boundary);
}

#[must_use]
pub fn redact_text(content: &str) -> (String, usize) {
  let mut count = 0;
  let rendered = content
    .lines()
    .map(|line| {
      let normalized = line.to_ascii_lowercase();
      if looks_secret(&normalized) {
        count += 1;
        let prefix = line
          .split_once(['=', ':'])
          .map_or("secret", |(prefix, _)| prefix.trim());
        format!("{prefix} = <redacted>")
      } else {
        line.to_owned()
      }
    })
    .collect::<Vec<_>>()
    .join("\n");
  (rendered, count)
}

fn looks_secret(line: &str) -> bool {
  const NAMES: &[&str] = &[
    "api_key",
    "apikey",
    "access_token",
    "auth_token",
    "client_secret",
    "password",
    "private_key",
  ];
  NAMES.iter().any(|name| line.contains(name)) && (line.contains('=') || line.contains(':'))
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn reads_only_allowlisted_files_and_redacts_secrets() {
    let directory = tempfile::tempdir().unwrap();
    fs::write(
      directory.path().join("README.md"),
      "hello\nAPI_KEY=very-secret\n",
    )
    .unwrap();
    fs::write(directory.path().join(".env"), "TOKEN=must-not-leak\n").unwrap();
    let facts = ProjectScanner::default().scan(directory.path());
    assert_eq!(facts.files.len(), 1);
    assert_eq!(facts.files[0].redactions, 1);
    assert!(facts.files[0].content.contains("<redacted>"));
    assert!(!facts.files[0].content.contains("very-secret"));
    assert!(!facts.files[0].content.contains("must-not-leak"));
  }

  #[test]
  fn enforces_file_budget() {
    let directory = tempfile::tempdir().unwrap();
    fs::write(directory.path().join("README.md"), "abcdefghij").unwrap();
    let facts = ProjectScanner::with_budgets(4, 4).scan(directory.path());
    assert!(facts.files[0].truncated);
    assert_eq!(facts.files[0].content, "abcd");
  }

  #[test]
  fn redaction_cannot_expand_past_budget() {
    let directory = tempfile::tempdir().unwrap();
    fs::write(directory.path().join("README.md"), "API_KEY=x").unwrap();
    let facts = ProjectScanner::with_budgets(12, 12).scan(directory.path());
    assert!(facts.files[0].content.len() <= 12);
    assert!(!facts.files[0].content.contains('x'));
  }
}
