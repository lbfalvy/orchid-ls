//! Types and tables for LSP errors

use std::fmt;

/// Error codes recognized by LSP. Paraphrased from
/// [The LSP spec](https://microsoft.github.io/language-server-protocol/specifications/lsp/3.17/specification/)
#[derive(Clone, Copy, Debug)]
pub enum LSErrCode {
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
}
impl fmt::Display for LSErrCode {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result { write!(f, "{self:?}") }
}
impl From<LSErrCode> for i64 {
  #[allow(deprecated)] // clearly, we have to match deprecated values.
  fn from(value: LSErrCode) -> Self {
    match value {
      LSErrCode::ParseError => -32700,
      LSErrCode::InvalidRequest => -32600,
      LSErrCode::MethodNotFound => -32601,
      LSErrCode::InvalidParams => -32602,
      LSErrCode::InternalError => -32603,
      LSErrCode::ServerNotInitialized => -32002,
      LSErrCode::Unknown => -32001,
      LSErrCode::RequestFailed => -32803,
      LSErrCode::ServerCancelled => -32802,
      LSErrCode::ContentModified => -32801,
      LSErrCode::RequestCancelled => -32800,
    }
  }
}
