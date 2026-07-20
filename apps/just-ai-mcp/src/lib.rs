use std::io::{self, BufRead, Write};

mod catalog;
mod protocol;
mod tools;

use protocol::handle_line;

pub fn run_stdio() -> io::Result<()> {
  let stdin = io::stdin();
  let mut stdout = io::stdout().lock();
  for line in stdin.lock().lines() {
    let line = line?;
    if line.trim().is_empty() {
      continue;
    }
    if let Some(response) = handle_line(&line) {
      serde_json::to_writer(&mut stdout, &response)?;
      stdout.write_all(b"\n")?;
      stdout.flush()?;
    }
  }
  Ok(())
}
