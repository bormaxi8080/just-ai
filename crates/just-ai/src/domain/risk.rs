use {
  serde::{Deserialize, Serialize},
  std::fmt::{self, Display, Formatter},
};

#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum RiskLevel {
  Low,
  Medium,
  High,
  Blocked,
}

impl RiskLevel {
  #[must_use]
  pub fn highest(findings: &[RiskFinding]) -> Self {
    findings
      .iter()
      .map(|finding| finding.level)
      .max()
      .unwrap_or(Self::Low)
  }
}

impl Display for RiskLevel {
  fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
    write!(
      f,
      "{}",
      match self {
        Self::Low => "low",
        Self::Medium => "medium",
        Self::High => "high",
        Self::Blocked => "blocked",
      }
    )
  }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct RiskFinding {
  pub level: RiskLevel,
  pub line: String,
  pub reason: String,
}

impl RiskFinding {
  #[must_use]
  pub fn scan_lines(lines: &[String]) -> Vec<Self> {
    lines
      .iter()
      .flat_map(|line| Self::scan_line(line))
      .collect()
  }

  #[must_use]
  pub fn scan_line(line: &str) -> Vec<Self> {
    let normalized = normalize_command(line);
    let mut findings = Vec::new();

    for (needle, reason) in [
      ("rm -rf /", "recursively removes from the filesystem root"),
      ("rm -fr /", "recursively removes from the filesystem root"),
    ] {
      if normalized.contains(needle) {
        findings.push(Self::new(RiskLevel::Blocked, line, reason));
      }
    }

    if downloads_to_shell(&normalized) {
      findings.push(Self::new(
        RiskLevel::Blocked,
        line,
        "pipes downloaded content to a shell",
      ));
    }

    for (needle, reason) in [
      ("rm -rf", "recursively removes files"),
      ("rm -fr", "recursively removes files"),
      ("sudo ", "requires elevated privileges"),
      ("docker system prune", "removes Docker data"),
      ("git clean -fd", "removes untracked files"),
      ("chmod -r", "recursively changes permissions"),
      ("chown -r", "recursively changes ownership"),
      ("mkfs", "formats a filesystem"),
      ("dd if=", "performs raw disk or byte copying"),
      ("> /dev/", "writes to a device file"),
    ] {
      if normalized.contains(needle) {
        findings.push(Self::new(RiskLevel::High, line, reason));
      }
    }

    for (needle, reason) in [
      ("cargo install", "installs executable dependencies"),
      ("npm install", "installs package dependencies"),
      ("pnpm install", "installs package dependencies"),
      ("yarn add", "installs package dependencies"),
      ("pip install", "installs package dependencies"),
      ("brew install", "installs system packages"),
      ("curl ", "downloads content from the network"),
      ("wget ", "downloads content from the network"),
      ("git push", "changes remote git state"),
      ("git pull", "changes local git state from a remote"),
      ("docker build", "builds container images"),
      ("docker compose", "runs Docker Compose"),
    ] {
      if normalized.contains(needle) {
        findings.push(Self::new(RiskLevel::Medium, line, reason));
      }
    }

    findings
  }

  fn new(level: RiskLevel, line: &str, reason: &str) -> Self {
    Self {
      level,
      line: line.to_owned(),
      reason: reason.to_owned(),
    }
  }
}

fn normalize_command(line: &str) -> String {
  line
    .to_ascii_lowercase()
    .split_whitespace()
    .collect::<Vec<_>>()
    .join(" ")
}

fn downloads_to_shell(command: &str) -> bool {
  (command.contains("curl ") || command.contains("wget "))
    && (command.contains("| sh") || command.contains("| bash"))
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn downloaded_shell_is_blocked() {
    let findings = RiskFinding::scan_line("curl https://example.test/install | sh");
    assert_eq!(RiskLevel::highest(&findings), RiskLevel::Blocked);
  }

  #[test]
  fn benign_command_is_low() {
    assert_eq!(
      RiskLevel::highest(&RiskFinding::scan_line("cargo test")),
      RiskLevel::Low
    );
  }
}
