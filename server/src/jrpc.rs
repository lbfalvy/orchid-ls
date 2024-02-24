use std::collections::HashMap;
use std::ops::{Deref, DerefMut};
use std::sync::atomic::AtomicI64;
use std::sync::{atomic, Arc, Mutex, MutexGuard};
use std::{fmt, mem};

use anyhow::anyhow;
use serde::Deserialize;
use serde_json::{json, Value};
use trait_set::trait_set;

use crate::ctx_map::{Ctx, CtxMap};
use crate::protocol::error::LSPErrCode;

static NEXT_REQ: AtomicI64 = AtomicI64::new(0);

#[derive(Clone)]
pub struct Abort(Arc<atomic::AtomicBool>);
impl Abort {
  pub fn new() -> Self { Self(Arc::new(atomic::AtomicBool::new(false))) }
  pub fn abort(&self) { self.0.store(true, atomic::Ordering::Release) }
  /// Check if the abort flag has been set with relaxed memory ordering. The
  /// relaxed order avoids blocking optimizations, so this can be called
  /// frequently in the processing pipeline to ensure timely abort detection.
  pub fn aborted(&self) -> bool { self.0.load(atomic::Ordering::Relaxed) }
  /// Check if the abort flag has been set with strict memory ordering. This
  /// prevents many optimizations across the load so it should be used
  /// sparingly, but it enables the abort to double as an expiry marker. If the
  /// task locks a shared work tracker then calls this function, it can be
  /// certain that no other task has fulfilled its task, provided that doing so
  /// would involve aborting this task.
  pub fn is_valid(&self) -> bool { !self.0.load(atomic::Ordering::Acquire) }
}

pub struct AsyncReq {
  name: String,
  id: i64,
  params: Option<Value>,
  abort: Abort,
  resolved: bool,
  comm: Session,
}
#[allow(dead_code)] // this struct isn't being used
impl AsyncReq {
  pub fn name(&self) -> &str { self.name.as_str() }
  pub fn params(&self) -> Option<&Value> { self.params.as_ref() }
  pub fn aborted(&self) -> bool { self.abort.aborted() }
  pub fn session(&self) -> &Session { &self.comm }
  pub fn resolve(mut self, result: anyhow::Result<Value>) { self.resolve_impl(result) }
  fn resolve_impl(&mut self, result: anyhow::Result<Value>) {
    self.resolved = true;
    self.comm.0.lock().unwrap().send_resp(self.id, result)
  }
}
impl Drop for AsyncReq {
  fn drop(&mut self) {
    if !self.resolved {
      if self.abort.aborted() {
        let err = anyhow!("Request cancelled by client");
        self.resolve_impl(Err(err.context(LSPErrCode::RequestCancelled)))
      }
      eprintln!("Dangling request {self:?} dropped")
    }
  }
}
impl fmt::Debug for AsyncReq {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    write!(f, "AsyncReq({} {})", self.name, serde_json::to_string(&self.params).unwrap())
  }
}

