use {
  crate::domain::risk::{RiskFinding, RiskLevel},
  schemars::JsonSchema,
  serde::{Deserialize, Serialize},
  std::{
    collections::HashMap,
    path::{Path, PathBuf},
    time::Duration,
  },
};

/// Top-level configuration for just-ai.
#[derive(Clone, Debug, Default, Deserialize, Serialize, JsonSchema)]
pub struct Config {
  #[serde(default)]
  pub history: HistoryConfig,
  #[serde(default)]
  pub execution: ExecutionConfig,
  #[serde(default)]
  pub risk: RiskConfig,
  #[serde(default)]
  pub policy: PolicyConfig,
  #[serde(default)]
  pub ai: AiConfig,
  #[serde(default)]
  pub scanner: ScannerConfig,
  #[serde(default)]
  pub bounded_file: BoundedFileConfig,
  #[serde(default)]
  pub mcp: McpConfig,
}

impl Config {
  /// Load configuration from just-ai.toml in the given directory or its parents.
  pub fn load(project_root: &Path) -> Result<Self, ConfigError> {
    let config_path = find_config(project_root);
    if let Some(path) = config_path {
      let content = std::fs::read_to_string(&path).map_err(|e| ConfigError {
        kind: ConfigErrorKind::IoError,
        path: path.clone(),
        source: e.to_string(),
      })?;
      toml::from_str(&content).map_err(|e| ConfigError {
        kind: ConfigErrorKind::ParseError,
        path,
        source: e.to_string(),
      })
    } else {
      Ok(Self::default())
    }
  }

  /// Load configuration from a specific file path.
  pub fn load_from(path: &Path) -> Result<Self, ConfigError> {
    let content = std::fs::read_to_string(path).map_err(|e| ConfigError {
      kind: ConfigErrorKind::IoError,
      path: path.to_path_buf(),
      source: e.to_string(),
    })?;
    toml::from_str(&content).map_err(|e| ConfigError {
      kind: ConfigErrorKind::ParseError,
      path: path.to_path_buf(),
      source: e.to_string(),
    })
  }
}

pub fn find_config(start: &Path) -> Option<PathBuf> {
  let mut dir = start.to_path_buf();
  loop {
    let candidate = dir.join("just-ai.toml");
    if candidate.is_file() {
      return Some(candidate);
    }
    if !dir.pop() {
      return None;
    }
  }
}

#[derive(Clone, Debug, Deserialize, Serialize, JsonSchema)]
pub struct ConfigError {
  pub kind: ConfigErrorKind,
  pub path: PathBuf,
  pub source: String,
}

#[derive(Clone, Debug, Deserialize, Serialize, JsonSchema)]
pub enum ConfigErrorKind {
  IoError,
  ParseError,
}

impl std::fmt::Display for ConfigError {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    match self.kind {
      ConfigErrorKind::IoError => write!(
        f,
        "failed to read config file {}: {}",
        self.path.display(),
        self.source
      ),
      ConfigErrorKind::ParseError => write!(
        f,
        "failed to parse config file {}: {}",
        self.path.display(),
        self.source
      ),
    }
  }
}

impl std::error::Error for ConfigError {}

/// History configuration (JSONL or SQLite run history).
#[derive(Clone, Debug, Default, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum HistoryBackend {
  Jsonl,
  #[default]
  Sqlite,
}

/// History configuration (JSONL or SQLite run history).
#[derive(Clone, Debug, Deserialize, Serialize, JsonSchema)]
pub struct HistoryConfig {
  /// Storage backend: "jsonl" or "sqlite" (default: "sqlite")
  #[serde(default)]
  pub backend: HistoryBackend,
  /// Maximum number of history records to retain.
  #[serde(default = "default_max_records")]
  pub max_records: usize,
  /// Maximum size of a single history record in bytes.
  #[serde(default = "default_max_record_bytes")]
  pub max_record_bytes: usize,
  /// Maximum total history file size in bytes (JSONL only).
  #[serde(default = "default_max_file_bytes")]
  pub max_file_bytes: usize,
  /// Number of bytes to keep from stdout/stderr tails.
  #[serde(default = "default_output_tail_bytes")]
  pub output_tail_bytes: usize,
  /// Additional secret patterns to redact (regex).
  #[serde(default)]
  pub redact_patterns: Vec<String>,
  /// History file name (relative to project root) for JSONL backend.
  #[serde(default = "default_history_file")]
  pub file_name: String,
  /// Database file path for SQLite backend (relative to data dir).
  #[serde(default = "default_database_file")]
  pub database_file: String,
}

