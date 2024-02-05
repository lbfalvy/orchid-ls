use std::collections::HashMap;
use std::fmt;
use std::sync::atomic::AtomicI64;
use std::sync::{atomic, Arc, Mutex};

use anyhow::anyhow;
use serde_json::{json, Value};
use trait_set::trait_set;

use crate::protocol::error::LSErrCode;

static NEXT_REQ: AtomicI64 = AtomicI64::new(0);

#[derive(Clone)]
pub struct Abort(Arc<atomic::AtomicBool>);
impl Abort {
  pub fn new() -> Self { Self(Arc::new(atomic::AtomicBool::new(false))) }
  pub fn abort(&self) { self.0.store(true, atomic::Ordering::Relaxed) }
  pub fn aborted(&self) -> bool { self.0.load(atomic::Ordering::Relaxed) }
}

pub struct AsyncReq {
  name: String,
  id: i64,
  params: Option<Value>,
  abort: Abort,
  resolved: bool,
  comm: Arc<Mutex<CommState>>,
}
impl AsyncReq {
  pub fn name(&self) -> &str { self.name.as_str() }
  pub fn params(&self) -> Option<&Value> { self.params.as_ref() }
  pub fn aborted(&self) -> bool { self.abort.aborted() }
  pub fn resolve(mut self, result: anyhow::Result<Value>) { self.resolve_impl(result) }
  fn resolve_impl(&mut self, result: anyhow::Result<Value>) {
    self.resolved = true;
    self.comm.lock().unwrap().send_resp(self.id, result)
  }
  pub fn request(&self, method: &str, params: Value, callback: impl ResHandler + 'static) {
    self.comm.lock().unwrap().send_request(method, params, callback)
  }
  pub fn notify(&self, method: &str, params: Value) {
    self.comm.lock().unwrap().send_notif(method, params)
  }
  pub fn progress(&self, token: Value, value: Value) {
    self.comm.lock().unwrap().send_progress(token, value)
  }
}
impl Drop for AsyncReq {
  fn drop(&mut self) {
    if !self.resolved {
      if self.abort.aborted() {
        let err = anyhow!("Request cancelled by client");
        self.resolve_impl(Err(err.context(LSErrCode::RequestCancelled)))
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
  pub trait ReqHandler = for<'a> FnMut(Option<&'a Value>) -> anyhow::Result<Value>;
  pub trait AsyncReqHandler = FnMut(AsyncReq);
  pub trait NotifHandler = for<'a> FnMut(Option<&'a Value>);
  pub trait SendCB = FnMut(Value) + Send;
  pub trait ResHandler = FnMut(Result<Value, ResponseError>) + Send;
}

pub struct ResponseError {
  pub code: i64,
  pub message: String,
  pub data: Option<Value>,
}

struct CommState {
  ingress: HashMap<i64, Abort>,
  egress: HashMap<i64, Box<dyn ResHandler>>,
  send: Box<dyn SendCB>,
}
impl CommState {
  fn send(&mut self, data: Value) { (self.send)(data) }
  fn send_resp(&mut self, id: i64, result: anyhow::Result<Value>) {
    self.send(match result {
      Ok(val) => json!({
        "id": id,
        "result": val
      }),
      Err(e) => {
        let code = e.downcast_ref::<LSErrCode>().copied().unwrap_or(LSErrCode::RequestFailed);
        json!({
          "id": id,
          "error": {
            "code": i64::from(code),
            "message": format!("{e}"),
            "data": e.downcast::<Value>().unwrap_or(Value::Null)
          }
        })
      },
    })
  }
  pub fn send_request(&mut self, method: &str, params: Value, callback: impl ResHandler + 'static) {
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
        code: err["code"].as_i64().unwrap(),
        data: err.get("data").cloned(),
        message: err["message"].as_str().unwrap().to_string(),
      }
    });
    let mut cb =
      (self.egress.remove(&req_id)).expect("Responses must have had an associated request");
    cb(res.cloned())
  }
}

pub struct JrpcServer {
  sync_hands: HashMap<String, Box<dyn ReqHandler>>,
  async_hands: HashMap<String, Box<dyn AsyncReqHandler>>,
  notif_hands: HashMap<String, Box<dyn NotifHandler>>,
  comm: Arc<Mutex<CommState>>,
}
impl JrpcServer {
  pub fn new(send: impl FnMut(Value) + Clone + Send + Sync + 'static) -> Self {
    Self {
      notif_hands: HashMap::new(),
      sync_hands: HashMap::new(),
      async_hands: HashMap::new(),
      comm: Arc::new(Mutex::new(CommState {
        ingress: HashMap::new(),
        egress: HashMap::new(),
        send: Box::new(send),
      })),
    }
  }

  pub fn on_req_sync(&mut self, name: &str, handler: impl ReqHandler + 'static) {
    self.sync_hands.insert(name.to_string(), Box::new(handler));
  }

  pub fn on_notif(&mut self, name: &str, handler: impl NotifHandler + 'static) {
    self.notif_hands.insert(name.to_string(), Box::new(handler));
  }

  pub fn on_req_async(&mut self, name: &str, handler: impl AsyncReqHandler + 'static) {
    self.async_hands.insert(name.to_string(), Box::new(handler));
  }

  pub fn recv(&mut self, message: Value) {
    let mut comm_guard = self.comm.lock().unwrap();
    let obj = message.as_object().expect("All messages are objects");
    let id = obj.get("id").map(|id| id.as_i64().expect("If ID exists, it's an uint"));
    match obj["method"].as_str() {
      None => comm_guard.handle_resp(message),
      Some(name) => {
        let params = obj.get("params");
        match id {
          None => match self.notif_hands.get_mut(name) {
            None => eprintln!("Unrecognized notification {name}"),
            Some(handler) => handler(params),
          },
          Some(id) =>
            if name == "$/cancelRequest" {
              let cancel_id = params.unwrap()["id"].as_i64().unwrap();
              if let Some(abort) = comm_guard.ingress.get(&cancel_id) {
                abort.abort();
              }
            } else if let Some(handler) = self.sync_hands.get_mut(name) {
              comm_guard.send_resp(id, handler(params));
            } else if let Some(handler) = self.async_hands.get_mut(name) {
              let abort = Abort::new();
              comm_guard.ingress.insert(id, abort.clone());
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
              comm_guard.send_resp(id, Err(err.context(LSErrCode::MethodNotFound)))
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
    srv.on_notif("hello", move |m| rep2.lock().unwrap().push(m.unwrap().clone()));
    srv.recv(json!({ "method": "hello", "params": "World!" }));
    assert!(serde_json::to_string(&replies.lock().unwrap()[..]).unwrap() == r#"["World!"]"#)
  }

  #[test]
  fn sync_req() {
    let replies = Arc::new(Mutex::new(Vec::new()));
    let rep2 = replies.clone();
    let mut srv = JrpcServer::new(move |m| rep2.lock().unwrap().push(m));
    srv.on_req_sync("hello", |p| {
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
