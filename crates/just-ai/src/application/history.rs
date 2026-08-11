use {
  super::{
    execution::{CompletedRun, RunRequest},
    project_context::redact_text,
  },
  crate::bounded_file,
  crate::config::{HistoryBackend, HistoryConfig},
  chrono::{DateTime, Utc},
  serde::{Deserialize, Serialize},
  sqlx::{Pool, Row, Sqlite, sqlite::SqlitePoolOptions},
  std::{
    fs,
    hash::{DefaultHasher, Hash, Hasher},
    io::{self, Write},
    path::{Path, PathBuf},
  },
  tempfile::NamedTempFile,
};

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct RunRecord {
  pub id: String,
  pub recipe: String,
  #[serde(default)]
  pub arguments: Vec<String>,
  pub started_at_ms: u128,
  pub duration_ms: u128,
  pub exit_code: Option<i32>,
  pub success: bool,
  #[serde(default)]
  pub cancelled: bool,
  pub stdout_tail: String,
  pub stderr_tail: String,
}

impl RunRecord {
  #[must_use]
  pub fn completed(
    request: &RunRequest,
    started_at_ms: u128,
    duration_ms: u128,
    completed: &CompletedRun,
    config: &HistoryConfig,
  ) -> Self {
    Self {
      id: format!("{started_at_ms}-{}", request.recipe),
      recipe: request.recipe.clone(),
      arguments: request.arguments.clone(),
      started_at_ms,
      duration_ms,
      exit_code: completed.status.code(),
      success: completed.status.success(),
      cancelled: completed.cancelled,
      stdout_tail: output_tail(&completed.stdout, config),
      stderr_tail: output_tail(&completed.stderr, config),
    }
  }
}

/// SQLite-based run record with additional metadata for queries
#[derive(Debug)]
struct SqliteRunRecord {
  id: String,
  recipe: String,
  arguments: String, // JSON serialized
  started_at_ms: i64,
  duration_ms: i64,
  exit_code: Option<i32>,
  success: bool,
  cancelled: bool,
  stdout_tail: String,
  stderr_tail: String,
  started_at: DateTime<Utc>,
}

impl From<RunRecord> for SqliteRunRecord {
  fn from(record: RunRecord) -> Self {
    let started_at =
      DateTime::from_timestamp_millis(i64::try_from(record.started_at_ms).unwrap_or(0))
        .unwrap_or_else(Utc::now);
    Self {
      id: record.id,
      recipe: record.recipe,
      arguments: serde_json::to_string(&record.arguments).unwrap_or_default(),
      started_at_ms: i64::try_from(record.started_at_ms).unwrap_or(0),
      duration_ms: i64::try_from(record.duration_ms).unwrap_or(0),
      exit_code: record.exit_code,
      success: record.success,
      cancelled: record.cancelled,
      stdout_tail: record.stdout_tail,
      stderr_tail: record.stderr_tail,
      started_at,
    }
  }
}

impl From<SqliteRunRecord> for RunRecord {
  fn from(record: SqliteRunRecord) -> Self {
    let arguments: Vec<String> = serde_json::from_str(&record.arguments).unwrap_or_default();
    Self {
      id: record.id,
      recipe: record.recipe,
      arguments,
      started_at_ms: u128::try_from(record.started_at_ms).unwrap_or(0),
      duration_ms: u128::try_from(record.duration_ms).unwrap_or(0),
      exit_code: record.exit_code,
      success: record.success,
      cancelled: record.cancelled,
      stdout_tail: record.stdout_tail,
      stderr_tail: record.stderr_tail,
    }
  }
}

pub trait RunHistory {
  fn append(&self, record: &RunRecord) -> io::Result<()>;
  fn recent(&self, limit: usize) -> io::Result<Vec<RunRecord>>;
  fn query(
    &self,
    recipe: Option<&str>,
    success: Option<bool>,
    limit: usize,
  ) -> io::Result<Vec<RunRecord>>;
}

#[derive(Clone, Debug)]
pub struct JsonLineHistory {
  path: PathBuf,
  config: HistoryConfig,
}

#[derive(Clone)]
pub struct SqliteHistory {
  pool: Pool<Sqlite>,
  config: HistoryConfig,
}

#[must_use]
pub fn project_history_path(root: &Path, file_name: &str) -> PathBuf {
  let base = std::env::var_os("JUST_AI_DATA_DIR")
    .map(PathBuf::from)
    .or_else(dirs::data_local_dir)
    .unwrap_or_else(std::env::temp_dir)
    .join("just-ai");
  let mut hasher = DefaultHasher::new();
  fs::canonicalize(root)
    .unwrap_or_else(|_| root.to_path_buf())
    .hash(&mut hasher);
  base.join(format!("project-{:016x}.{}", hasher.finish(), file_name))
}

