use std::sync::atomic::{self, AtomicUsize};
use std::sync::Arc;
use std::{mem, thread};

use hashbrown::{HashMap, HashSet};
use intern_all::{i, Tok};
use itertools::Itertools;
use orchidlang::name::{PathSlice, VPath};
use orchidlang::virt_fs::{DirNode, FSResult, Loaded, PrefixFS, VirtFS};
use serde::Deserialize;
use serde_json::json;

use crate::jrpc::{Abort, JrpcServer, Session};
use crate::orc::project::{find_all_projects, LoadedProject};
use crate::protocol::document::{FileUri, WspaceEnt};
use crate::protocol::tokens::transcode_tokens;

pub fn ttypes() -> Vec<Tok<String>> {
  vec![
    i!(str: "namespace"),
    i!(str: "variable"),
    i!(str: "parameter"),
    i!(str: "function"),
    i!(str: "macro"),
    i!(str: "comment"),
    i!(str: "operator"),
    i!(str: "string"),
    i!(str: "number"),
    i!(str: "keyword"),
  ]
}

#[derive(Clone, Deserialize)]
pub struct PatchFile {
  uri: FileUri,
  text: String,
  version: u64,
}

#[derive(Clone, Deserialize)]
pub struct PatchStore {
  basepath: FileUri,
  patches: Vec<PatchFile>,
}
impl PatchStore {
  pub fn new(basepath: FileUri) -> Arc<Self> { Arc::new(Self { basepath, patches: Vec::new() }) }
  pub fn unpack(self: Arc<Self>) -> Self { Arc::unwrap_or_clone(self) }
  pub fn change(self: &mut Arc<Self>, cb: impl FnOnce(&mut Self)) {
    take_mut::take(self, |arc| {
      let mut this = arc.unpack();
      cb(&mut this);
      Arc::new(this)
    })
  }
  fn index_of(&self, uri: &FileUri) -> Option<usize> {
    self.patches.iter().find_position(|f| &f.uri == uri).map(|p| p.0)
  }
  pub fn basepath(&self) -> &FileUri { &self.basepath }
  pub fn patch(&mut self, patch: PatchFile) {
    match self.index_of(&patch.uri) {
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
  pub fn unpatch(&mut self, uri: &FileUri) {
    match self.index_of(uri) {
      None => panic!("No existing patch!"),
      Some(i) => {
        self.patches.remove(i);
      },
    }
  }
  pub fn mk_vfs(self: Arc<Self>, path: &FileUri) -> Option<impl VirtFS> {
    let subpath = path.to_vpath(&self.basepath)?;
    eprintln!("Building VFS for {subpath} in {}", self.basepath);
    Some(PrefixFS::new(PatchFS::new(self), "", subpath.to_string()))
  }
}

pub struct PatchFS {
  basedir: DirNode,
  store: Arc<PatchStore>,
}
impl PatchFS {
  pub fn new(store: Arc<PatchStore>) -> Self {
    Self { basedir: DirNode::new(store.basepath().to_path(), ".orc"), store }
  }
}
impl VirtFS for PatchFS {
  fn get(&self, path: &[Tok<String>], full_path: &PathSlice) -> FSResult {
    let pbuf = self.store.basepath();
    if let Some(i) = self.store.index_of(&pbuf.extended(path.iter().map(|t| t.as_str()))) {
      return Ok(Loaded::Code(Arc::new(self.store.patches[i].text.clone())));
    }
    self.basedir.get(path, full_path)
  }
  fn display(&self, path: &[Tok<String>]) -> Option<String> { self.basedir.display(path) }
}

pub struct CtxProj {
  pub path: VPath,
  pub changes: HashSet<VPath>,
  pub abort: Abort,
}
impl CtxProj {
  pub fn new(path: VPath) -> Self { Self { path, changes: HashSet::new(), abort: Abort::new() } }
  pub fn path_in<'a>(&self, path: &'a PathSlice) -> Option<&'a PathSlice> {
    path.strip_prefix(&self.path)
  }
}