impl Default for HistoryConfig {
  fn default() -> Self {
    Self {
      backend: HistoryBackend::default(),
      max_records: default_max_records(),
      max_record_bytes: default_max_record_bytes(),
      max_file_bytes: default_max_file_bytes(),
      output_tail_bytes: default_output_tail_bytes(),
      redact_patterns: vec![],
      file_name: default_history_file(),
      database_file: default_database_file(),
    }
  }
}

fn default_max_records() -> usize {
  500
}
fn default_max_record_bytes() -> usize {
  64 * 1024
}
fn default_max_file_bytes() -> usize {
  500 * (64 * 1024 + 1)
}
fn default_output_tail_bytes() -> usize {
  16 * 1024
}
fn default_history_file() -> String {
  ".just-ai/history.jsonl".to_string()
}
fn default_database_file() -> String {
  "history.db".to_string()
}

impl HistoryConfig {
  /// Effective max file bytes (computed from records × record_bytes if not explicitly set).
  pub fn effective_max_file_bytes(&self) -> usize {
    self.max_file_bytes
  }
}

/// Execution configuration (recipe runtime).
#[derive(Clone, Debug, Deserialize, Serialize, JsonSchema)]
pub struct ExecutionConfig {
  /// Maximum captured output per stream (stdout/stderr) in bytes.
  #[serde(default = "default_max_capture_bytes")]
  pub max_capture_bytes: usize,
  /// Internal streaming queue capacity for run events.
  #[serde(default = "default_stream_queue_capacity")]
  pub stream_queue_capacity: usize,
  /// Cancellation poll interval in milliseconds.
  #[serde(default = "default_cancellation_poll_ms")]
  pub cancellation_poll_ms: u64,
  /// Default just binary name or path.
  #[serde(default = "default_just_binary")]
  pub just_binary: String,
  /// Read timeout for process output streams in seconds.
  #[serde(default = "default_read_timeout_secs")]
  pub read_timeout_secs: u64,
}

impl Default for ExecutionConfig {
  fn default() -> Self {
    Self {
      max_capture_bytes: default_max_capture_bytes(),
      stream_queue_capacity: default_stream_queue_capacity(),
      cancellation_poll_ms: default_cancellation_poll_ms(),
      just_binary: default_just_binary(),
      read_timeout_secs: default_read_timeout_secs(),
    }
  }
}

fn default_max_capture_bytes() -> usize {
  8 * 1024 * 1024
}
fn default_stream_queue_capacity() -> usize {
  32
}
fn default_cancellation_poll_ms() -> u64 {
  25
}
fn default_just_binary() -> String {
  "just".to_string()
}
fn default_read_timeout_secs() -> u64 {
  30
}

impl ExecutionConfig {
  pub fn cancellation_poll_interval(&self) -> Duration {
    Duration::from_millis(self.cancellation_poll_ms)
  }
  pub fn read_timeout(&self) -> Duration {
    Duration::from_secs(self.read_timeout_secs)
  }
}

/// Risk analysis configuration.
#[derive(Clone, Debug, Deserialize, Serialize, JsonSchema)]
pub struct RiskConfig {
  /// Custom blocked patterns: pattern -> reason
  #[serde(default)]
  pub blocked_patterns: HashMap<String, String>,
  /// Custom high-risk patterns: pattern -> reason
  #[serde(default)]
  pub high_patterns: HashMap<String, String>,
  /// Custom medium-risk patterns: pattern -> reason
  #[serde(default)]
  pub medium_patterns: HashMap<String, String>,
  /// Whether to enable built-in risk rules.
  #[serde(default = "default_true")]
  pub builtin_rules: bool,
}

