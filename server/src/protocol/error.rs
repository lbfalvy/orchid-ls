//! Types and tables for LSP errors

use std::fmt;

use serde::{Deserialize, Serialize};

/// Error codes recognized by LSP. Paraphrased from
/// [The LSP spec](https://microsoft.github.io/language-server-protocol/specifications/lsp/3.17/specification/)
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum LSPErrCode {
  /// Defined by JSON-RPC
  ParseError,
  /// Defined by JSON-RPC
  InvalidRequest,
  /// Defined by JSON-RPC
  MethodNotFound,
  /// Defined by JSON-RPC
  InvalidParams,
  /// Defined by JSON-RPC
  InternalError,

  /// Error code indicating that a server received a notification or request
  /// before the server has received the `initialize` request.
  ServerNotInitialized,

  /// LSP defines this but never actually explains when it should be used. Do
  /// not use until meaning is clarified and this comment updated
  #[deprecated]
  Unknown,

  /// A request failed but it was syntactically correct, e.g the method name was
  /// known and the parameters were valid. The error message should contain
  /// human readable information about why the request failed.
  RequestFailed,

  /// The server cancelled the request. This error code should only be used for
  /// requests that explicitly support being server cancellable.
  ServerCancelled,

  /// The server detected that the content of a document got modified outside
  /// normal conditions. A server should NOT send this error code if it
  /// detects a content change in it unprocessed messages. The result even
  /// computed on an older state might still be useful for the client.
  ///
  /// If a client decides that a result is not of any use anymore the client
  /// should cancel the request.
  ContentModified,

  /// The client has canceled a request and a server as detected the cancel.
  RequestCancelled,

  UnclassifiedError(i64),
}

#[allow(deprecated)] // clearly, we have to match deprecated values.
static CODE_MAP: &[(LSPErrCode, i64)] = &[
  (LSPErrCode::ParseError, -32700),
  (LSPErrCode::InvalidRequest, -32600),
  (LSPErrCode::MethodNotFound, -32601),
  (LSPErrCode::InvalidParams, -32602),
  (LSPErrCode::InternalError, -32603),
  (LSPErrCode::ServerNotInitialized, -32002),
  (LSPErrCode::Unknown, -32001),
  (LSPErrCode::RequestFailed, -32803),
  (LSPErrCode::ServerCancelled, -32802),
  (LSPErrCode::ContentModified, -32801),
  (LSPErrCode::RequestCancelled, -32800),
];

impl fmt::Display for LSPErrCode {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result { write!(f, "{self:?}") }
}
impl From<LSPErrCode> for i64 {
  fn from(value: LSPErrCode) -> Self {
    match value {
      LSPErrCode::UnclassifiedError(code) => code,
      _ => CODE_MAP.iter().find(|(ec, _)| ec == &value).unwrap().1,
    }
  }
}
impl From<i64> for LSPErrCode {
  fn from(value: i64) -> Self {
    CODE_MAP.iter().find(|(_, i)| i == &value).map_or(LSPErrCode::UnclassifiedError(value), |p| p.0)
  }
}
impl Serialize for LSPErrCode {
  fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
  where S: serde::Serializer {
    serializer.serialize_i64((*self).into())
  }
}
impl<'de> Deserialize<'de> for LSPErrCode {
  fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
  where D: serde::Deserializer<'de> {
    Ok(i64::deserialize(deserializer)?.into())
  }
}
