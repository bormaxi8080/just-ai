use {
  crate::config::BoundedFileConfig,
  std::{
    fs::File,
    io::{self, Read},
    path::Path,
  },
};

pub struct Prefix {
  pub bytes: Vec<u8>,
  pub truncated: bool,
}

pub fn read_prefix(path: &Path, limit: usize) -> io::Result<Prefix> {
  let file = File::open(path)?;
  let take_limit = u64::try_from(limit).unwrap_or(u64::MAX).saturating_add(1);
  let mut bytes = Vec::with_capacity(limit.min(16 * 1024));
  file.take(take_limit).read_to_end(&mut bytes)?;
  let truncated = bytes.len() > limit;
  bytes.truncate(limit);
  Ok(Prefix { bytes, truncated })
}

pub fn read_utf8(path: &Path, limit: usize) -> io::Result<String> {
  let prefix = read_prefix(path, limit)?;
  if prefix.truncated {
    return Err(io::Error::new(
      io::ErrorKind::InvalidData,
      format!("{} exceeds {limit} byte limit", path.display()),
    ));
  }
  String::from_utf8(prefix.bytes).map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))
}

pub fn max_editable_file_bytes() -> usize {
  BoundedFileConfig::default().max_editable_file_bytes
}

pub fn ensure_text_limit(text: &str, label: &str, limit: usize) -> io::Result<()> {
  if text.len() > limit {
    return Err(io::Error::new(
      io::ErrorKind::InvalidInput,
      format!("{label} exceeds {limit} byte limit"),
    ));
  }
  Ok(())
}
