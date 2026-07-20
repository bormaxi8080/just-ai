use std::{
  error::Error,
  fmt::{self, Display, Formatter},
  io::{self, Read},
  process::{Command, Output, Stdio},
  sync::mpsc,
  thread,
  time::Duration,
};

pub(crate) const MAX_CAPTURE_BYTES: usize = 8 * 1024 * 1024;

pub(crate) fn capture(command: &mut Command) -> Result<Output, CaptureError> {
  capture_with_limit(command, MAX_CAPTURE_BYTES)
}

pub(crate) fn extend_with_limit(
  output: &mut Vec<u8>,
  bytes: &[u8],
  stream: &'static str,
  limit: usize,
) -> Result<(), CaptureError> {
  if output.len().saturating_add(bytes.len()) > limit {
    return Err(CaptureError::Limit { stream, limit });
  }
  output.extend_from_slice(bytes);
  Ok(())
}

fn capture_with_limit(command: &mut Command, limit: usize) -> Result<Output, CaptureError> {
  command.stdout(Stdio::piped()).stderr(Stdio::piped());
  let mut child = command.spawn().map_err(CaptureError::Io)?;
  let Some(stdout) = child.stdout.take() else {
    terminate(&mut child);
    return Err(CaptureError::MissingPipe("stdout"));
  };
  let Some(stderr) = child.stderr.take() else {
    terminate(&mut child);
    return Err(CaptureError::MissingPipe("stderr"));
  };
  let (issue_sender, issue_receiver) = mpsc::channel();
  let stdout_reader = spawn_reader(stdout, "stdout", limit, issue_sender.clone());
  let stderr_reader = spawn_reader(stderr, "stderr", limit, issue_sender);

  let status = loop {
    if issue_receiver.try_recv().is_ok() {
      let _ = child.kill();
      break child.wait().map_err(CaptureError::Io)?;
    }
    if let Some(status) = child.try_wait().map_err(CaptureError::Io)? {
      break status;
    }
    thread::sleep(Duration::from_millis(5));
  };

  let stdout = join_reader(stdout_reader)?;
  let stderr = join_reader(stderr_reader)?;
  Ok(Output {
    status,
    stdout,
    stderr,
  })
}

fn terminate(child: &mut std::process::Child) {
  let _ = child.kill();
  let _ = child.wait();
}

fn spawn_reader<R>(
  reader: R,
  stream: &'static str,
  limit: usize,
  issue_sender: mpsc::Sender<()>,
) -> thread::JoinHandle<Result<Vec<u8>, CaptureError>>
where
  R: Read + Send + 'static,
{
  thread::spawn(move || {
    let result = read_bounded(reader, stream, limit);
    if result.is_err() {
      let _ = issue_sender.send(());
    }
    result
  })
}

fn join_reader(
  reader: thread::JoinHandle<Result<Vec<u8>, CaptureError>>,
) -> Result<Vec<u8>, CaptureError> {
  reader.join().map_err(|_| CaptureError::ReaderPanicked)?
}

fn read_bounded(
  mut reader: impl Read,
  stream: &'static str,
  limit: usize,
) -> Result<Vec<u8>, CaptureError> {
  let mut output = Vec::with_capacity(limit.min(16 * 1024));
  let mut buffer = [0_u8; 8 * 1024];
  loop {
    let count = reader.read(&mut buffer).map_err(CaptureError::Io)?;
    if count == 0 {
      return Ok(output);
    }
    extend_with_limit(&mut output, &buffer[..count], stream, limit)?;
  }
}

#[derive(Debug)]
pub(crate) enum CaptureError {
  Io(io::Error),
  Limit { stream: &'static str, limit: usize },
  MissingPipe(&'static str),
  ReaderPanicked,
}

impl Display for CaptureError {
  fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
    match self {
      Self::Io(error) => write!(f, "failed to capture process output: {error}"),
      Self::Limit { stream, limit } => {
        write!(f, "process {stream} exceeded {limit} byte limit")
      }
      Self::MissingPipe(stream) => write!(f, "process {stream} pipe was unavailable"),
      Self::ReaderPanicked => write!(f, "process output reader panicked"),
    }
  }
}

impl Error for CaptureError {
  fn source(&self) -> Option<&(dyn Error + 'static)> {
    match self {
      Self::Io(error) => Some(error),
      Self::Limit { .. } | Self::MissingPipe(_) | Self::ReaderPanicked => None,
    }
  }
}

#[cfg(test)]
mod tests {
  use {super::*, std::io::Cursor};

  #[test]
  fn reader_accepts_exact_limit() {
    assert_eq!(
      read_bounded(Cursor::new(b"1234"), "stdout", 4).unwrap(),
      b"1234"
    );
  }

  #[test]
  fn reader_rejects_bytes_past_limit() {
    assert!(matches!(
      read_bounded(Cursor::new(b"12345"), "stderr", 4),
      Err(CaptureError::Limit {
        stream: "stderr",
        limit: 4
      })
    ));
  }

  #[cfg(unix)]
  #[test]
  fn capture_preserves_output_and_exit_status() {
    let output = capture_with_limit(
      Command::new("sh").args(["-c", "printf out; printf err >&2; exit 7"]),
      4,
    )
    .unwrap();
    assert_eq!(output.status.code(), Some(7));
    assert_eq!(output.stdout, b"out");
    assert_eq!(output.stderr, b"err");
  }

  #[cfg(unix)]
  #[test]
  fn capture_rejects_oversized_process_output() {
    let error = capture_with_limit(Command::new("printf").arg("12345"), 4).unwrap_err();
    assert!(matches!(
      error,
      CaptureError::Limit {
        stream: "stdout",
        limit: 4
      }
    ));
  }

  #[cfg(unix)]
  #[test]
  fn capture_rejects_oversized_process_error() {
    let error =
      capture_with_limit(Command::new("sh").args(["-c", "printf 12345 >&2"]), 4).unwrap_err();
    assert!(matches!(
      error,
      CaptureError::Limit {
        stream: "stderr",
        limit: 4
      }
    ));
  }
}