impl Default for RiskConfig {
  fn default() -> Self {
    Self {
      blocked_patterns: HashMap::new(),
      high_patterns: HashMap::new(),
      medium_patterns: HashMap::new(),
      builtin_rules: true,
    }
  }
}

fn default_true() -> bool {
  true
}

impl RiskConfig {
  /// Scan a line for risk findings, combining built-in and custom rules.
  pub fn scan_line(&self, line: &str) -> Vec<RiskFinding> {
    let mut findings = Vec::new();
    if self.builtin_rules {
      findings.extend(RiskFinding::scan_line(line));
    }
    let normalized = normalize_command(line);
    for (pattern, reason) in &self.blocked_patterns {
      if normalized.contains(pattern) {
        findings.push(RiskFinding {
          level: RiskLevel::Blocked,
          line: line.to_owned(),
          reason: reason.clone(),
        });
      }
    }
    for (pattern, reason) in &self.high_patterns {
      if normalized.contains(pattern) {
        findings.push(RiskFinding {
          level: RiskLevel::High,
          line: line.to_owned(),
          reason: reason.clone(),
        });
      }
    }
    for (pattern, reason) in &self.medium_patterns {
      if normalized.contains(pattern) {
        findings.push(RiskFinding {
          level: RiskLevel::Medium,
          line: line.to_owned(),
          reason: reason.clone(),
        });
      }
    }
    findings
  }

  /// Scan multiple lines for risk findings.
  pub fn scan_lines(&self, lines: &[String]) -> Vec<RiskFinding> {
    lines.iter().flat_map(|line| self.scan_line(line)).collect()
  }
}

fn normalize_command(line: &str) -> String {
  line
    .to_ascii_lowercase()
    .split_whitespace()
    .collect::<Vec<_>>()
    .join(" ")
}

/// Policy configuration.
#[derive(Clone, Debug, Deserialize, Serialize, JsonSchema)]
pub struct PolicyConfig {
  /// Custom policy decisions per risk level.
  /// If not set, defaults apply: Low=Allow, Medium=Confirm, High=ConfirmTyped, Blocked=Deny.
  #[serde(default)]
  pub decisions: HashMap<String, PolicyDecisionConfig>,
  /// Default phrase template for ConfirmTyped (available vars: {recipe}).
  #[serde(default = "default_confirm_typed_phrase")]
  pub confirm_typed_phrase: String,
}

impl Default for PolicyConfig {
  fn default() -> Self {
    Self {
      decisions: HashMap::new(),
      confirm_typed_phrase: default_confirm_typed_phrase(),
    }
  }
}

fn default_confirm_typed_phrase() -> String {
  "run {recipe}".to_string()
}

#[derive(Clone, Debug, Deserialize, Serialize, JsonSchema)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum PolicyDecisionConfig {
  Allow,
  Confirm,
  ConfirmTyped { phrase: Option<String> },
  Deny { reason: String },
}

impl PolicyConfig {
  /// Evaluate a recipe against the policy, returning the decision.
  pub fn evaluate(&self, recipe: &str, risk: RiskLevel) -> crate::domain::policy::PolicyDecision {
    let risk_key = format!("{risk:?}").to_lowercase();
    if let Some(decision) = self.decisions.get(&risk_key) {
      return decision.to_policy_decision(recipe, &self.confirm_typed_phrase);
    }
    // Default policy
    match risk {
      RiskLevel::Low => crate::domain::policy::PolicyDecision::Allow,
      RiskLevel::Medium => crate::domain::policy::PolicyDecision::Confirm,
      RiskLevel::High => crate::domain::policy::PolicyDecision::ConfirmTyped {
        phrase: self.confirm_typed_phrase.replace("{recipe}", recipe),
      },
      RiskLevel::Blocked => crate::domain::policy::PolicyDecision::Deny {
        reason: "blocked by the default safety policy".into(),
      },
    }
  }
}

