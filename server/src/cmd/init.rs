use std::process;

use serde::Deserialize;
use serde_json::{json, Value};

use super::fs::WorkspaceCtx;
use crate::jrpc::JrpcServer;
use crate::protocol::document::{FileUri, WspaceEnt};

pub fn attach(srv: &mut JrpcServer) {
  srv.on_req_sync("initialize", |init, session| {
    let init = init.unwrap();
    let wf = &init["workspaceFolders"];
    session.set(match wf.as_array() {
      None => wf.as_null().map(|()| WorkspaceCtx::new([])).unwrap(),
      Some(ents) => WorkspaceCtx::new((ents.iter()).map(|ent| WspaceEnt {
        name: String::deserialize(&ent["name"]).unwrap(),
        uri: FileUri::deserialize(&ent["uri"]).unwrap(),
      })),
    });
    Ok(json!({
      "serverInfo": {
        "name": "OrchidLS",
        "version": "0.0.1",
      },
      "capabilities": {
        "workspace": {
          "workspaceFolders": { "supported": true, "changeNotifications": false },
        },
        "textDocumentSync": {
          "openClose": true,
          "change": 1,
        },
        // "semanticTokensProvider": semantic_tokens_provider(),
      }
    }))
  });
  srv.on_notif("initialized", move |_v, session| {
    eprintln!("Received notif");
    session.request(
      "client/registerCapability",
      json!({
        "registrations": [{
          "id": "file-watcher-registration-id",
          "method": "workspace/didChangeWatchedFiles",
          "registerOptions": {
            "documentSelector": [{ "language": "orchid", "scheme": "file" }],
            "watchers": [{
              "globPattern": "**/*.orc"
            }]
          }
        }],
      }),
      |res| {
        res.unwrap();
        eprintln!("Resolved file watcher registration");
      },
    )
  });
  srv.on_req_sync("shutdown", |_, _| {
    eprintln!("Shutting down");
    Ok(Value::Null)
  });
  srv.on_notif("exit", |_, _| {
    eprintln!("Exiting");
    process::exit(0)
  });
}
