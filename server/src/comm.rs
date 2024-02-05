use std::io::{stdin, stdout, Write};

use serde_json::Value;

pub fn stdio_ingress() -> impl Iterator<Item = Value> {
  stdin().lines().map(Result::unwrap).filter_map(|line| {
    let line = line.trim();
    if line.starts_with('{') {
      Some(serde_json::from_str::<Value>(line).unwrap())
    } else {
      if line.is_empty() || line.starts_with("Content-Length:") {
        // We don't need content-length and never need to react to the value
      } else if let Some(ct) = line.strip_prefix("Content-Type:") {
        if let Some((ty, cs)) = ct.split_once("; charset=") {
          let cs_known = ["utf-8", "utf8"].contains(&cs.trim());
          // to be extended
          let ty_known = ty.trim() == "application/vscode-jsonrpc";
          if cs_known && ty_known {
            return None;
          }
        }
        eprintln!("Unrecognized Content-Type header: \"{ct}\"");
      } else {
        eprintln!("Unexpected line: \"{line}\"");
      }
      None
    }
  })
}

pub fn stdout_write(val: Value) {
  let mut out = stdout().lock();
  write!(out, "{}", serde_json::to_string(&val).unwrap()).unwrap();
  out.flush().unwrap();
}
