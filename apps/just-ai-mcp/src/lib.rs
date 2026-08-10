use std::io::{self, BufRead, Write};

mod catalog;
mod protocol;
mod tools;

use protocol::{handle_message, oversized_request};

const MAX_MESSAGE_BYTES: usize = 1024 * 1024;

enum Frame {
  End,
  Message(Vec<u8>),
  Oversized,
}

pub fn run_stdio() -> io::Result<()> {
  let stdin = io::stdin();
  let mut stdin = stdin.lock();
  let mut stdout = io::stdout().lock();
  serve(&mut stdin, &mut stdout)
}

fn serve(reader: &mut impl BufRead, writer: &mut impl Write) -> io::Result<()> {
  loop {
    let response = match read_frame(reader)? {
      Frame::End => break,
      Frame::Message(message) if message.iter().all(u8::is_ascii_whitespace) => continue,
      Frame::Message(message) => handle_message(&message),
      Frame::Oversized => Some(oversized_request()),
    };
    if let Some(response) = response {
      serde_json::to_writer(&mut *writer, &response)?;
      writer.write_all(b"\n")?;
      writer.flush()?;
    }
  }
  Ok(())
}

fn read_frame(reader: &mut impl BufRead) -> io::Result<Frame> {
  let mut message = Vec::new();
  let mut oversized = false;
  loop {
    let (consumed, terminated) = {
      let available = reader.fill_buf()?;
      if available.is_empty() {
        return Ok(if oversized {
          Frame::Oversized
        } else if message.is_empty() {
          Frame::End
        } else {
          Frame::Message(message)
        });
      }
      let newline = available.iter().position(|byte| *byte == b'\n');
      let content_length = newline.unwrap_or(available.len());
      if !oversized {
        if message.len().saturating_add(content_length) > MAX_MESSAGE_BYTES {
          oversized = true;
          message.clear();
        } else {
          message.extend_from_slice(&available[..content_length]);
        }
      }
      (
        content_length + usize::from(newline.is_some()),
        newline.is_some(),
      )
    };
    reader.consume(consumed);
    if terminated {
      return Ok(if oversized {
        Frame::Oversized
      } else {
        Frame::Message(message)
      });
    }
  }
}

#[cfg(test)]
mod tests {
  use {
    super::*,
    serde_json::Value,
    std::io::{BufReader, Cursor},
  };

  fn responses(output: &[u8]) -> Vec<Value> {
    output
      .split(|byte| *byte == b'\n')
      .filter(|line| !line.is_empty())
      .map(|line| serde_json::from_slice(line).unwrap())
      .collect()
  }

  #[test]
  fn oversized_frame_is_rejected_and_next_frame_is_processed() {
    let mut input = vec![b'x'; MAX_MESSAGE_BYTES + 1];
    input.extend_from_slice(b"\n{\"jsonrpc\":\"2.0\",\"id\":8,\"method\":\"ping\"}\n");
    let mut reader = BufReader::with_capacity(4096, Cursor::new(input));
    let mut output = Vec::new();

    serve(&mut reader, &mut output).unwrap();

    let responses = responses(&output);
    assert_eq!(responses.len(), 2);
    assert_eq!(
      responses[0].pointer("/error/code").and_then(Value::as_i64),
      Some(-32600)
    );
    assert_eq!(responses[1].get("id").and_then(Value::as_i64), Some(8));
  }

  #[test]
  fn frame_at_size_limit_is_accepted() {
    let mut input = vec![b' '; MAX_MESSAGE_BYTES];
    input.push(b'\n');
    let mut reader = BufReader::with_capacity(4096, Cursor::new(input));

    match read_frame(&mut reader).unwrap() {
      Frame::Message(message) => assert_eq!(message.len(), MAX_MESSAGE_BYTES),
      Frame::End | Frame::Oversized => panic!("frame at limit must be accepted"),
    }
  }

  #[test]
  fn invalid_utf8_is_a_parse_error_and_does_not_stop_transport() {
    let input = b"\xff\n{\"jsonrpc\":\"2.0\",\"id\":9,\"method\":\"ping\"}\n";
    let mut reader = BufReader::new(Cursor::new(input));
    let mut output = Vec::new();

    serve(&mut reader, &mut output).unwrap();

    let responses = responses(&output);
    assert_eq!(responses.len(), 2);
    assert_eq!(
      responses[0].pointer("/error/code").and_then(Value::as_i64),
      Some(-32700)
    );
    assert_eq!(responses[1].get("id").and_then(Value::as_i64), Some(9));
  }
}
