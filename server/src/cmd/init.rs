use std::process;

use serde_json::{json, Value};

use super::fs::WorkspaceCtx;
use crate::jrpc::JrpcServer;
use crate::protocol::document::WspaceEnt;

pub fn attach(srv: &mut JrpcServer) {
  srv.on_req_sync("initialize", |init, ctx| {
    let init = init.unwrap();
    let wf = &init["workspaceFolders"];
    ctx.set(match wf.as_array() {
      None => wf.as_null().map(|()| WorkspaceCtx::new([])).unwrap(),
      Some(ents) => WorkspaceCtx::new((ents.iter()).map(|ent| WspaceEnt {
        name: ent["name"].as_str().unwrap().to_owned(),
        uri: ent["uri"].as_str().unwrap().to_owned(),
      })),
    });
    Ok(json!({
      "serverInfo": {
        "name": "OrchidLS",
        "version": "0.0.1",
      },
      "capabilities": {
        "textDocumentSync": {
          "openClose": true,
          "change": 1,
        }
      }
    }))
  });
  srv.on_req_sync("shutdown", |_, _| Ok(Value::Null));
  srv.on_notif("exit", |_, _| process::exit(0));
}
