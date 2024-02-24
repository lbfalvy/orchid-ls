use std::io::{stdin, stdout, BufRead, Read, Write};
use std::iter;

use serde_json::Value;

/// Lock stdin and read LSP header-data blocks from it. Because stdin doesn't
/// offer packets, it's critically important that messages end at exactly the
/// specified number of bytes.
pub fn stdin_ingress() -> impl Iterator<Item = Value> {
  let mut stdin = stdin().lock();
  return iter::from_fn(move || {
    eprintln!("\nPolling for input");
    let mut length = None;
    // process all headers
    loop {
      let mut buf = String::new();
      stdin.read_line(&mut buf).unwrap();
      eprint!("Received header: {buf}");
      match buf.trim().split_once(':') {
        Some(("Content-Type", ct)) => match ct.trim().split_once("; charset=") {
          Some(("application/vscode-jsonrpc", "utf-8" | "utf8")) => (),
          // not a hard error because most likely the stream is standard LSP ASCII anyway
          _ => eprintln!("Unrecognized Content-Type header: \"{ct}\""),
        },
        Some(("Content-Length", cl)) => length = Some(cl.trim().parse().unwrap()),
        None if buf.trim().is_empty() => break,
        // Maybe this shouldn't be a hard error?
        _ => panic!("Unrecognized header \"{buf}\""),
      }
    }
    let mut line = vec![0u8; length.unwrap()];
    stdin.read_exact(&mut line).unwrap();
    // This should fail if we accidentally block on an extra character
    let val = serde_json::from_slice(&line).unwrap();
    eprintln!("Received message {val}");
    Some(val)
  });
}

/// Serialize and write a json-rpc message to stdout.
pub fn stdout_write(val: Value) {
  let mut out = stdout().lock();
  let text = serde_json::to_string(&val).unwrap();
  write!(out, "Content-Length: {}\r\n\r\n{}", text.len(), text).unwrap();
  out.flush().unwrap();
}