impl PolicyDecisionConfig {
  fn to_policy_decision(
    &self,
    recipe: &str,
    default_phrase: &str,
  ) -> crate::domain::policy::PolicyDecision {
    match self {
      PolicyDecisionConfig::Allow => crate::domain::policy::PolicyDecision::Allow,
      PolicyDecisionConfig::Confirm => crate::domain::policy::PolicyDecision::Confirm,
      PolicyDecisionConfig::ConfirmTyped { phrase } => {
        crate::domain::policy::PolicyDecision::ConfirmTyped {
          phrase: phrase
            .as_ref()
            .map(|p| p.replace("{recipe}", recipe))
            .unwrap_or_else(|| default_phrase.replace("{recipe}", recipe)),
        }
      }
      PolicyDecisionConfig::Deny { reason } => crate::domain::policy::PolicyDecision::Deny {
        reason: reason.clone(),
      },
    }
  }
}

/// AI Provider configuration.
#[derive(Clone, Debug, Deserialize, Serialize, JsonSchema)]
pub struct AiConfig {
  /// Provider name: openai, ollama, openai-compatible
  #[serde(default = "default_provider")]
  pub provider: String,
  /// Model name.
  #[serde(default)]
  pub model: Option<String>,
  /// Base URL for the API.
  #[serde(default)]
  pub base_url: Option<String>,
  /// API key (can also be set via JUST_AI_API_KEY env var).
  #[serde(default)]
  pub api_key: Option<String>,
  /// Request timeout in seconds.
  #[serde(default = "default_ai_timeout_secs")]
  pub timeout_secs: u64,
  /// Maximum tokens for completion (if supported).
  #[serde(default)]
  pub max_tokens: Option<u32>,
  /// Temperature for sampling (if supported).
  #[serde(default)]
  pub temperature: Option<f32>,
}

impl Default for AiConfig {
  fn default() -> Self {
    Self {
      provider: default_provider(),
      model: None,
      base_url: None,
      api_key: None,
      timeout_secs: default_ai_timeout_secs(),
      max_tokens: None,
      temperature: None,
    }
  }
}

fn default_provider() -> String {
  "openai".to_string()
}
fn default_ai_timeout_secs() -> u64 {
  60
}

impl AiConfig {
  /// Resolve the effective model name, falling back to provider defaults.
  pub fn effective_model(&self) -> String {
    self
      .model
      .clone()
      .unwrap_or_else(|| match self.provider.as_str() {
        "ollama" => "llama3.1".to_string(),
        "openai" => "gpt-5.6-terra".to_string(),
        "anthropic" => "claude-3-5-sonnet-20241022".to_string(),
        "azure" => "gpt-4o".to_string(),
        "gemini" => "gemini-1.5-pro".to_string(),
        _ => "gpt-5-mini".to_string(),
      })
  }

  /// Resolve the effective base URL.
  pub fn effective_base_url(&self) -> String {
    self
      .base_url
      .clone()
      .unwrap_or_else(|| match self.provider.as_str() {
        "ollama" => "http://localhost:11434".to_string(),
        "anthropic" => "https://api.anthropic.com".to_string(),
        "gemini" => "https://generativelanguage.googleapis.com".to_string(),
        _ => "https://api.openai.com/v1".to_string(),
      })
  }

  /// Resolve the API key from config or environment.
  pub fn effective_api_key(&self) -> Option<String> {
    self
      .api_key
      .clone()
      .or_else(|| std::env::var("JUST_AI_API_KEY").ok())
  }

  /// Build provider string for error messages.
  pub fn describe(&self) -> String {
    format!("{} ({})", self.provider, self.effective_model())
  }
}

/// Project scanner configuration.
#[derive(Clone, Debug, Deserialize, Serialize, JsonSchema)]
pub struct ScannerConfig {
  /// Per-file budget in bytes.
  #[serde(default = "default_file_budget")]
  pub file_budget: usize,
  /// Total budget in bytes.
  #[serde(default = "default_total_budget")]
  pub total_budget: usize,
  /// Allowlisted file names to scan.
  #[serde(default = "default_allowlist")]
  pub allowlist: Vec<String>,
}

impl Default for ScannerConfig {
  fn default() -> Self {
    Self {
      file_budget: default_file_budget(),
      total_budget: default_total_budget(),
      allowlist: default_allowlist(),
    }
  }
}

