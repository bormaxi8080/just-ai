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
    let mut command = Command::new(&self.just_binary);
    command
      .current_dir(&request.project_root)
      .arg("--dry-run")
      .arg(&request.recipe)
      .args(&request.arguments);
    let output = command.output().map_err(io_error)?;
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
    let current = self.prepare(prepared.request.clone())?;
    if &current != prepared {
      return Err(ExecutionError(
        "recipe preview or policy changed after preparation".into(),
      ));
    }
    authorize(&current.policy, confirmation)?;

    let output = self.command(&prepared.request).output().map_err(io_error)?;
    Ok(CompletedRun {
      status: output.status,
      stdout: output.stdout,
      stderr: output.stderr,
      cancelled: false,
    })
  }

  pub fn execute_streaming<F>(
    &self,
    prepared: &PreparedRun,
    confirmation: &RunConfirmation,
    cancellation: &CancellationToken,
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
    configure_process_tree(&mut command);
    let mut child = command.spawn().map_err(io_error)?;
    let stdout = child
      .stdout
      .take()
      .ok_or_else(|| ExecutionError("stdout pipe missing".into()))?;
    let stderr = child
      .stderr
      .take()
      .ok_or_else(|| ExecutionError("stderr pipe missing".into()))?;
    let (sender, receiver) = mpsc::channel();
    stream_reader(stdout, StreamKind::Stdout, sender.clone());
    stream_reader(stderr, StreamKind::Stderr, sender);

    let mut stdout_bytes = Vec::new();
    let mut stderr_bytes = Vec::new();
    let mut closed = 0;
    let mut cancelled = false;
    while closed < 2 {
      if cancellation.is_cancelled() && !cancelled {
        terminate_process_tree(&mut child)?;
        cancelled = true;
      }
      match receiver.recv_timeout(Duration::from_millis(25)) {
        Ok(StreamMessage::Data(StreamKind::Stdout, bytes)) => {
          stdout_bytes.extend_from_slice(&bytes);
          emit(RunEvent::Stdout {
            text: String::from_utf8_lossy(&bytes).into_owned(),
          });
        }
        Ok(StreamMessage::Data(StreamKind::Stderr, bytes)) => {
          stderr_bytes.extend_from_slice(&bytes);
          emit(RunEvent::Stderr {
            text: String::from_utf8_lossy(&bytes).into_owned(),
          });
        }
        Ok(StreamMessage::Closed) => closed += 1,
        Err(mpsc::RecvTimeoutError::Timeout) => {}
        Err(mpsc::RecvTimeoutError::Disconnected) => break,
      }
    }
    let status = child.wait().map_err(io_error)?;
    emit(RunEvent::Exited {
      code: status.code(),
      cancelled,
    });
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
      .arg(&request.recipe)
      .args(&request.arguments);
    command
  }
}

#[cfg(unix)]
fn configure_process_tree(command: &mut Command) {
  use std::os::unix::process::CommandExt;
  command.process_group(0);
}

#[cfg(not(unix))]
fn configure_process_tree(_command: &mut Command) {}

#[cfg(unix)]
fn terminate_process_tree(child: &mut std::process::Child) -> Result<(), ExecutionError> {
  let process_group = i32::try_from(child.id())
    .map_err(|_| ExecutionError("child process id exceeds platform range".into()))?;
  // SAFETY: `process_group` is the positive PID returned by `Child::id`; its
  // negation intentionally addresses only the isolated group configured above.
  let result = unsafe { libc::kill(-process_group, libc::SIGKILL) };
  if result == 0 {
    return Ok(());
  }
  let error = std::io::Error::last_os_error();
  if error.raw_os_error() == Some(libc::ESRCH) {
    Ok(())
  } else {
    Err(io_error(error))
  }
}

#[cfg(not(unix))]
fn terminate_process_tree(child: &mut std::process::Child) -> Result<(), ExecutionError> {
  child.kill().map_err(io_error)
}

#[derive(Clone, Copy)]
enum StreamKind {
  Stdout,
  Stderr,
}

enum StreamMessage {
  Data(StreamKind, Vec<u8>),
  Closed,
}

fn stream_reader(
  mut reader: impl Read + Send + 'static,
  kind: StreamKind,
  sender: mpsc::Sender<StreamMessage>,
) {
  thread::spawn(move || {
    let mut buffer = [0_u8; 4096];
    loop {
      match reader.read(&mut buffer) {
        Ok(0) | Err(_) => break,
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
  fn command_uses_argv_without_shell() {
    let executor = RecipeExecutor::new("just");
    let request = RunRequest {
      project_root: PathBuf::from("."),
      recipe: "test".into(),
      arguments: vec!["a; rm -rf /".into()],
    };
    let command = executor.command(&request);
    let args = command
      .get_args()
      .map(|arg| arg.to_string_lossy().into_owned())
      .collect::<Vec<_>>();
    assert_eq!(args, ["test", "a; rm -rf /"]);
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
      "#!/bin/sh\nif [ \"$1\" = \"--dry-run\" ]; then echo 'echo safe'; exit 0; fi\nprintf out\nprintf err >&2\n",
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
  }

  #[cfg(unix)]
  #[test]
  fn cancellation_terminates_descendants_holding_output_pipes() {
    use std::{fs, os::unix::fs::PermissionsExt, time::Instant};
    let directory = tempfile::tempdir().unwrap();
    let binary = directory.path().join("fake-just");
    fs::write(
      &binary,
      "#!/bin/sh\nif [ \"$1\" = \"--dry-run\" ]; then echo 'sleep 30'; exit 0; fi\nsleep 30 &\nwait\n",
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
}