#[must_use]
pub fn output_tail(bytes: &[u8], config: &HistoryConfig) -> String {
  let from = bytes.len().saturating_sub(config.output_tail_bytes);
  redact_text(&String::from_utf8_lossy(&bytes[from..])).0
}

impl JsonLineHistory {
  #[must_use]
  pub fn new(path: impl Into<PathBuf>, config: HistoryConfig) -> Self {
    let mut config = config;
    config.max_records = config.max_records.min(10000); // hard upper bound
    Self {
      path: path.into(),
      config,
    }
  }

  fn read_all(&self) -> io::Result<Vec<RunRecord>> {
    let content = match bounded_file::read_utf8(&self.path, self.config.effective_max_file_bytes())
    {
      Ok(content) => content,
      Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
      Err(error) => return Err(error),
    };
    let mut records = content
      .lines()
      .filter(|line| !line.trim().is_empty())
      .rev()
      .take(self.config.max_records)
      .map(|line| {
        if line.len() > self.config.max_record_bytes {
          return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!(
              "history record exceeds {} byte limit",
              self.config.max_record_bytes
            ),
          ));
        }
        serde_json::from_str(line)
          .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))
      })
      .collect::<io::Result<Vec<_>>>()?;
    records.reverse();
    Ok(records)
  }
}

impl RunHistory for JsonLineHistory {
  fn append(&self, record: &RunRecord) -> io::Result<()> {
    let encoded = serde_json::to_vec(record).map_err(io::Error::other)?;
    if encoded.len() > self.config.max_record_bytes {
      return Err(io::Error::new(
        io::ErrorKind::InvalidInput,
        format!(
          "history record exceeds {} byte limit",
          self.config.max_record_bytes
        ),
      ));
    }
    let mut records = self.read_all()?;
    records.push(record.clone());
    let keep_from = records.len().saturating_sub(self.config.max_records);
    let parent = self.path.parent().unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent)?;
    let mut temporary = NamedTempFile::new_in(parent)?;
    for record in &records[keep_from..] {
      serde_json::to_writer(&mut temporary, record).map_err(io::Error::other)?;
      temporary.write_all(b"\n")?;
    }
    temporary.as_file().sync_all()?;
    temporary.persist(&self.path).map_err(|error| error.error)?;
    Ok(())
  }

  fn recent(&self, limit: usize) -> io::Result<Vec<RunRecord>> {
    let records = self.read_all()?;
    let from = records.len().saturating_sub(limit);
    Ok(records[from..].iter().rev().cloned().collect())
  }

  fn query(
    &self,
    recipe: Option<&str>,
    success: Option<bool>,
    limit: usize,
  ) -> io::Result<Vec<RunRecord>> {
    let records = self.read_all()?;
    let mut filtered: Vec<RunRecord> = records
      .into_iter()
      .filter(|r| recipe.is_none_or(|recipe_name| r.recipe == recipe_name))
      .filter(|r| success.is_none_or(|s| r.success == s))
      .rev()
      .take(limit)
      .collect();
    filtered.reverse();
    Ok(filtered)
  }
}

impl SqliteHistory {
  pub async fn new(config: HistoryConfig) -> io::Result<Self> {
    let base = std::env::var_os("JUST_AI_DATA_DIR")
      .map(PathBuf::from)
      .or_else(dirs::data_local_dir)
      .unwrap_or_else(std::env::temp_dir)
      .join("just-ai");
    fs::create_dir_all(&base)?;
    let db_path = base.join(&config.database_file);
    let db_url = format!("sqlite://{}?mode=rwc", db_path.display());

    let pool = SqlitePoolOptions::new()
      .max_connections(1)
      .connect(&db_url)
      .await
      .map_err(io::Error::other)?;

    sqlx::query(
      r#"
            CREATE TABLE IF NOT EXISTS runs (
                id TEXT PRIMARY KEY,
                recipe TEXT NOT NULL,
                arguments TEXT NOT NULL DEFAULT '[]',
                started_at_ms INTEGER NOT NULL,
                duration_ms INTEGER NOT NULL,
                exit_code INTEGER,
                success BOOLEAN NOT NULL,
                cancelled BOOLEAN NOT NULL DEFAULT FALSE,
                stdout_tail TEXT NOT NULL DEFAULT '',
                stderr_tail TEXT NOT NULL DEFAULT '',
                started_at DATETIME NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_runs_recipe ON runs(recipe);
            CREATE INDEX IF NOT EXISTS idx_runs_started_at ON runs(started_at);
            CREATE INDEX IF NOT EXISTS idx_runs_success ON runs(success);
            "#,
    )
    .execute(&pool)
    .await
    .map_err(io::Error::other)?;

    Ok(Self { pool, config })
  }

  async fn enforce_retention(&self) -> io::Result<()> {
    sqlx::query(
      r#"
            DELETE FROM runs
            WHERE id IN (
                SELECT id FROM runs
                ORDER BY started_at DESC
                LIMIT -1 OFFSET ?
            )
            "#,
    )
    .bind(i64::try_from(self.config.max_records).unwrap_or(500))
    .execute(&self.pool)
    .await
    .map_err(io::Error::other)?;
    Ok(())
  }
}

impl RunHistory for SqliteHistory {
  fn append(&self, record: &RunRecord) -> io::Result<()> {
    // For synchronous interface, we block on the async operation in a new runtime
    let rt = tokio::runtime::Runtime::new().map_err(io::Error::other)?;
    rt.block_on(self.append_async(record))
  }

