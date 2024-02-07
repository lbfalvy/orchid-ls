use std::io::{stdin, stdout, Read, Write};
use std::{cmp, iter};

use serde_json::Value;

pub fn stdin_ingress() -> impl Iterator<Item = Value> {
  let mut buffer = Vec::<u8>::new();
  let mut length = 0;
  let mut reader = 0;
  return iter::from_fn(move || {
    Some(loop {
      let mut rbuf = [0u8; 1024];
      let len = stdin().lock().read(&mut rbuf).unwrap();
      buffer.extend_from_slice(&rbuf[..len]);
      let text = if let Ok(s) = std::str::from_utf8(&buffer[reader..]) { s } else { continue };
      if let Some(s) = text.strip_prefix("Content-Length: ") {
        let (cl_str, rest) = if let Some(p) = s.split_once("\r\n") { p } else { continue };
        length = cl_str.parse().unwrap();
        reader = buffer.len() - rest.len();
      } else if let Some(s) = text.strip_prefix("Content-Type: ") {
        let (ct_str, rest) = if let Some(p) = s.split_once("\r\n") { p } else { continue };
        reader = buffer.len() - rest.len();
        if let Some((ty, cs)) = ct_str.split_once("; charset=") {
          let cs_known = ["utf-8", "utf8"].contains(&cs.trim());
          // to be extended
          let ty_known = ty.trim() == "application/vscode-jsonrpc";
          if cs_known && ty_known {
            continue;
          }
        }
        eprintln!("Unrecognized Content-Type header: \"{ct_str}\"");
      } else if let Some(tail) = text.strip_prefix("\r\n") {
        if tail.len() < length {
          continue
        }
        let value = serde_json::from_str::<Value>(&tail[..length]).unwrap();
        buffer = tail[length..].as_bytes().to_vec();
        reader = 0;
        break value;
      }
    })
  });
}

pub fn stdout_write(val: Value) {
  let mut out = stdout().lock();
  let text = serde_json::to_string(&val).unwrap();
  write!(out, "Content-Length: {}\r\n\r\n{}", text.len(), text).unwrap();
  out.flush().unwrap();
}