pub struct CtxWsp {
  pub name: String,
  pub store: Arc<PatchStore>,
  pub projects: Vec<CtxProj>,
}
impl CtxWsp {
  pub fn path_in(&self, path: &FileUri) -> Option<VPath> { path.to_vpath(&self.store.basepath) }

  pub fn get_proj<'a, 'b>(&'a self, p: &'b PathSlice) -> Option<(&'b PathSlice, &'a CtxProj)> {
    self.projects.iter().find_map(|proj| Some((proj.path_in(p)?, proj)))
  }

  pub fn get_proj_mut<'a, 'b>(
    &'a mut self,
    p: &'b PathSlice,
  ) -> Option<(&'b PathSlice, &'a mut CtxProj)> {
    self.projects.iter_mut().find_map(|proj| Some((proj.path_in(p)?, proj)))
  }
}

pub struct WorkspaceCtx(Vec<CtxWsp>);
impl WorkspaceCtx {
  pub fn new(wspace_entries: impl IntoIterator<Item = WspaceEnt>) -> Self {
    Self(
      wspace_entries
        .into_iter()
        .map(|ent| {
          // let path = uri2path(&ent.uri)?;
          let store = PatchStore::new(ent.uri.clone());
          let wspace_vfs = store.clone().mk_vfs(&store.basepath).unwrap();
          let projects =
            find_all_projects(VPath::new([]), &wspace_vfs).into_iter().map(CtxProj::new).collect();
          CtxWsp { name: ent.name, store, projects }
        })
        .collect(),
    )
  }
  pub fn get_wsp<'a>(&'a self, path: &FileUri) -> Option<(VPath, &'a CtxWsp)> {
    (self.0.iter())
      .filter_map(|e| e.path_in(path).map(|p| (p, e)))
      .max_by_key(|(p, _)| -(p.len() as i32))
  }
  pub fn get_wsp_mut<'a>(&'a mut self, path: &FileUri) -> Option<(VPath, &'a mut CtxWsp)> {
    (self.0.iter_mut())
      .filter_map(|e| e.path_in(path).map(|p| (p, e)))
      .max_by_key(|(p, _)| -(p.len() as i32))
  }
  #[allow(unused)]
  pub fn get_proj<'a>(&'a self, path: &FileUri) -> Option<(VPath, &'a CtxWsp, &'a CtxProj)> {
    let (subpath, wsp) = self.get_wsp(path)?;
    let (path, proj) = wsp.get_proj(&subpath)?;
    Some((path.to_vpath(), wsp, proj))
  }
  pub fn get_proj_mut<'a>(
    &'a mut self,
    path: &FileUri,
  ) -> Option<(VPath, Arc<PatchStore>, &'a mut CtxProj)> {
    let (subpath, wsp) = self.get_wsp_mut(path)?;
    let store = wsp.store.clone();
    let (path, proj) = wsp.get_proj_mut(&subpath)?;
    Some((path.to_vpath(), store, proj))
  }
}

static THREADCNT: AtomicUsize = AtomicUsize::new(0);

