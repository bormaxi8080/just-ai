use {
  crate::bounded_file::{self, max_editable_file_bytes},
  std::{
    fs,
    io::{self, Write},
    path::Path,
  },
  tempfile::NamedTempFile,
};

/// Replace reviewed content under a cooperative writer lock. Editors that do
/// not use this lock can still race a filesystem rename; content is rechecked
/// immediately before commit to detect intervening edits where possible.
pub fn apply_reviewed_change(path: &Path, reviewed: &str, proposed: &str) -> io::Result<()> {
  apply_reviewed_changes(path, reviewed, proposed, &[])
}

/// Create new sibling files without clobbering, then commit the root file.
/// On a returned error, newly created files are rolled back. The root rename
/// is the commit point; this is not a crash-atomic multi-file transaction.
pub(crate) fn apply_reviewed_changes(
  path: &Path,
  reviewed: &str,
  proposed: &str,
  additions: &[(std::path::PathBuf, String)],
) -> io::Result<()> {
  bounded_file::ensure_text_limit(reviewed, "reviewed content", max_editable_file_bytes())?;
  bounded_file::ensure_text_limit(proposed, "proposed content", max_editable_file_bytes())?;
  let parent = path.parent().unwrap_or_else(|| Path::new("."));
  let lock_path = parent.join(format!(
    ".{}.just-ai-write.lock",
    path.file_name().unwrap_or_default().to_string_lossy()
  ));
  let _lock = WriteLock::acquire(lock_path)?;
  check_reviewed(path, reviewed)?;
  let permissions = fs::metadata(path)?.permissions();
  let mut temporary = NamedTempFile::new_in(parent)?;
  temporary.write_all(proposed.as_bytes())?;
  temporary.as_file().set_permissions(permissions)?;
  temporary.as_file().sync_all()?;
  let mut staged = Vec::new();
  let mut targets = std::collections::HashSet::new();
  for (target, content) in additions {
    if target.parent() != Some(parent) || target == path || !targets.insert(target.clone()) {
      return Err(io::Error::new(
        io::ErrorKind::InvalidInput,
        "new files must be distinct siblings of the root justfile",
      ));
    }
    bounded_file::ensure_text_limit(content, "module content", max_editable_file_bytes())?;
    let mut file = NamedTempFile::new_in(parent)?;
    file.write_all(content.as_bytes())?;
    file
      .as_file()
      .set_permissions(fs::metadata(path)?.permissions())?;
    file.as_file().sync_all()?;
    staged.push((target, file));
  }
  let mut created = Vec::new();
  let result = (|| {
    for (target, file) in staged {
      file
        .persist_noclobber(target)
        .map_err(|error| error.error)?;
      created.push(target);
    }
    check_reviewed(path, reviewed)?;
    temporary.persist(path).map_err(|error| error.error)?;
    Ok(())
  })();
  if let Err(error) = result {
    for target in created {
      fs::remove_file(target).map_err(|rollback| {
        io::Error::other(format!(
          "{error}; rollback failed for {}: {rollback}",
          target.display()
        ))
      })?;
    }
    return Err(error);
  }
  Ok(())
}

fn check_reviewed(path: &Path, reviewed: &str) -> io::Result<()> {
  if fs::symlink_metadata(path)?.file_type().is_symlink() {
    return Err(io::Error::new(
      io::ErrorKind::InvalidInput,
      "refusing to replace a symlink",
    ));
  }
  if bounded_file::read_utf8(path, max_editable_file_bytes())? != reviewed {
    return Err(io::Error::new(
      io::ErrorKind::AlreadyExists,
      "target changed after the proposal was reviewed",
    ));
  }
  Ok(())
}

struct WriteLock {
  path: std::path::PathBuf,
  file: Option<fs::File>,
}

impl WriteLock {
  fn acquire(path: std::path::PathBuf) -> io::Result<Self> {
    let file = fs::OpenOptions::new()
      .write(true)
      .create_new(true)
      .open(&path)?;
    Ok(Self {
      path,
      file: Some(file),
    })
  }
}

impl Drop for WriteLock {
  fn drop(&mut self) {
    drop(self.file.take());
    let _ = fs::remove_file(&self.path);
  }
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
    fs::write(&path, vec![b'a'; max_editable_file_bytes() + 1]).unwrap();

    let error = apply_reviewed_change(&path, "reviewed", "proposed").unwrap_err();
    assert_eq!(error.kind(), io::ErrorKind::InvalidData);
    assert_eq!(
      fs::metadata(path).unwrap().len(),
      u64::try_from(max_editable_file_bytes() + 1).unwrap()
    );
  }

  #[test]
  fn rejects_oversized_proposal_without_writing() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("justfile");
    fs::write(&path, "reviewed").unwrap();
    let proposed = "x".repeat(max_editable_file_bytes() + 1);

    let error = apply_reviewed_change(&path, "reviewed", &proposed).unwrap_err();
    assert_eq!(error.kind(), io::ErrorKind::InvalidInput);
    assert_eq!(fs::read_to_string(path).unwrap(), "reviewed");
  }
}

#[cfg(test)]
mod transaction_tests {
  use super::*;

  #[test]
  fn rolls_back_new_modules_when_a_later_file_exists() {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path().join("justfile");
    let first = directory.path().join("first.just");
    let existing = directory.path().join("existing.just");
    fs::write(&root, "reviewed").unwrap();
    fs::write(&existing, "keep").unwrap();
    let result = apply_reviewed_changes(
      &root,
      "reviewed",
      "proposed",
      &[
        (first.clone(), "new".into()),
        (existing.clone(), "replace".into()),
      ],
    );
    assert!(result.is_err());
    assert!(!first.exists());
    assert_eq!(fs::read_to_string(existing).unwrap(), "keep");
    assert_eq!(fs::read_to_string(root).unwrap(), "reviewed");
  }

  #[test]
  fn refuses_another_writer_and_cleans_up_own_lock() {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path().join("justfile");
    fs::write(&root, "reviewed").unwrap();
    let lock_path = directory.path().join(".justfile.just-ai-write.lock");
    let lock = WriteLock::acquire(lock_path.clone()).unwrap();
    assert!(apply_reviewed_change(&root, "reviewed", "proposed").is_err());
    drop(lock);
    apply_reviewed_change(&root, "reviewed", "proposed").unwrap();
    assert!(!lock_path.exists());
  }
}
