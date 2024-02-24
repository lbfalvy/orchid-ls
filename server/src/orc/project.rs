use std::collections::VecDeque;
use std::io::BufReader;
use std::rc::Rc;
use std::sync::Arc;

use hashbrown::HashMap;
use intern_all::i;
use itertools::Itertools;
use orchidlang::error::{ProjectErrorObj, Reporter};
use orchidlang::facade::loader::Loader;
use orchidlang::facade::macro_runner::MacroRunner;
use orchidlang::foreign::inert::Inert;
use orchidlang::libs::asynch::system::AsynchSystem;
use orchidlang::libs::directfs::DirectFS;
use orchidlang::libs::io::{IOService, Stream};
use orchidlang::libs::scheduler::system::SeqScheduler;
use orchidlang::libs::std::std_system::StdConfig;
use orchidlang::location::SourceRange;
use orchidlang::name::{NameLike, PathSlice, Sym, VPath};
use orchidlang::parse::lexer::namestart;
use orchidlang::parse::parsed;
use orchidlang::pipeline::project::{ItemKind, ProjItem, ProjectTree};
use orchidlang::tree::{ModMember, ModMemberRef, TreeTransforms};
use orchidlang::utils::pure_seq::pushed;
use orchidlang::virt_fs::{DeclTree, Loaded, VirtFS};
use ordered_float::NotNan;
use substack::Substack;

use crate::cmd::fs::PatchStore;
use crate::jrpc::Abort;
use crate::protocol::tokens::SemToken;

/// Find all Orchid projects in a vfs. An Orchid project is either
/// - a folder containing `project_info.orc`
/// - a file not belonging to any such folder
pub fn find_all_projects(path: VPath, vfs: &impl VirtFS) -> Vec<VPath> {
  let mut queue = VecDeque::from([path.clone()]);
  let mut results = Vec::new();
  while let Some(p) = queue.pop_front() {
    match vfs.read(&p) {
      Err(_) => (),
      Ok(Loaded::Code(_)) => results.push(p),
      // Ok(Loaded::Code(_)) => continue,
      Ok(Loaded::Collection(c)) if c.iter().any(|f| &**f == "project_info") => results.push(p),
      Ok(Loaded::Collection(c)) =>
        c.iter().for_each(|item| queue.push_back(p.clone().suffix([item.clone()]))),
    }
  }
  eprintln!("Projects in {path}:\n{}", results.iter().join(", "));
  results
}

pub struct LoadedProject {
  pub patches: Arc<PatchStore>,
  pub root: VPath,
  pub tree: ProjectTree,
  pub macros: MacroRunner,
}
impl LoadedProject {
  pub fn new(
    patches: Arc<PatchStore>,
    root: VPath,
    abort: Abort,
  ) -> Result<Self, Vec<ProjectErrorObj>> {
    if abort.aborted() {
      return Err(vec![]);
    }
    let mut asynch = AsynchSystem::new();
    let scheduler = SeqScheduler::new(&mut asynch);
    let std_streams = [
      ("stdout", Stream::Sink(Box::<Vec<u8>>::default())),
      ("stdout", Stream::Sink(Box::<Vec<u8>>::default())),
      ("stdin", Stream::Source(BufReader::new(Box::new(&[][..])))),
    ];
    let reporter = Reporter::new();
    let env = Loader::new()
      .add_system(StdConfig { impure: true })
      .add_system(asynch)
      .add_system(IOService::new(scheduler.clone(), std_streams))
      .add_system(DirectFS::new(scheduler.clone()))
      .add_system(scheduler);
    let vfs_root = patches.basepath().extended(root.clone());
    eprintln!("{} + {} = {}", patches.basepath(), root, vfs_root);
    let vfs = patches.clone().mk_vfs(&vfs_root).expect("Root not in fs");
    let srctree = DeclTree::ns("tree", [DeclTree::leaf(Rc::new(vfs))]);
    if abort.aborted() {
      return Err(vec![]);
    }
    let tree = env.load_project(srctree, &reporter);
    if reporter.failing() || abort.aborted() {
      return Err(reporter.into_errors().unwrap_or_default());
    }
    let macros = MacroRunner::new(&tree, Some(10_000), &reporter);
    if reporter.failing() || abort.aborted() {
      return Err(reporter.into_errors().unwrap_or_default());
    }
    Ok(Self { patches, root, tree, macros })
  }

