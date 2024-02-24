use serde_json::json;

use crate::jrpc::{JrpcServer, Session};

enum TraceValue {
  Off,
  Messages,
  Verbose,
}

#[allow(unused)] // TODO: convert some long-lived eprintln lines to this
pub fn log(session: Session, message: &str, verbose: impl FnOnce() -> String) {
  let msg = match session.lock().get() {
    Some(TraceValue::Off) | None => return,
    Some(TraceValue::Messages) => json!({ "message": message }),
    Some(TraceValue::Verbose) => json!({ "message": message, "verbose": verbose()}),
  };
  session.notify("$/logTrace", msg);
}

pub fn attach(srv: &mut JrpcServer) {
  srv.on_notif("$/setTrace", |val, ctx| {
    let value = match val.unwrap()["value"].as_str().unwrap() {
      "off" => TraceValue::Off,
      "messages" => TraceValue::Messages,
      "verbose" => TraceValue::Verbose,
      s => panic!("Unrecognized trace value \"{s}\""),
    };
    ctx.set(value);
  });
}
