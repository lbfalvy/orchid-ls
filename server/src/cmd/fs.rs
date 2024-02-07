use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use intern_all::{i, Tok};
use itertools::Itertools;
use orchidlang::name::PathSlice;
use orchidlang::virt_fs::{DirNode, FSResult, Loaded, PrefixFS, VirtFS};
use serde::ser::SerializeStruct;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde_json::Map;

use crate::jrpc::JrpcServer;
use crate::protocol::document::WspaceEnt;

pub fn uri2path(uri: &str) -> Option<PathBuf> {
  uri.strip_prefix("file://").map(|path| PathBuf::from(&*urlencoding::decode(path).unwrap()))
}

fn de_uri<'de, D: Deserializer<'de>>(d: D) -> Result<PathBuf, D::Error> {
  Ok(uri2path(&String::deserialize(d)?).unwrap())
}

fn ser_uri<S: Serializer>(path: &Path, s: S) -> Result<S::Ok, S::Error> {
  let enc_path =
    path.components().map(|c| urlencoding::encode(c.as_os_str().to_str().unwrap())).join("/");
  s.serialize_str(&format!("file://{enc_path}"))
}

#[derive(Serialize, Deserialize)]
pub struct PatchFile {
  #[serde(alias="uri", deserialize_with = "de_uri", serialize_with = "ser_uri")]
  path: PathBuf,
  text: String,
  version: u64,
}

pub struct PatchStore {
  basepath: PathBuf,
  patches: Vec<PatchFile>,
}
impl PatchStore {
  pub fn new(basepath: PathBuf) -> Self { Self { basepath, patches: Vec::new() } }
  fn index_of(&self, path: &Path) -> Option<usize> {
    self.patches.iter().find_position(|f| f.path == path).map(|p| p.0)
  }
  pub fn patch(&mut self, patch: PatchFile) {
    match self.index_of(&patch.path) {
      None => self.patches.push(patch),
      Some(idx) => {
        let old = &mut self.patches[idx];
        if old.version <= patch.version {
          old.version = patch.version;
          old.text = patch.text;
        }
      },
    }
  }
  pub fn unpatch(&mut self, path: &Path) {
    match self.index_of(path) {
      None => panic!("No existing patch!"),
      Some(i) => {
        self.patches.remove(i);
      },
    }
  }
}

pub struct PatchFS<'a> {
  basedir: DirNode,
  store: &'a PatchStore,
}
impl<'a> PatchFS<'a> {
  pub fn new(store: &'a PatchStore) -> Self {
    Self { basedir: DirNode::new(store.basepath.clone(), ".orc"), store }
  }
}
impl<'a> VirtFS for PatchFS<'a> {
  fn get(&self, path: &[Tok<String>], full_path: PathSlice) -> FSResult {
    let mut pbuf = self.store.basepath.clone();
    path.iter().for_each(|seg| pbuf.push(seg.as_str()));
    if let Some(i) = self.store.index_of(&pbuf) {
      return Ok(Loaded::Code(Arc::new(self.store.patches[i].text.clone())));
    }
    self.basedir.get(path, full_path)
  }
  fn display(&self, path: &[Tok<String>]) -> Option<String> { self.basedir.display(path) }
}

struct WspCtxEnt {
  name: String,
  path: PathBuf,
  store: PatchStore,
}

pub struct WorkspaceCtx(Vec<WspCtxEnt>);
impl WorkspaceCtx {
  pub fn new(wspace_entries: impl IntoIterator<Item = WspaceEnt>) -> Self {
    Self(
      wspace_entries
        .into_iter()
        .filter_map(|ent| {
          let path = uri2path(&ent.uri)?;
          Some(WspCtxEnt { name: ent.name, store: PatchStore::new(path.clone()), path })
        })
        .collect(),
    )
  }
  pub fn find_path<'a, 'b>(&'a self, path: &'b Path) -> Option<(&'b Path, &'a WspCtxEnt)> {
    (self.0.iter())
      .filter_map(|e| path.strip_prefix(&e.path).ok().map(|p| (p, e)))
      .max_by_key(|(p, _)| p.as_os_str().len())
  }
  pub fn find_path_mut<'a, 'b>(
    &'a mut self,
    path: &'b Path,
  ) -> Option<(&'b Path, &'a mut WspCtxEnt)> {
    (self.0.iter_mut())
      .filter_map(|e| path.strip_prefix(&e.path).ok().map(|p| (p, e)))
      .max_by_key(|(p, _)| p.as_os_str().len())
  }
  pub fn mk_vfs<'a>(&'a self, path: &Path) -> Option<impl VirtFS + 'a> {
    let (subpath, entry) = self.find_path(path)?;
    let prefix = subpath.components().map(|s| s.as_os_str().to_str().unwrap()).join("::");
    Some(PrefixFS::new(PatchFS::new(&entry.store), "", prefix))
  }
}

pub fn attach(srv: &mut JrpcServer) {
  srv.on_notif("textDocument/didOpen", |req, ctx| {
    let text_doc = &req.unwrap()["textDocument"];
    if text_doc["languageId"].as_str().unwrap() != "orchid" { return; }
    let patch = PatchFile::deserialize(text_doc).unwrap();
    let fsctx = ctx.get_mut::<WorkspaceCtx>().unwrap();
    let (_, entry) = fsctx.find_path_mut(&patch.path).unwrap();
    entry.store.patch(patch);
  });
  srv.on_notif("textDocument/didClose", |req, ctx| {
    let fsctx = ctx.get_mut::<WorkspaceCtx>().unwrap();
    let path = uri2path(req.unwrap()["textDocument"]["uri"].as_str().unwrap()).unwrap();
    let (_, entry) = fsctx.find_path_mut(&path).unwrap();
    entry.store.unpatch(&path);
  });
  srv.on_notif("textDocument/didChange", |req, ctx| {
    let req = req.unwrap();
    let text_doc = &req["textDocument"];
    let last_change = req["contentChanges"].as_array().unwrap().last().unwrap();
    assert!(last_change.get("range").is_none(), "We requested absolute changes only");
    let fsctx = ctx.get_mut::<WorkspaceCtx>().unwrap();
    let path = uri2path(text_doc["uri"].as_str().unwrap()).unwrap();
    let (_, entry) = fsctx.find_path_mut(&path).unwrap();
    entry.store.patch(PatchFile {
      path,
      version: text_doc["version"].as_u64().unwrap(),
      text: String::deserialize(&last_change["text"]).unwrap(),
    });
  })
}