  fn recent(&self, limit: usize) -> io::Result<Vec<RunRecord>> {
    let rt = tokio::runtime::Runtime::new().map_err(io::Error::other)?;
    rt.block_on(self.recent_async(limit))
  }

  fn query(
    &self,
    recipe: Option<&str>,
    success: Option<bool>,
    limit: usize,
  ) -> io::Result<Vec<RunRecord>> {
    let rt = tokio::runtime::Runtime::new().map_err(io::Error::other)?;
    rt.block_on(self.query_async(recipe, success, limit))
  }
}

impl SqliteHistory {
  async fn append_async(&self, record: &RunRecord) -> io::Result<()> {
    let sqlite_record: SqliteRunRecord = record.clone().into();
    sqlx::query(
            r#"
            INSERT OR REPLACE INTO runs (id, recipe, arguments, started_at_ms, duration_ms, exit_code, success, cancelled, stdout_tail, stderr_tail, started_at)
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            "#
        )
        .bind(&sqlite_record.id)
        .bind(&sqlite_record.recipe)
        .bind(&sqlite_record.arguments)
        .bind(sqlite_record.started_at_ms)
        .bind(sqlite_record.duration_ms)
        .bind(sqlite_record.exit_code)
        .bind(sqlite_record.success)
        .bind(sqlite_record.cancelled)
        .bind(&sqlite_record.stdout_tail)
        .bind(&sqlite_record.stderr_tail)
        .bind(sqlite_record.started_at)
        .execute(&self.pool)
        .await
        .map_err(io::Error::other)?;

    self.enforce_retention().await
  }

  async fn recent_async(&self, limit: usize) -> io::Result<Vec<RunRecord>> {
    let rows = sqlx::query(
            r#"
            SELECT id, recipe, arguments, started_at_ms, duration_ms, exit_code, success, cancelled, stdout_tail, stderr_tail, started_at
            FROM runs
            ORDER BY started_at DESC
            LIMIT ?
            "#
        )
        .bind(i64::try_from(limit).unwrap_or(50))
        .fetch_all(&self.pool)
        .await
        .map_err(io::Error::other)?;

    let mut records: Vec<RunRecord> = rows
      .into_iter()
      .map(|row| {
        SqliteRunRecord {
          id: row.get("id"),
          recipe: row.get("recipe"),
          arguments: row.get("arguments"),
          started_at_ms: row.get("started_at_ms"),
          duration_ms: row.get("duration_ms"),
          exit_code: row.get("exit_code"),
          success: row.get("success"),
          cancelled: row.get("cancelled"),
          stdout_tail: row.get("stdout_tail"),
          stderr_tail: row.get("stderr_tail"),
          started_at: row.get("started_at"),
        }
        .into()
      })
      .collect();
    records.reverse();
    Ok(records)
  }

