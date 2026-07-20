use {
  crate::domain::{
    policy::{DefaultPolicy, PolicyDecision},
    risk::{RiskFinding, RiskLevel},
  },
  serde::{Deserialize, Serialize},
  std::{
    error::Error,
    fmt::{self, Display, Formatter},
    io::Read,
    path::PathBuf,
    process::{Command, ExitStatus, Stdio},
    sync::{
      Arc,
      atomic::{AtomicBool, Ordering},
      mpsc,
    },
    thread,
    time::{Duration, SystemTime, UNIX_EPOCH},
  },
};

mod process_tree;

use process_tree::ProcessTree;

const STREAM_QUEUE_CAPACITY: usize = 32;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct RunRequest {
  pub project_root: PathBuf,
  pub recipe: String,
  #[serde(default)]
  pub arguments: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct PreparedRun {
  pub request: RunRequest,
  pub preview: Vec<String>,
  pub risk: RiskLevel,
  pub findings: Vec<RiskFinding>,
  pub policy: PolicyDecision,
}

#[derive(Debug)]
pub struct CompletedRun {
  pub status: ExitStatus,
  pub stdout: Vec<u8>,
  pub stderr: Vec<u8>,
  pub cancelled: bool,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case", tag = "confirmation")]
pub enum RunConfirmation {
  None,
  Confirmed,
  Typed { phrase: String },
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case", tag = "event")]
pub enum RunEvent {
  Started { id: String },
  Stdout { text: String },
  Stderr { text: String },
  Exited { code: Option<i32>, cancelled: bool },
}

#[derive(Clone, Debug, Default)]
pub struct CancellationToken(Arc<AtomicBool>);

impl CancellationToken {
  pub fn cancel(&self) {
    self.0.store(true, Ordering::Release);
  }
  #[must_use]
  pub fn is_cancelled(&self) -> bool {
    self.0.load(Ordering::Acquire)
  }
}

#[derive(Debug)]
pub struct ExecutionError(String);

impl Display for ExecutionError {
  fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
    f.write_str(&self.0)
  }
}

impl Error for ExecutionError {}

#[derive(Clone, Debug)]
pub struct RecipeExecutor {
  just_binary: PathBuf,
}

impl RecipeExecutor {
  #[must_use]
  pub fn new(just_binary: impl Into<PathBuf>) -> Self {
    Self {
      just_binary: just_binary.into(),
    }
  }

  pub fn prepare(&self, request: RunRequest) -> Result<PreparedRun, ExecutionError> {
    validate_request(&request)?;
    let dump = crate::just_dump::load_at(&self.just_binary, Some(&request.project_root))
      .map_err(|error| ExecutionError(format!("just safety inspection failed: {error}")))?;
    if let Some(function) = crate::just_dump::first_function_call(&dump) {
      return Err(ExecutionError(format!(
        "safe preview unavailable: project contains function call `{function}()`"
      )));
    }
    if crate::just_dump::contains_non_empty_field(&dump, "dotenv_command") {
      return Err(ExecutionError(
        "safe preview unavailable: project configures dotenv-command".into(),
      ));
    }
    let output = crate::bounded_output::capture(&mut self.prepare_command(&request))
      .map_err(|error| ExecutionError(format!("just dry-run output capture failed: {error}")))?;
    if !output.status.success() {
      return Err(command_error("just dry-run", &output.stderr));
    }

    let preview_text = String::from_utf8_lossy(&output.stdout);
    let preview = preview_text.lines().map(str::to_owned).collect::<Vec<_>>();
    let findings = RiskFinding::scan_lines(&preview);
    let risk = RiskLevel::highest(&findings);
    let policy = DefaultPolicy.evaluate(&request.recipe, risk);

    Ok(PreparedRun {
      request,
      preview,
      risk,
      findings,
      policy,
    })
  }

  pub fn execute(
    &self,
    prepared: &PreparedRun,
    confirmation: &RunConfirmation,
  ) -> Result<CompletedRun, ExecutionError> {
    self.execute_streaming_with_limit(
      prepared,
      confirmation,
      &CancellationToken::default(),
      crate::bounded_output::MAX_CAPTURE_BYTES,
      |_| {},
    )
  }

  pub fn execute_streaming<F>(
    &self,
    prepared: &PreparedRun,
    confirmation: &RunConfirmation,
    cancellation: &CancellationToken,
    emit: F,
  ) -> Result<CompletedRun, ExecutionError>
  where
    F: FnMut(RunEvent),
  {
    self.execute_streaming_with_limit(
      prepared,
      confirmation,
      cancellation,
      crate::bounded_output::MAX_CAPTURE_BYTES,
      emit,
    )
  }

  fn execute_streaming_with_limit<F>(
    &self,
    prepared: &PreparedRun,
    confirmation: &RunConfirmation,
    cancellation: &CancellationToken,
    output_limit: usize,
    mut emit: F,
  ) -> Result<CompletedRun, ExecutionError>
  where
    F: FnMut(RunEvent),
  {
    let current = self.prepare(prepared.request.clone())?;
    if &current != prepared {
      return Err(ExecutionError(
        "recipe preview or policy changed after preparation".into(),
      ));
    }
    authorize(&current.policy, confirmation)?;

    let id = SystemTime::now()
      .duration_since(UNIX_EPOCH)
      .map_err(|error| ExecutionError(error.to_string()))?
      .as_nanos()
      .to_string();
    emit(RunEvent::Started { id });

    let mut command = self.command(&prepared.request);
    command.stdout(Stdio::piped()).stderr(Stdio::piped());
    let process_tree = ProcessTree::configure(&mut command)?;
    let mut child = command.spawn().map_err(io_error)?;
    process_tree.attach(&mut child)?;
    let Some(stdout) = child.stdout.take() else {
      process_tree.terminate(&mut child)?;
      return Err(ExecutionError("stdout pipe missing".into()));
    };
    let Some(stderr) = child.stderr.take() else {
      process_tree.terminate(&mut child)?;
      return Err(ExecutionError("stderr pipe missing".into()));
    };
    let (sender, receiver) = mpsc::sync_channel(STREAM_QUEUE_CAPACITY);
    stream_reader(stdout, StreamKind::Stdout, sender.clone());
    stream_reader(stderr, StreamKind::Stderr, sender);

    let mut stdout_bytes = Vec::new();
    let mut stderr_bytes = Vec::new();
    let mut closed = 0;
    let mut cancelled = false;
    let mut terminated = false;
    let mut output_error = None;
    while closed < 2 {
      if cancellation.is_cancelled() && !cancelled {
        if !terminated {
          process_tree.terminate(&mut child)?;
          terminated = true;
        }
        cancelled = true;
      }
      match receiver.recv_timeout(Duration::from_millis(25)) {
        Ok(StreamMessage::Data(StreamKind::Stdout, bytes)) => {
          if output_error.is_none() {
            match crate::bounded_output::extend_with_limit(
              &mut stdout_bytes,
              &bytes,
              "stdout",
              output_limit,
            ) {
              Ok(()) => emit(RunEvent::Stdout {
                text: String::from_utf8_lossy(&bytes).into_owned(),
              }),
              Err(error) => output_error = Some(ExecutionError(error.to_string())),
            }
          }
        }
        Ok(StreamMessage::Data(StreamKind::Stderr, bytes)) => {
          if output_error.is_none() {
            match crate::bounded_output::extend_with_limit(
              &mut stderr_bytes,
              &bytes,
              "stderr",
              output_limit,
            ) {
              Ok(()) => emit(RunEvent::Stderr {
                text: String::from_utf8_lossy(&bytes).into_owned(),
              }),
              Err(error) => output_error = Some(ExecutionError(error.to_string())),
            }
          }
        }
        Ok(StreamMessage::Closed) => closed += 1,
        Ok(StreamMessage::Failed(kind, error)) => {
          closed += 1;
          output_error.get_or_insert_with(|| {
            ExecutionError(format!("recipe {} read failed: {error}", kind.name()))
          });
        }
        Err(mpsc::RecvTimeoutError::Timeout) => {}
        Err(mpsc::RecvTimeoutError::Disconnected) => {
          output_error.get_or_insert_with(|| {
            ExecutionError("recipe output readers disconnected unexpectedly".into())
          });
          if !terminated {
            process_tree.terminate(&mut child)?;
          }
          break;
        }
      }
      if output_error.is_some() && !terminated {
        process_tree.terminate(&mut child)?;
        terminated = true;
      }
    }
    let status = child.wait().map_err(io_error)?;
    emit(RunEvent::Exited {
      code: status.code(),
      cancelled,
    });
    if let Some(error) = output_error {
      return Err(error);
    }
    Ok(CompletedRun {
      status,
      stdout: stdout_bytes,
      stderr: stderr_bytes,
      cancelled,
    })
  }

  fn command(&self, request: &RunRequest) -> Command {
    let mut command = Command::new(&self.just_binary);
    command
      .current_dir(&request.project_root)
      .arg("--")
      .arg(&request.recipe)
      .args(&request.arguments);
    command
  }

  fn prepare_command(&self, request: &RunRequest) -> Command {
    let mut command = Command::new(&self.just_binary);
    command
      .current_dir(&request.project_root)
      .arg("--dry-run")
      .arg("--")
      .arg(&request.recipe)
      .args(&request.arguments);
    command
  }
}

#[derive(Clone, Copy)]
enum StreamKind {
  Stdout,
  Stderr,
}

impl StreamKind {
  fn name(self) -> &'static str {
    match self {
      Self::Stdout => "stdout",
      Self::Stderr => "stderr",
    }
  }
}

enum StreamMessage {
  Data(StreamKind, Vec<u8>),
  Closed,
  Failed(StreamKind, std::io::Error),
}

fn stream_reader(
  mut reader: impl Read + Send + 'static,
  kind: StreamKind,
  sender: mpsc::SyncSender<StreamMessage>,
) {
  thread::spawn(move || {
    let mut buffer = [0_u8; 4096];
    loop {
      match reader.read(&mut buffer) {
        Ok(0) => break,
        Err(error) => {
          let _ = sender.send(StreamMessage::Failed(kind, error));
          return;
        }
        Ok(read) => {
          if sender
            .send(StreamMessage::Data(kind, buffer[..read].to_vec()))
            .is_err()
          {
            return;
          }
        }
      }
    }
    let _ = sender.send(StreamMessage::Closed);
  });
}

fn authorize(
  decision: &PolicyDecision,
  confirmation: &RunConfirmation,
) -> Result<(), ExecutionError> {
  let authorized = match (decision, confirmation) {
    (PolicyDecision::Allow, _) => true,
    (PolicyDecision::Confirm, RunConfirmation::Confirmed) => true,
    (
      PolicyDecision::ConfirmTyped { phrase: expected },
      RunConfirmation::Typed { phrase: actual },
    ) => expected == actual,
    (PolicyDecision::Deny { .. }, _) | (_, _) => false,
  };
  authorized
    .then_some(())
    .ok_or_else(|| ExecutionError("run does not have the required confirmation".into()))
}

fn validate_request(request: &RunRequest) -> Result<(), ExecutionError> {
  if request.recipe.is_empty() || request.recipe.starts_with('-') {
    return Err(ExecutionError(
      "recipe name must be non-empty and must not start with '-'".into(),
    ));
  }
  if !request.project_root.is_dir() {
    return Err(ExecutionError(format!(
      "project root is not a directory: {}",
      request.project_root.display()
    )));
  }
  Ok(())
}

fn io_error(error: std::io::Error) -> ExecutionError {
  ExecutionError(error.to_string())
}

fn command_error(action: &str, stderr: &[u8]) -> ExecutionError {
  ExecutionError(format!(
    "{action} failed: {}",
    String::from_utf8_lossy(stderr).trim()
  ))
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn rejects_option_instead_of_recipe() {
    let request = RunRequest {
      project_root: PathBuf::from("."),
      recipe: "--evaluate".into(),
      arguments: Vec::new(),
    };
    assert!(validate_request(&request).is_err());
  }

  #[test]
  fn commands_use_argv_and_separate_just_options() {
    let executor = RecipeExecutor::new("just");
    let request = RunRequest {
      project_root: PathBuf::from("."),
      recipe: "test".into(),
      arguments: vec!["--shell".into(), "a; rm -rf /".into()],
    };
    let command = executor.command(&request);
    let args = command
      .get_args()
      .map(|arg| arg.to_string_lossy().into_owned())
      .collect::<Vec<_>>();
    assert_eq!(args, ["--", "test", "--shell", "a; rm -rf /"]);

    let prepare = executor.prepare_command(&request);
    let prepare_args = prepare
      .get_args()
      .map(|arg| arg.to_string_lossy().into_owned())
      .collect::<Vec<_>>();
    assert_eq!(
      prepare_args,
      ["--dry-run", "--", "test", "--shell", "a; rm -rf /"]
    );
  }

  #[cfg(unix)]
  #[test]
  fn prepare_rejects_function_call_before_dry_run() {
    use std::{fs, os::unix::fs::PermissionsExt};
    let directory = tempfile::tempdir().unwrap();
    let binary = directory.path().join("fake-just");
    let marker = directory.path().join("dry-run-started");
    fs::write(
      &binary,
      format!(
        "#!/bin/sh\nif [ \"$1\" = \"--dump\" ]; then echo '{{\"assignments\":{{\"x\":{{\"value\":[\"call\",\"shell\",\"touch marker\"]}}}}}}'; exit 0; fi\ntouch '{}'\n",
        marker.display()
      ),
    )
    .unwrap();
    let mut permissions = fs::metadata(&binary).unwrap().permissions();
    permissions.set_mode(0o700);
    fs::set_permissions(&binary, permissions).unwrap();

    let error = RecipeExecutor::new(&binary)
      .prepare(RunRequest {
        project_root: directory.path().into(),
        recipe: "test".into(),
        arguments: Vec::new(),
      })
      .unwrap_err();

    assert_eq!(
      error.to_string(),
      "safe preview unavailable: project contains function call `shell()`"
    );
    assert!(!marker.exists());
  }

  #[cfg(unix)]
  #[test]
  fn prepare_rejects_dotenv_command_before_dry_run() {
    use std::{fs, os::unix::fs::PermissionsExt};
    let directory = tempfile::tempdir().unwrap();
    let binary = directory.path().join("fake-just");
    let marker = directory.path().join("dry-run-started");
    fs::write(
      &binary,
      format!(
        "#!/bin/sh\nif [ \"$1\" = \"--dump\" ]; then echo '{{\"settings\":{{\"dotenv_command\":[\"generate-env\"]}}}}'; exit 0; fi\ntouch '{}'\n",
        marker.display()
      ),
    )
    .unwrap();
    let mut permissions = fs::metadata(&binary).unwrap().permissions();
    permissions.set_mode(0o700);
    fs::set_permissions(&binary, permissions).unwrap();

    let error = RecipeExecutor::new(&binary)
      .prepare(RunRequest {
        project_root: directory.path().into(),
        recipe: "test".into(),
        arguments: Vec::new(),
      })
      .unwrap_err();

    assert_eq!(
      error.to_string(),
      "safe preview unavailable: project configures dotenv-command"
    );
    assert!(!marker.exists());
  }

  #[test]
  fn typed_confirmation_must_match() {
    let decision = PolicyDecision::ConfirmTyped {
      phrase: "run deploy".into(),
    };
    assert!(
      authorize(
        &decision,
        &RunConfirmation::Typed {
          phrase: "run deploy".into()
        }
      )
      .is_ok()
    );
    assert!(authorize(&decision, &RunConfirmation::Confirmed).is_err());
  }

  #[test]
  fn denied_run_cannot_be_confirmed() {
    let decision = PolicyDecision::Deny {
      reason: "blocked".into(),
    };
    assert!(authorize(&decision, &RunConfirmation::Confirmed).is_err());
  }

  #[test]
  fn cancellation_token_is_shared() {
    let first = CancellationToken::default();
    let second = first.clone();
    second.cancel();
    assert!(first.is_cancelled());
  }

  #[cfg(unix)]
  #[test]
  fn streams_stdout_and_stderr_from_direct_process() {
    use std::{fs, os::unix::fs::PermissionsExt};
    let directory = tempfile::tempdir().unwrap();
    let binary = directory.path().join("fake-just");
    fs::write(
      &binary,
      "#!/bin/sh\nif [ \"$1\" = \"--dump\" ]; then echo '{}'; exit 0; fi\nif [ \"$1\" = \"--dry-run\" ]; then echo 'echo safe'; exit 0; fi\nprintf out\nprintf err >&2\n",
    )
    .unwrap();
    let mut permissions = fs::metadata(&binary).unwrap().permissions();
    permissions.set_mode(0o700);
    fs::set_permissions(&binary, permissions).unwrap();

    let executor = RecipeExecutor::new(binary);
    let prepared = executor
      .prepare(RunRequest {
        project_root: directory.path().into(),
        recipe: "hello".into(),
        arguments: Vec::new(),
      })
      .unwrap();
    let mut events = Vec::new();
    let completed = executor
      .execute_streaming(
        &prepared,
        &RunConfirmation::None,
        &CancellationToken::default(),
        |event| events.push(event),
      )
      .unwrap();
    assert!(completed.status.success());
    assert_eq!(completed.stdout, b"out");
    assert_eq!(completed.stderr, b"err");
    assert!(
      events
        .iter()
        .any(|event| matches!(event, RunEvent::Stdout { text } if text == "out"))
    );
    assert!(
      events
        .iter()
        .any(|event| matches!(event, RunEvent::Stderr { text } if text == "err"))
    );

    let collected = executor.execute(&prepared, &RunConfirmation::None).unwrap();
    assert_eq!(collected.stdout, b"out");
    assert_eq!(collected.stderr, b"err");
  }

  #[cfg(unix)]
  #[test]
  fn output_overflow_terminates_recipe_process_tree() {
    use std::{fs, os::unix::fs::PermissionsExt, time::Instant};
    let directory = tempfile::tempdir().unwrap();
    let binary = directory.path().join("fake-just");
    fs::write(
      &binary,
      "#!/bin/sh\nif [ \"$1\" = \"--dump\" ]; then echo '{}'; exit 0; fi\nif [ \"$1\" = \"--dry-run\" ]; then echo 'printf 12345'; exit 0; fi\nprintf 12345\nsleep 30 &\nwait\n",
    )
    .unwrap();
    let mut permissions = fs::metadata(&binary).unwrap().permissions();
    permissions.set_mode(0o700);
    fs::set_permissions(&binary, permissions).unwrap();

    let executor = RecipeExecutor::new(binary);
    let prepared = executor
      .prepare(RunRequest {
        project_root: directory.path().into(),
        recipe: "verbose".into(),
        arguments: Vec::new(),
      })
      .unwrap();
    let started = Instant::now();
    let mut events = Vec::new();
    let error = executor
      .execute_streaming_with_limit(
        &prepared,
        &RunConfirmation::None,
        &CancellationToken::default(),
        4,
        |event| events.push(event),
      )
      .unwrap_err();

    assert!(started.elapsed() < Duration::from_secs(3));
    assert_eq!(error.to_string(), "process stdout exceeded 4 byte limit");
    assert!(events.iter().any(|event| matches!(
      event,
      RunEvent::Exited {
        cancelled: false,
        ..
      }
    )));
  }

  #[cfg(unix)]
  #[test]
  fn cancellation_terminates_descendants_holding_output_pipes() {
    use std::{fs, os::unix::fs::PermissionsExt, time::Instant};
    let directory = tempfile::tempdir().unwrap();
    let binary = directory.path().join("fake-just");
    fs::write(
      &binary,
      "#!/bin/sh\nif [ \"$1\" = \"--dump\" ]; then echo '{}'; exit 0; fi\nif [ \"$1\" = \"--dry-run\" ]; then echo 'sleep 30'; exit 0; fi\nsleep 30 &\nwait\n",
    )
    .unwrap();
    let mut permissions = fs::metadata(&binary).unwrap().permissions();
    permissions.set_mode(0o700);
    fs::set_permissions(&binary, permissions).unwrap();

    let executor = RecipeExecutor::new(binary);
    let prepared = executor
      .prepare(RunRequest {
        project_root: directory.path().into(),
        recipe: "long-running".into(),
        arguments: Vec::new(),
      })
      .unwrap();
    let cancellation = CancellationToken::default();
    let cancellation_handle = cancellation.clone();
    let cancel_thread = thread::spawn(move || {
      thread::sleep(Duration::from_millis(100));
      cancellation_handle.cancel();
    });
    let started = Instant::now();
    let mut events = Vec::new();
    let completed = executor
      .execute_streaming(&prepared, &RunConfirmation::None, &cancellation, |event| {
        events.push(event)
      })
      .unwrap();
    cancel_thread.join().unwrap();

    assert!(started.elapsed() < Duration::from_secs(3));
    assert!(!completed.status.success());
    assert!(events.iter().any(|event| matches!(
      event,
      RunEvent::Exited {
        cancelled: true,
        ..
      }
    )));
  }

  #[cfg(windows)]
  #[test]
  fn cancellation_terminates_windows_job_descendants() {
    use std::{fs, time::Instant};
    let directory = tempfile::tempdir().unwrap();
    let binary = directory.path().join("fake-just.cmd");
    fs::write(
      &binary,
      "@echo off\r\nif \"%1\"==\"--dump\" (\r\n  echo {}\r\n  exit /b 0\r\n)\r\nif \"%1\"==\"--dry-run\" (\r\n  echo powershell.exe -NoProfile -Command \"Start-Sleep -Seconds 30\"\r\n  exit /b 0\r\n)\r\nstart \"\" /b powershell.exe -NoProfile -Command \"Start-Sleep -Seconds 30\"\r\npowershell.exe -NoProfile -Command \"Start-Sleep -Seconds 30\"\r\n",
    )
    .unwrap();

    let executor = RecipeExecutor::new(binary);
    let prepared = executor
      .prepare(RunRequest {
        project_root: directory.path().into(),
        recipe: "long-running".into(),
        arguments: Vec::new(),
      })
      .unwrap();
    let cancellation = CancellationToken::default();
    let cancellation_handle = cancellation.clone();
    let cancel_thread = thread::spawn(move || {
      thread::sleep(Duration::from_millis(250));
      cancellation_handle.cancel();
    });
    let started = Instant::now();
    let completed = executor
      .execute_streaming(&prepared, &RunConfirmation::None, &cancellation, |_| {})
      .unwrap();
    cancel_thread.join().unwrap();

    assert!(started.elapsed() < Duration::from_secs(5));
    assert!(!completed.status.success());
    assert!(completed.cancelled);
  }
}