trait_set! {
  pub trait ReqHandler =
    for<'a, 'b> FnMut(Option<&'a Value>, Session) -> anyhow::Result<Value> + 'static;
  pub trait AsyncReqHandler = FnMut(AsyncReq) + 'static;
  pub trait NotifHandler = for<'a, 'b> FnMut(Option<&'a Value>, Session) + 'static;
  pub trait SendCB = FnMut(Value) + Send + 'static;
  pub trait ResHandler = FnMut(Result<Value, ResponseError>) + Send + 'static;
}

#[derive(Debug)]
pub struct ResponseError {
  pub code: LSPErrCode,
  pub message: String,
  pub data: Option<Value>,
}

struct State {
  ingress: HashMap<i64, Abort>,
  egress: HashMap<i64, Box<dyn ResHandler>>,
  context: CtxMap,
  send: Box<dyn SendCB>,
}

impl State {
  fn new(send: impl SendCB) -> Self {
    Self {
      context: CtxMap::new(),
      egress: HashMap::new(),
      ingress: HashMap::new(),
      send: Box::new(send),
    }
  }

  fn send(&mut self, mut data: Value) {
    data["jsonrpc"] = json!("2.0");
    eprintln!("Sending {data}");
    (self.send)(data)
  }
  fn send_resp(&mut self, id: i64, result: anyhow::Result<Value>) {
    self.send(match result {
      Ok(val) => json!({
        "id": id,
        "result": val
      }),
      Err(e) => {
        let code = e.downcast_ref::<LSPErrCode>().copied().unwrap_or(LSPErrCode::RequestFailed);
        json!({
          "id": id,
          "error": {
            "code": code,
            "message": format!("{e}"),
            "data": e.downcast::<Value>().unwrap_or(Value::Null)
          }
        })
      },
    })
  }
  pub fn send_request(&mut self, method: &str, params: Value, callback: impl ResHandler) {
    let id = NEXT_REQ.fetch_add(1, atomic::Ordering::Relaxed);
    self.egress.insert(id, Box::new(callback));
    self.send(json!({ "id": id, "method": method, "params": params }))
  }
  pub fn send_notif(&mut self, method: &str, params: Value) {
    self.send(json!({ "method": method, "params": params }))
  }
  pub fn send_progress(&mut self, token: Value, value: Value) {
    self.send_notif("$/progress", json!({ "token": token, "value": value }))
  }
  fn handle_resp(&mut self, msg: Value) {
    let req_id = msg["id"].as_i64().unwrap();
    let res = msg.get("result").ok_or_else(|| {
      let err = msg.get("error").unwrap().as_object().unwrap();
      ResponseError {
        code: LSPErrCode::deserialize(err["code"].clone()).unwrap(),
        data: err.get("data").cloned(),
        message: err["message"].as_str().unwrap().to_string(),
      }
    });
    let mut cb =
      (self.egress.remove(&req_id)).expect("Responses must have had an associated request");
    cb(res.cloned())
  }
}

pub struct SessionGuard<'b>(MutexGuard<'b, State>);
impl<'b> SessionGuard<'b> {
  pub fn request(&mut self, method: &str, params: Value, callback: impl ResHandler) {
    self.0.send_request(method, params, callback)
  }
  pub fn notify(&mut self, method: &str, params: Value) { self.0.send_notif(method, params) }
  pub fn progress(&mut self, token: Value, value: Value) { self.0.send_progress(token, value) }
}
impl<'a> Deref for SessionGuard<'a> {
  type Target = CtxMap;
  fn deref(&self) -> &Self::Target { &self.0.context }
}
impl<'a> DerefMut for SessionGuard<'a> {
  fn deref_mut(&mut self) -> &mut Self::Target { &mut self.0.context }
}

#[derive(Clone)]
pub struct Session(Arc<Mutex<State>>);
impl Session {
  fn new(send: impl SendCB) -> Self { Self(Arc::new(Mutex::new(State::new(send)))) }

  pub fn request(&self, method: &str, params: Value, callback: impl ResHandler) {
    self.lock().request(method, params, callback)
  }
  pub fn notify(&self, method: &str, params: Value) { self.lock().notify(method, params) }
  #[allow(unused)] // we definitely need this but definitely not now
  pub fn progress(&self, token: Value, value: Value) { self.lock().progress(token, value) }
  pub fn set<U: Ctx>(&self, ctx: U) { self.0.lock().unwrap().context.set(ctx) }
  pub fn lock(&self) -> SessionGuard<'_> { SessionGuard(self.0.lock().unwrap()) }
}

pub struct JrpcServer {
  sync_hands: HashMap<String, Box<dyn ReqHandler>>,
  async_hands: HashMap<String, Box<dyn AsyncReqHandler>>,
  notif_hands: HashMap<String, Box<dyn NotifHandler>>,
  comm: Session,
}
impl JrpcServer {
  pub fn new(send: impl SendCB) -> Self {
    Self {
      notif_hands: HashMap::new(),
      sync_hands: HashMap::new(),
      async_hands: HashMap::new(),
      comm: Session::new(send),
    }
  }

  pub fn on_req_sync(&mut self, name: &str, handler: impl ReqHandler) {
    self.sync_hands.insert(name.to_string(), Box::new(handler));
  }

  pub fn on_notif(&mut self, name: &str, handler: impl NotifHandler) {
    self.notif_hands.insert(name.to_string(), Box::new(handler));
  }

  #[allow(dead_code)] // not being used yet
  pub fn on_req_async(&mut self, name: &str, handler: impl AsyncReqHandler) {
    self.async_hands.insert(name.to_string(), Box::new(handler));
  }

  pub fn recv(&mut self, message: Value) {
    // eprintln!("Received {message}");
    let mut comm_guard = self.comm.0.lock().unwrap();
    let obj = message.as_object().expect("All messages are objects");
    let id = obj.get("id").map(|id| id.as_i64().expect("If ID exists, it's an uint"));
    match obj.get("method").map(|m| m.as_str().unwrap()) {
      None => comm_guard.handle_resp(message),
      Some(name) => {
        let params = obj.get("params");
        match id {
          None => match self.notif_hands.get_mut(name) {
            None => eprintln!("Unrecognized notification {name}"),
            Some(handler) => {
              mem::drop(comm_guard);
              handler(params, self.comm.clone());
            },
          },
          Some(id) =>
            if name == "$/cancelRequest" {
              let cancel_id = params.unwrap()["id"].as_i64().unwrap();
              if let Some(abort) = comm_guard.ingress.get(&cancel_id) {
                abort.abort();
              }
            } else if let Some(handler) = self.sync_hands.get_mut(name) {
              mem::drop(comm_guard);
              let res = handler(params, self.comm.clone());
              self.comm.0.lock().unwrap().send_resp(id, res);
            } else if let Some(handler) = self.async_hands.get_mut(name) {
              let abort = Abort::new();
              comm_guard.ingress.insert(id, abort.clone());
              mem::drop(comm_guard);
              handler(AsyncReq {
                abort,
                id,
                name: name.to_owned(),
                params: params.cloned(),
                resolved: false,
                comm: self.comm.clone(),
              })
            } else if name.starts_with("$/") {
              eprintln!("Unrecognized optional request {name}");
              let err = anyhow::anyhow!("Unsupported request");
              comm_guard.send_resp(id, Err(err.context(LSPErrCode::MethodNotFound)))
            } else {
              panic!("Unrecognized request {name}")
            },
        }
      },
    }
  }
}

#[cfg(test)]
mod test {
  use std::sync::{Arc, Mutex};

  use serde_json::{json, Value};

  use super::JrpcServer;

  #[test]
  fn notif() {
    let replies = Arc::new(Mutex::new(Vec::new()));
    let rep2 = replies.clone();
    let mut srv = JrpcServer::new(move |_| panic!("Message should not be sent here"));
    srv.on_notif("hello", move |m, _| rep2.lock().unwrap().push(m.unwrap().clone()));
    srv.recv(json!({ "method": "hello", "params": "World!" }));
    assert!(serde_json::to_string(&replies.lock().unwrap()[..]).unwrap() == r#"["World!"]"#)
  }

  #[test]
  fn sync_req() {
    let replies = Arc::new(Mutex::new(Vec::new()));
    let rep2 = replies.clone();
    let mut srv = JrpcServer::new(move |m| rep2.lock().unwrap().push(m));
    srv.on_req_sync("hello", |p, _| {
      assert_eq!(p.unwrap().as_str(), Some("World!"));
      Ok(p.unwrap().clone())
    });
    srv.recv(json!({ "method": "hello", "id": 0, "params": "World!" }));
    let reps = replies.lock().unwrap();
    assert_eq!(reps.len(), 1);
    assert_eq!(reps[0]["id"].as_i64(), Some(0));
    assert_eq!(reps[0]["result"], Value::String("World!".to_string()))
  }
}