  async fn query_async(
    &self,
    recipe: Option<&str>,
    success: Option<bool>,
    limit: usize,
  ) -> io::Result<Vec<RunRecord>> {
    let mut query = String::from(
      r#"
            SELECT id, recipe, arguments, started_at_ms, duration_ms, exit_code, success, cancelled, stdout_tail, stderr_tail, started_at
            FROM runs
            WHERE 1=1
            "#,
    );
    if recipe.is_some() {
      query.push_str(" AND recipe = ?");
    }
    if success.is_some() {
      query.push_str(" AND success = ?");
    }
    query.push_str(" ORDER BY started_at DESC LIMIT ?");

    let mut q = sqlx::query(&query);
    if let Some(recipe_name) = recipe {
      q = q.bind(recipe_name);
    }
    if let Some(s) = success {
      q = q.bind(s);
    }
    q = q.bind(i64::try_from(limit).unwrap_or(50));

    let rows = q.fetch_all(&self.pool).await.map_err(io::Error::other)?;

    let mut records: Vec<RunRecord> = rows
      .into_iter()
      .map(|row| {
        SqliteRunRecord {
          id: row.get("id"),
          recipe: row.get("recipe"),
          arguments: row.get("arguments"),
          started_at_ms: row.get("started_at_ms"),
          duration_ms: row.get("duration_ms"),
          exit_code: row.get("exit_code"),
          success: row.get("success"),
          cancelled: row.get("cancelled"),
          stdout_tail: row.get("stdout_tail"),
          stderr_tail: row.get("stderr_tail"),
          started_at: row.get("started_at"),
        }
        .into()
      })
      .collect();
    records.reverse();
    Ok(records)
  }
}

pub fn create_history(config: HistoryConfig) -> io::Result<Box<dyn RunHistory>> {
  match config.backend {
    HistoryBackend::Jsonl => {
      let path = project_history_path(Path::new("."), &config.file_name);
      Ok(Box::new(JsonLineHistory::new(path, config)))
    }
    HistoryBackend::Sqlite => {
      // This creates the history in a blocking manner - caller should use async version
      // For backward compatibility, we provide a synchronous factory that works with existing code
      let rt = tokio::runtime::Runtime::new().map_err(io::Error::other)?;
      let history = rt.block_on(SqliteHistory::new(config))?;
      Ok(Box::new(history))
    }
  }
}

/// Migrate history from JSONL to SQLite.
/// Reads all records from the JSONL file and writes them to SQLite.
pub fn migrate_jsonl_to_sqlite(
  jsonl_config: HistoryConfig,
  sqlite_config: HistoryConfig,
) -> io::Result<()> {
  let jsonl_path = project_history_path(Path::new("."), &jsonl_config.file_name);
  let jsonl_history = JsonLineHistory::new(jsonl_path, jsonl_config.clone());
  let records = jsonl_history.read_all()?;
  let count = records.len();

  let rt = tokio::runtime::Runtime::new().map_err(io::Error::other)?;
  let sqlite_history = rt.block_on(SqliteHistory::new(sqlite_config))?;

  for record in records {
    sqlite_history.append(&record)?;
  }

  println!("Migrated {count} records from JSONL to SQLite");
  Ok(())
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::config::{HistoryBackend, HistoryConfig};

  fn config() -> HistoryConfig {
    HistoryConfig {
      max_records: 2,
      backend: HistoryBackend::Jsonl,
      ..Default::default()
    }
  }

  fn record(id: &str) -> RunRecord {
    RunRecord {
      id: id.into(),
      recipe: "test".into(),
      arguments: Vec::new(),
      started_at_ms: 1,
      duration_ms: 2,
      exit_code: Some(0),
      success: true,
      cancelled: false,
      stdout_tail: String::new(),
      stderr_tail: String::new(),
    }
  }

  #[test]
  fn retains_only_configured_number_of_records() {
    let directory = tempfile::tempdir().unwrap();
    let history = JsonLineHistory::new(directory.path().join("history.jsonl"), config());
    history.append(&record("one")).unwrap();
    history.append(&record("two")).unwrap();
    history.append(&record("three")).unwrap();
    assert_eq!(
      history.recent(10).unwrap(),
      [record("three"), record("two")]
    );
  }

  #[test]
  fn constructor_caps_retention() {
    let config = HistoryConfig {
      max_records: 100,
      ..Default::default()
    };
    let history = JsonLineHistory::new("history.jsonl", config);
    assert_eq!(history.config.max_records, 100);
  }

  #[test]
  fn read_keeps_only_newest_configured_records() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("history.jsonl");
    let content = [record("one"), record("two"), record("three")]
      .into_iter()
      .map(|record| serde_json::to_string(&record).unwrap())
      .collect::<Vec<_>>()
      .join("\n");
    fs::write(&path, format!("{content}\n")).unwrap();

    assert_eq!(
      JsonLineHistory::new(path, config()).recent(10).unwrap(),
      [record("three"), record("two")]
    );
  }

  #[test]
  fn rejects_oversized_stored_record() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("history.jsonl");
    fs::write(&path, "x".repeat(config().max_record_bytes + 1)).unwrap();