  pub fn tokens(&self) -> Vec<SemToken> {
    let mut tokv = vec![];
    self.tree.0.search_all((), |_, mem, ()| {
      if let ModMemberRef::Item(ProjItem { kind: ItemKind::Const(val) }) = mem {
        tokv.extend(tokens(val, &val.range.path(), &self.macros).into_iter().flatten())
      }
    });
    tokv
  }

  pub fn module_tokens(&self, prefix: &PathSlice) -> Vec<SemToken> {
    if prefix.is_empty() {
      return self.tokens();
    }
    let (ent, _) = self.tree.0.walk1_ref(&[], prefix, |_| true).expect("Path must be valid");
    let consts = match &ent.member {
      ModMember::Item(ProjItem { kind: ItemKind::Const(val) }) => vec![val],
      ModMember::Sub(module) => module.search_all(vec![], |_, mem, consts| match mem {
        ModMemberRef::Item(ProjItem { kind: ItemKind::Const(val) }) => pushed(consts, val),
        _ => consts,
      }),
      _ => return vec![],
    };
    (consts.into_iter())
      .flat_map(|c| tokens(c, &c.range.path(), &self.macros).into_iter().flatten())
      .collect()
  }
}

pub fn tokens(
  expr: &parsed::Expr,
  path: &Sym,
  macros: &MacroRunner,
) -> Option<impl Iterator<Item = SemToken>> {
  let postmacro = macros.process_expr(expr.clone()).ok()?;
  let n_toks = name_toks(&postmacro, Substack::Bottom, path);
  let mut tokens = Vec::new();
  expr.search_all(&mut |ex| {
    if &ex.range.path() != path {
      return None;
    }
    match &ex.value {
      parsed::Clause::Name(n) if !n_toks.contains_key(&ex.range) => {
        let is_name = n.last().starts_with(namestart);
        let ty = if is_name { i!(str: "keyword") } else { i!(str: "operator") };
        tokens.push(SemToken::new(ex.range.clone(), ty));
      },
      parsed::Clause::Atom(at) => {
        let atom = at.run();
        tokens.push(SemToken::new(
          ex.range.clone(),
          if atom.is::<Inert<usize>>() || atom.is::<Inert<NotNan<f64>>>() {
            i!(str: "number")
          } else if atom.is::<Inert<bool>>() {
            i!(str: "keyword")
          } else {
            i!(str: "string")
          },
        ));
      },
      _ => (),
    }
    None::<()>
  });
  Some(n_toks.into_values().chain(tokens))
}

/// Create tokens for all names that have the same origin path (were not created
/// by macros) based on whether they appear bound or unbound in the postmacro
/// tree
pub fn name_toks(
  ast: &parsed::Expr,
  bindings: Substack<Sym>,
  path: &Sym,
) -> HashMap<SourceRange, SemToken> {
  match &ast.value {
    parsed::Clause::Lambda(arg, body) => {
      let mut map = HashMap::new();
      let bindings = match &arg[..] {
        [parsed::Expr { value: parsed::Clause::Name(n), range }] => {
          if &range.path() == path {
            map.insert(range.clone(), SemToken::new(range.clone(), i!(str: "parameter")));
          }
          bindings.push(n.clone())
        },
        _ => bindings,
      };
      for ex in body.iter() {
        map.extend(name_toks(ex, bindings.clone(), path));
      }
      map
    },
    parsed::Clause::Name(n) if &ast.range.path() == path => {
      let is_bound = bindings.iter().any(|b| b == n);
      let ty = if is_bound { i!(str: "variable") } else { i!(str: "function") };
      HashMap::from([(ast.range.clone(), SemToken::new(ast.range.clone(), ty))])
    },
    parsed::Clause::S(_, b) => {
      let mut hash = HashMap::new();
      b.iter().for_each(|x| hash.extend(name_toks(x, bindings.clone(), path)));
      hash
    },
    _ => HashMap::new(),
  }
}
