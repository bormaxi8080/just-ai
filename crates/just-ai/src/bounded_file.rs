use std::{
  fs::File,
  io::{self, Read},
  path::Path,
};

pub(crate) const MAX_EDITABLE_FILE_BYTES: usize = 1024 * 1024;

pub(crate) struct Prefix {
  pub(crate) bytes: Vec<u8>,
  pub(crate) truncated: bool,
}

pub(crate) fn read_prefix(path: &Path, limit: usize) -> io::Result<Prefix> {
  let file = File::open(path)?;
  let take_limit = u64::try_from(limit).unwrap_or(u64::MAX).saturating_add(1);
  let mut bytes = Vec::with_capacity(limit.min(16 * 1024));
  file.take(take_limit).read_to_end(&mut bytes)?;
  let truncated = bytes.len() > limit;
  bytes.truncate(limit);
  Ok(Prefix { bytes, truncated })
}

pub(crate) fn read_utf8(path: &Path, limit: usize) -> io::Result<String> {
  let prefix = read_prefix(path, limit)?;
  if prefix.truncated {
    return Err(io::Error::new(
      io::ErrorKind::InvalidData,
      format!("{} exceeds {limit} byte limit", path.display()),
    ));
  }
  String::from_utf8(prefix.bytes).map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))
}

pub(crate) fn ensure_text_limit(text: &str, label: &str, limit: usize) -> io::Result<()> {
  if text.len() > limit {
    return Err(io::Error::new(
      io::ErrorKind::InvalidInput,
      format!("{label} exceeds {limit} byte limit"),
    ));
  }
  Ok(())
}

#[cfg(test)]
mod tests {
  use {super::*, std::fs};

  #[test]
  fn prefix_accepts_exact_limit_and_marks_overflow() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("input");
    fs::write(&path, b"1234").unwrap();
    let exact = read_prefix(&path, 4).unwrap();
    assert_eq!(exact.bytes, b"1234");
    assert!(!exact.truncated);

    fs::write(&path, b"12345").unwrap();
    let truncated = read_prefix(&path, 4).unwrap();
    assert_eq!(truncated.bytes, b"1234");
    assert!(truncated.truncated);
  }

  #[test]
  fn utf8_read_rejects_overflow_and_invalid_text() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("input");
    fs::write(&path, b"12345").unwrap();
    assert_eq!(
      read_utf8(&path, 4).unwrap_err().kind(),
      io::ErrorKind::InvalidData
    );

    fs::write(&path, [0xff]).unwrap();
    assert_eq!(
      read_utf8(&path, 4).unwrap_err().kind(),
      io::ErrorKind::InvalidData
    );
  }
}