    let error = JsonLineHistory::new(path, config()).recent(5).unwrap_err();
    assert_eq!(error.kind(), io::ErrorKind::InvalidData);
  }

  #[test]
  fn rejects_oversized_new_record_before_creating_file() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("history.jsonl");
    let mut oversized = record("oversized");
    oversized.arguments = vec!["x".repeat(config().max_record_bytes)];
    let history = JsonLineHistory::new(&path, config());

    let error = history.append(&oversized).unwrap_err();
    assert_eq!(error.kind(), io::ErrorKind::InvalidInput);
    assert!(!path.exists());
  }

  #[test]
  fn missing_history_is_empty() {
    let directory = tempfile::tempdir().unwrap();
    let history = JsonLineHistory::new(directory.path().join("missing.jsonl"), config());
    assert!(history.recent(5).unwrap().is_empty());
  }

  #[test]
  fn equivalent_project_paths_share_history() {
    let directory = tempfile::tempdir().unwrap();
    assert_eq!(
      project_history_path(directory.path(), &config().file_name),
      project_history_path(&directory.path().join("."), &config().file_name)
    );
  }

  #[test]
  fn output_is_bounded_to_tail() {
    let bytes = vec![b'x'; config().output_tail_bytes + 10];
    assert_eq!(
      output_tail(&bytes, &config()).len(),
      config().output_tail_bytes
    );
  }

  #[test]
  fn output_tail_redacts_likely_secrets() {
    assert_eq!(
      output_tail(b"API_KEY=secret\n", &config()),
      "API_KEY = <redacted>"
    );
  }

  #[test]
  fn completed_record_bounds_and_redacts_output() {
    let request = RunRequest {
      project_root: PathBuf::from("."),
      recipe: "deploy".into(),
      arguments: vec!["production west".into()],
    };
    let completed = CompletedRun {
      status: successful_status(),
      stdout: b"deployed\n".to_vec(),
      stderr: b"API_KEY=secret\n".to_vec(),
      cancelled: false,
    };
    let cfg = config();
    let record = RunRecord::completed(&request, 10, 25, &completed, &cfg);
    assert_eq!(record.id, "10-deploy");
    assert_eq!(record.arguments, ["production west"]);
    assert_eq!(record.stdout_tail, "deployed");
    assert_eq!(record.stderr_tail, "API_KEY = <redacted>");
  }

  #[test]
  fn legacy_records_default_new_observability_fields() {
    let record: RunRecord = serde_json::from_str(
            r#"{"id":"1-test","recipe":"test","started_at_ms":1,"duration_ms":2,"exit_code":0,"success":true,"stdout_tail":"","stderr_tail":""}"#,
        )
        .unwrap();
    assert!(record.arguments.is_empty());
    assert!(!record.cancelled);
  }

  #[test]
  fn custom_config_overrides_defaults() {
    let config = HistoryConfig {
      max_records: 10,
      output_tail_bytes: 4096,
      ..Default::default()
    };
    let directory = tempfile::tempdir().unwrap();
    let history = JsonLineHistory::new(directory.path().join("history.jsonl"), config);
    for i in 0..15 {
      history.append(&record(&format!("record-{i}"))).unwrap();
    }
    let recent = history.recent(20).unwrap();
    assert_eq!(recent.len(), 10);
    assert_eq!(recent[0].id, "record-14");
  }

  #[test]
  fn jsonl_query_filters_by_recipe() {
    let directory = tempfile::tempdir().unwrap();
    let history = JsonLineHistory::new(directory.path().join("history.jsonl"), config());
    history.append(&record("1")).unwrap();
    let mut r2 = record("2");
    r2.recipe = "build".into();
    history.append(&r2).unwrap();
    let mut r3 = record("3");
    r3.recipe = "test".into();
    history.append(&r3).unwrap();

    let results = history.query(Some("test"), None, 10).unwrap();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].recipe, "test");
  }

  #[test]
  fn jsonl_query_filters_by_success() {
    let directory = tempfile::tempdir().unwrap();
    let history = JsonLineHistory::new(directory.path().join("history.jsonl"), config());
    let mut r1 = record("1");
    r1.success = true;
    history.append(&r1).unwrap();
    let mut r2 = record("2");
    r2.success = false;
    history.append(&r2).unwrap();

    let results = history.query(None, Some(false), 10).unwrap();
    assert_eq!(results.len(), 1);
    assert!(!results[0].success);
  }

  #[cfg(unix)]
  fn successful_status() -> std::process::ExitStatus {
    std::process::Command::new("true").status().unwrap()
  }

  #[cfg(windows)]
  fn successful_status() -> std::process::ExitStatus {
    std::process::Command::new("cmd")
      .args(["/C", "exit", "0"])
      .status()
      .unwrap()
  }
}
