//! tokio_http11 - Tokio integration for shiguredo_http11
//!
//! tokio と tokio-rustls を使用した非同期 HTTP/1.1 クライアント/サーバーライブラリ。
//!
//! ## Features
//!
//! - `client` - HTTP/HTTPS クライアント機能 (デフォルト有効)
//! - `server` - HTTP/HTTPS サーバー機能 (デフォルト有効)
//! - `full` - すべての機能を有効化
//!
//! ## 特徴
//!
//! - **shiguredo_http11 ベース**: Sans I/O ライブラリをベースにした設計
//! - **非同期 I/O**: tokio による完全非同期対応
//! - **TLS 対応**: tokio-rustls による HTTPS 対応
//! - **Keep-Alive**: HTTP/1.1 Keep-Alive 接続のサポート
//!
//! ## クライアント
//!
//! ```ignore
//! use tokio_http11::Client;
//!
//! // GET
//! let client = Client::new();
//! let response = client.get("http://example.com/path").await?;
//!
//! // ヘッダー追加
//! let response = client.get("http://example.com")
//!     .header("User-Agent", "my-app")
//!     .await?;
//!
//! // POST with body
//! let response = client.post("http://example.com/api")
//!     .header("Content-Type", "application/json")
//!     .body(b"{\"key\": \"value\"}")
//!     .await?;
//!
//! // HTTPS (OS のルート証明書を自動使用)
//! let response = client.get("https://example.com").await?;
//! ```
//!
//! ## サーバー
//!
//! ```ignore
//! use tokio_http11::{Server, Request, Response};
//!
//! async fn handler(request: Request) -> Response {
//!     Response::new(200, "OK")
//!         .header("Content-Type", "text/plain")
//!         .body(b"Hello, World!".to_vec())
//! }
//!
//! // HTTP
//! let server = Server::bind("0.0.0.0:8080").await?;
//! server.serve(handler).await?;
//!
//! // HTTPS
//! let server = Server::bind("0.0.0.0:8443").await?.tls(tls_config);
//! server.serve(handler).await?;
//! ```

#[cfg(feature = "client")]
pub mod client;
pub mod error;
pub mod response_ext;
#[cfg(feature = "server")]
pub mod server;

#[cfg(feature = "client")]
pub use client::{Client, Connection, ConnectionRequestBuilder, RequestBuilder, parse_url};
pub use error::{Error, Result};
pub use response_ext::{JsonError, ResponseExt};
#[cfg(feature = "server")]
pub use server::{Handler, Server};

// shiguredo_http11 の型を re-export
pub use shiguredo_http11::{Request, Response};