fn process_update(patch: PatchFile, session: Session) {
  // This task thread contains 2 critical sections. The first sets the abort flag
  // for the previous instance and replaces it with its own abort flag, the
  // second checks the state of the abort flag after locking. This ensures that
  thread::Builder::new()
    .name("patch-processor".into())
    .stack_size(1 << 26)
    .spawn(move || {
      let id = THREADCNT.fetch_add(1, atomic::Ordering::Relaxed);
      eprintln!("~{id} Spawned");
      // Using session while this is live would deadlock
      let mut g = session.lock();
      let fsctx = g.get_mut::<WorkspaceCtx>().unwrap();
      let uri = patch.uri.clone();
      let (in_wsp, entry) = fsctx.get_wsp_mut(&uri).unwrap();
      entry.store.change(|s| s.patch(patch));
      let patches = entry.store.clone();
      let (in_proj, proj) = match entry.get_proj_mut(&in_wsp) {
        Some(p) => p,
        None => {
          eprintln!("Could not find {in_wsp} in {} while resolving {uri}", patches.basepath);
          panic!("Entry only contains {}", entry.projects.iter().map(|p| &p.path).join(", "))
        },
      };
      proj.abort.abort();
      let abort = Abort::new();
      proj.abort = abort.clone();
      proj.changes.insert(in_proj.to_vpath());
      let changes = proj.changes.clone();
      let proj_root = proj.path.clone();
      mem::drop(g);
      let lpr = LoadedProject::new(patches.clone(), proj_root, abort.clone())
        .unwrap_or_else(|ev| panic!("{}", ev.into_iter().join("\n\n")));
      eprintln!("~{id} loaded project");
      let ttypes = ttypes();
      let fsroot = patches.basepath().extended(lpr.root.as_slice());
      let vfs = patches.mk_vfs(&fsroot).unwrap();
      let mut file_tokens = HashMap::new();
      for path in changes.into_iter() {
        if abort.aborted() {
          return;
        }
        let mut tokens = lpr.module_tokens(&path.clone().prefix([i!(str: "tree")]));
        tokens.sort_by_key(|st| st.range.start);
        let src = match vfs.get(&path, &path).unwrap() {
          Loaded::Code(c) => c.clone(),
          _ => panic!("this must be a file"),
        };
        let tokens = match tokens.is_empty() {
          true => vec![],
          false => transcode_tokens(tokens, &src)
            .map(|(pos, len, sem)| {
              (pos.line, pos.char, len, ttypes.iter().position(|x| x == &sem.typ))
            })
            .collect_vec(),
        };
        file_tokens.insert(path, tokens);
      }
      let mut g = session.lock();
      // this asserts that between the two regions synchronized over ctx a new process
      // has not been spawned
      if !abort.is_valid() {
        return;
      }
      let fsctx = g.get_mut::<WorkspaceCtx>().unwrap();
      let (store, proj) = match fsctx.get_proj_mut(&uri) {
        // We find the project via the trigger URI, but the corresponding path is useless
        Some((_, store, proj)) => (store, proj),
        None => {
          eprintln!("Syntax not delivered because the project has been deleted");
          return;
        },
      };
      proj.changes = HashSet::new();
      let proj_root = proj.path.clone();
      for (path, tokens) in file_tokens {
        let uri = store.basepath().extended(proj_root.as_slice().iter().chain(path.as_slice()));
        g.notify(
          "client/syntacticTokens",
          json!({
            "textDocument": { "uri": uri.stringify(true) },
            "tokens": tokens,
            "legend": &ttypes,
          }),
        )
      }
    })
    .unwrap();
}

pub fn attach(srv: &mut JrpcServer) {
  srv.on_notif("textDocument/didOpen", |req, session| {
    let text_doc = &req.unwrap()["textDocument"];
    let lid = text_doc["languageId"].as_str().unwrap();
    if lid != "orchid" {
      eprintln!("Document has wrong lid \"{lid}\"");
      return;
    }
    let patch = PatchFile::deserialize(text_doc).unwrap();
    process_update(patch, session)
  });
  srv.on_notif("textDocument/didClose", |req, session| {
    let uri = FileUri::deserialize(&req.unwrap()["textDocument"]["uri"]).unwrap();
    let mut ctx = session.lock();
    let fsctx = ctx.get_mut::<WorkspaceCtx>().unwrap();
    let (_, entry) = fsctx.get_wsp_mut(&uri).unwrap();
    // release file so that external updates are received
    entry.store.change(|s| s.unpatch(&uri));
  });
  srv.on_notif("textDocument/didChange", |req, session| {
    let req = req.unwrap();
    let text_doc = &req["textDocument"];
    let last_change = req["contentChanges"].as_array().unwrap().last().unwrap();
    assert!(last_change.get("range").is_none(), "We requested absolute changes only");
    let patch = PatchFile {
      uri: FileUri::deserialize(&text_doc["uri"]).unwrap(),
      version: text_doc["version"].as_u64().unwrap(),
      text: String::deserialize(&last_change["text"]).unwrap(),
    };
    process_update(patch, session)
  })
}
