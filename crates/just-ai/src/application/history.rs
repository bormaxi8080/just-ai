use {
  super::{
    execution::{CompletedRun, RunRequest},
    project_context::redact_text,
  },
  serde::{Deserialize, Serialize},
  std::{
    fs,
    hash::{DefaultHasher, Hash, Hasher},
    io::{self, BufRead, BufReader, Write},
    path::{Path, PathBuf},
  },
  tempfile::NamedTempFile,
};

const OUTPUT_TAIL_BYTES: usize = 16 * 1024;

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
      stdout_tail: output_tail(&completed.stdout),
      stderr_tail: output_tail(&completed.stderr),
    }
  }
}

pub trait RunHistory {
  fn append(&self, record: &RunRecord) -> io::Result<()>;
  fn recent(&self, limit: usize) -> io::Result<Vec<RunRecord>>;
}

#[derive(Clone, Debug)]
pub struct JsonLineHistory {
  path: PathBuf,
  retained_records: usize,
}

#[must_use]
pub fn project_history_path(root: &Path) -> PathBuf {
  let base = std::env::var_os("JUST_AI_DATA_DIR")
    .map(PathBuf::from)
    .or_else(dirs::data_local_dir)
    .unwrap_or_else(std::env::temp_dir)
    .join("just-ai");
  let mut hasher = DefaultHasher::new();
  fs::canonicalize(root)
    .unwrap_or_else(|_| root.to_path_buf())
    .hash(&mut hasher);
  base.join(format!("project-{:016x}.jsonl", hasher.finish()))
}

#[must_use]
pub fn output_tail(bytes: &[u8]) -> String {
  let from = bytes.len().saturating_sub(OUTPUT_TAIL_BYTES);
  redact_text(&String::from_utf8_lossy(&bytes[from..])).0
}

impl JsonLineHistory {
  #[must_use]
  pub fn new(path: impl Into<PathBuf>, retained_records: usize) -> Self {
    Self {
      path: path.into(),
      retained_records,
    }
  }

  fn read_all(&self) -> io::Result<Vec<RunRecord>> {
    let file = match fs::File::open(&self.path) {
      Ok(file) => file,
      Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
      Err(error) => return Err(error),
    };
    BufReader::new(file)
      .lines()
      .filter(|line| line.as_ref().map_or(true, |line| !line.trim().is_empty()))
      .map(|line| {
        let line = line?;
        serde_json::from_str(&line)
          .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))
      })
      .collect()
  }
}

impl RunHistory for JsonLineHistory {
  fn append(&self, record: &RunRecord) -> io::Result<()> {
    let mut records = self.read_all()?;
    records.push(record.clone());
    let keep_from = records.len().saturating_sub(self.retained_records);
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
}

#[cfg(test)]
mod tests {
  use super::*;

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
    let history = JsonLineHistory::new(directory.path().join("history.jsonl"), 2);
    history.append(&record("one")).unwrap();
    history.append(&record("two")).unwrap();
    history.append(&record("three")).unwrap();
    assert_eq!(
      history.recent(10).unwrap(),
      [record("three"), record("two")]
    );
  }

  #[test]
  fn missing_history_is_empty() {
    let directory = tempfile::tempdir().unwrap();
    let history = JsonLineHistory::new(directory.path().join("missing.jsonl"), 5);
    assert!(history.recent(5).unwrap().is_empty());
  }

  #[test]
  fn equivalent_project_paths_share_history() {
    let directory = tempfile::tempdir().unwrap();
    assert_eq!(
      project_history_path(directory.path()),
      project_history_path(&directory.path().join("."))
    );
  }

  #[test]
  fn output_is_bounded_to_tail() {
    let bytes = vec![b'x'; OUTPUT_TAIL_BYTES + 10];
    assert_eq!(output_tail(&bytes).len(), OUTPUT_TAIL_BYTES);
  }

  #[test]
  fn output_tail_redacts_likely_secrets() {
    assert_eq!(output_tail(b"API_KEY=secret\n"), "API_KEY = <redacted>");
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
    let record = RunRecord::completed(&request, 10, 25, &completed);
    assert_eq!(record.id, "10-deploy");
    assert_eq!(record.arguments, ["production west"]);
    assert_eq!(record.stdout_tail, "deployed");
    assert_eq!(record.stderr_tail, "API_KEY = <redacted>");
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

  #[test]
  fn legacy_records_default_new_observability_fields() {
    let record: RunRecord = serde_json::from_str(
      r#"{"id":"1-test","recipe":"test","started_at_ms":1,"duration_ms":2,"exit_code":0,"success":true,"stdout_tail":"","stderr_tail":""}"#,
    )
    .unwrap();
    assert!(record.arguments.is_empty());
    assert!(!record.cancelled);
  }
}
