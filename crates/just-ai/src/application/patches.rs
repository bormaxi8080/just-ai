use {
  std::{
    fs,
    io::{self, Write},
    path::Path,
  },
  tempfile::NamedTempFile,
};

/// Atomically replace a file only if it still contains the reviewed content.
///
/// This prevents an AI proposal from overwriting edits made after its diff was
/// displayed. The temporary file is created beside the target so persistence
/// remains an atomic same-filesystem rename.
pub fn apply_reviewed_change(path: &Path, reviewed: &str, proposed: &str) -> io::Result<()> {
  let current = fs::read_to_string(path)?;
  if current != reviewed {
    return Err(io::Error::new(
      io::ErrorKind::AlreadyExists,
      "target changed after the proposal was reviewed",
    ));
  }

  let parent = path.parent().unwrap_or_else(|| Path::new("."));
  let permissions = fs::metadata(path)?.permissions();
  let mut temporary = NamedTempFile::new_in(parent)?;
  temporary.write_all(proposed.as_bytes())?;
  temporary.as_file().sync_all()?;
  temporary.as_file().set_permissions(permissions)?;
  temporary.persist(path).map_err(|error| error.error)?;
  Ok(())
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn applies_reviewed_change() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("justfile");
    fs::write(&path, "test:\n  cargo test\n").unwrap();

    apply_reviewed_change(
      &path,
      "test:\n  cargo test\n",
      "test:\n  cargo test --all\n",
    )
    .unwrap();

    assert_eq!(
      fs::read_to_string(path).unwrap(),
      "test:\n  cargo test --all\n"
    );
  }

  #[test]
  fn refuses_to_overwrite_concurrent_edit() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("justfile");
    fs::write(&path, "changed\n").unwrap();

    let error = apply_reviewed_change(&path, "reviewed\n", "proposed\n").unwrap_err();
    assert_eq!(error.kind(), io::ErrorKind::AlreadyExists);
    assert_eq!(fs::read_to_string(path).unwrap(), "changed\n");
  }
}
