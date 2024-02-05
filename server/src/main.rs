mod cmd;
mod comm;
mod jrpc;
mod protocol;

use std::process;

use serde_json::{json, Value};

use crate::comm::{stdio_ingress, stdout_write};
use crate::jrpc::JrpcServer;

fn main() {
  eprintln!("Starting Orchid LSP server");
  let mut srv = JrpcServer::new(stdout_write);
  srv.on_req_sync("initialize", |_| {
    Ok(json!({
      "serverInfo": {
        "name": "OrchidLS",
        "version": "0.0.1",
      },
      "capabilities": {}
    }))
  });
  srv.on_req_sync("shutdown", |_| Ok(Value::Null));
  srv.on_notif("exit", |_| process::exit(0));
  for message in stdio_ingress() {
    srv.recv(message)
  }
  eprintln!("stdin closed unexpectedly");
  process::exit(1);
}
