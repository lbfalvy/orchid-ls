mod cmd;
mod comm;
mod ctx_map;
mod jrpc;
mod orc;
mod protocol;

use std::process;

use crate::cmd::{fs, init, logging};
use crate::comm::{stdin_ingress, stdout_write};
use crate::jrpc::JrpcServer;

fn main() {
  eprintln!("Starting Orchid LSP server");
  let mut srv = JrpcServer::new(stdout_write);
  init::attach(&mut srv);
  logging::attach(&mut srv);
  fs::attach(&mut srv);
  // code::attach(&mut srv);
  eprintln!("srv initialized");
  for message in stdin_ingress() {
    srv.recv(message)
  }
  eprintln!("stdin closed unexpectedly");
  process::exit(1);
}