fn default_file_budget() -> usize {
  16 * 1024
}
fn default_total_budget() -> usize {
  64 * 1024
}
fn default_allowlist() -> Vec<String> {
  vec![
    "Cargo.toml".into(),
    "pyproject.toml".into(),
    "package.json".into(),
    "compose.yaml".into(),
    "compose.yml".into(),
    "docker-compose.yaml".into(),
    "docker-compose.yml".into(),
    "Makefile".into(),
    "README.md".into(),
  ]
}

/// Bounded file reading configuration.
#[derive(Clone, Debug, Deserialize, Serialize, JsonSchema)]
pub struct BoundedFileConfig {
  /// Maximum size of an editable file in bytes.
  #[serde(default = "default_max_editable_file_bytes")]
  pub max_editable_file_bytes: usize,
  /// Default prefix read limit in bytes.
  #[serde(default = "default_prefix_limit")]
  pub prefix_limit: usize,
}

impl Default for BoundedFileConfig {
  fn default() -> Self {
    Self {
      max_editable_file_bytes: default_max_editable_file_bytes(),
      prefix_limit: default_prefix_limit(),
    }
  }
}

fn default_max_editable_file_bytes() -> usize {
  1024 * 1024
}
fn default_prefix_limit() -> usize {
  16 * 1024
}

/// MCP server configuration.
#[derive(Clone, Debug, Deserialize, Serialize, JsonSchema)]
pub struct McpConfig {
  /// Maximum JSON-RPC frame size in bytes.
  #[serde(default = "default_max_message_bytes")]
  pub max_message_bytes: usize,
  /// Whether to enable stdio transport.
  #[serde(default = "default_true")]
  pub stdio_enabled: bool,
  /// Allowed tool names (empty = all built-in tools).
  #[serde(default)]
  pub allowed_tools: Vec<String>,
}

impl Default for McpConfig {
  fn default() -> Self {
    Self {
      max_message_bytes: default_max_message_bytes(),
      stdio_enabled: true,
      allowed_tools: vec![],
    }
  }
}

fn default_max_message_bytes() -> usize {
  1024 * 1024
}

#[cfg(test)]
mod tests {
  use super::*;
  use std::fs;

  #[test]
  fn default_config_is_valid() {
    let config = Config::default();
    assert_eq!(config.history.max_records, 500);
    assert_eq!(config.execution.max_capture_bytes, 8 * 1024 * 1024);
    assert_eq!(config.scanner.file_budget, 16 * 1024);
  }

  #[test]
  fn load_config_from_toml() {
    let toml = r#"
            [history]
            max_records = 1000
            output_tail_bytes = 32768

            [execution]
            max_capture_bytes = 16777216
            just_binary = "/usr/local/bin/just"

            [risk]
            builtin_rules = true
            blocked_patterns = { "rm -rf /data" = "protects data directory" }
            high_patterns = { "kubectl delete" = "kubernetes resource deletion" }

            [policy]
            decisions.high = { type = "confirm_typed", phrase = "execute {recipe}" }

            [ai]
            provider = "ollama"
            model = "mistral"
            base_url = "http://localhost:11434"
        "#;
    let config: Config = toml::from_str(toml).unwrap();
    assert_eq!(config.history.max_records, 1000);
    assert_eq!(config.history.output_tail_bytes, 32768);
    assert_eq!(config.execution.max_capture_bytes, 16 * 1024 * 1024);
    assert_eq!(config.execution.just_binary, "/usr/local/bin/just");
    assert!(config.risk.blocked_patterns.contains_key("rm -rf /data"));
    assert!(config.risk.high_patterns.contains_key("kubectl delete"));
    assert_eq!(config.ai.provider, "ollama");
    assert_eq!(config.ai.effective_model(), "mistral");
  }

  #[test]
  fn find_config_walks_parents() {
    let dir = tempfile::tempdir().unwrap();
    let subdir = dir.path().join("sub").join("dir");
    fs::create_dir_all(&subdir).unwrap();
    fs::write(
      dir.path().join("just-ai.toml"),
      "[history]\nmax_records = 42\n",
    )
    .unwrap();

    let found = find_config(&subdir).unwrap();
    assert_eq!(found, dir.path().join("just-ai.toml"));
  }
}
