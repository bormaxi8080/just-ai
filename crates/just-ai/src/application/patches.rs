use {
  crate::bounded_file::{self, MAX_EDITABLE_FILE_BYTES},
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
  bounded_file::ensure_text_limit(reviewed, "reviewed content", MAX_EDITABLE_FILE_BYTES)?;
  bounded_file::ensure_text_limit(proposed, "proposed content", MAX_EDITABLE_FILE_BYTES)?;
  let current = bounded_file::read_utf8(path, MAX_EDITABLE_FILE_BYTES)?;
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

  #[test]
  fn rejects_oversized_current_file_without_writing() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("justfile");
    fs::write(&path, vec![b'a'; MAX_EDITABLE_FILE_BYTES + 1]).unwrap();

    let error = apply_reviewed_change(&path, "reviewed", "proposed").unwrap_err();
    assert_eq!(error.kind(), io::ErrorKind::InvalidData);
    assert_eq!(
      fs::metadata(path).unwrap().len(),
      u64::try_from(MAX_EDITABLE_FILE_BYTES + 1).unwrap()
    );
  }

  #[test]
  fn rejects_oversized_proposal_without_writing() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("justfile");
    fs::write(&path, "reviewed").unwrap();
    let proposed = "x".repeat(MAX_EDITABLE_FILE_BYTES + 1);

    let error = apply_reviewed_change(&path, "reviewed", &proposed).unwrap_err();
    assert_eq!(error.kind(), io::ErrorKind::InvalidInput);
    assert_eq!(fs::read_to_string(path).unwrap(), "reviewed");
  }
}
