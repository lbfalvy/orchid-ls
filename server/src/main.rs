mod cmd;
mod comm;
mod jrpc;
mod protocol;
mod ctx_map;
mod orc;

use std::process;

use serde_json::{json, Value};

use crate::cmd::{fs, init};
use crate::comm::{stdin_ingress, stdout_write};
use crate::jrpc::JrpcServer;

fn main() {
  eprintln!("Starting Orchid LSP server");
  let mut srv = JrpcServer::new(stdout_write);
  init::attach(&mut srv);
  fs::attach(&mut srv);
  eprintln!("srv initialized");
  for message in stdin_ingress() {
    srv.recv(message)
  }
  eprintln!("stdin closed unexpectedly");
  process::exit(1);
}
