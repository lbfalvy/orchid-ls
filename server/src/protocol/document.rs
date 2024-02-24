use std::borrow::Cow;
use std::path::PathBuf;
use std::str::FromStr;
use std::sync::Arc;
use std::{fmt, hash};

use intern_all::i;
use orchidlang::name::VPath;
use serde::{Deserialize, Serialize};
use trait_set::trait_set;

use super::docpos::DocPos;

/// Entries in `workspaceEntries` on init
#[derive(Deserialize, Clone, Debug, Hash, PartialEq, Eq)]
pub struct WspaceEnt {
  pub name: String,
  pub uri: FileUri,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, Hash, PartialEq, Eq)]
pub struct DocRange {
  pub start: DocPos,
  pub end: DocPos,
}

#[derive(Deserialize)]
pub struct TextDocumentItem {
  pub uri: FileUri,
  #[serde(alias = "languageId")]
  pub language_id: String,
  pub version: i64,
  pub text: String,
}

trait_set! {
  pub trait UriSegments<'a> = Iterator<Item = Cow<'a, str>> + Clone + 'a
}

/// A file URI
///
/// # Assumptions
///
/// These derive from the overarching concept of a file URI and apply to each
/// constituent concept:
///
/// - all path segments are valid Unicode
/// - all paths are absolute
/// - the prefix is always file:///. No host, no alternative schema
///
/// The path segments'
#[derive(Clone, Debug, Eq)]
pub struct FileUri(Arc<String>);
impl FileUri {
  pub fn to_path(&self) -> PathBuf {
    url::Url::from_str(&format!("file:///{}", self.0)).unwrap().to_file_path().unwrap()
  }
  pub fn segments(&self) -> impl UriSegments {
    self.0.split('/').map(|s| urlencoding::decode(s).unwrap())
  }
  pub fn to_vpath(&self, prefix: &FileUri) -> Option<VPath> {
    let rest = self.0.strip_prefix(&*prefix.0)?;
    if rest.is_empty() {
      return Some(VPath::new([]));
    }
    let rest = rest.strip_prefix('/')?;
    Some(VPath::new(rest.split('/').map(i)))
  }
  #[must_use = "This is a pure function"]
  pub fn extended<S: AsRef<str>>(&self, segments: impl IntoIterator<Item = S>) -> Self {
    Self(Arc::new(segments.into_iter().fold(self.0.to_string(), |s, seg| s + "/" + seg.as_ref())))
  }
  pub fn stringify(&self, is_file: bool) -> String {
    format!("file:///{}{}", self.0, is_file.then_some(".orc").unwrap_or_default())
  }
}
impl fmt::Display for FileUri {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result { write!(f, "file:///{}", self.0) }
}
impl<'de> Deserialize<'de> for FileUri {
  fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
  where D: serde::Deserializer<'de> {
    let s = String::deserialize(deserializer)?;
    let path = (s.strip_prefix("file:///"))
      .ok_or_else(|| serde::de::Error::custom("FileUri has non-file scheme"))?;
    let path = path.strip_suffix('/').or(path.strip_suffix(".orc")).unwrap_or(path);
    Ok(Self(Arc::new(path.to_string())))
  }
}
impl PartialEq for FileUri {
  fn eq(&self, other: &Self) -> bool { self.segments().eq(other.segments()) }
}
impl hash::Hash for FileUri {
  fn hash<H: hash::Hasher>(&self, state: &mut H) { self.segments().for_each(|seg| seg.hash(state)) }
}
