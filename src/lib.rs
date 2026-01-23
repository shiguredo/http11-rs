//! # shiguredo_http11
//!
//! 依存なしの HTTP/1.1 スタイル テキストプロトコルライブラリ (Sans I/O)
//!
//! ## 特徴
//!
//! - **依存なし**: 標準ライブラリのみ使用
//! - **Sans I/O**: I/O を完全に分離した設計
//! - **柔軟性**: HTTP/1.1, RTSP/1.0, RTSP/2.0 等に対応
//!
//! ## 使い方
//!
//! ### クライアント (リクエスト送信、レスポンス受信)
//!
//! ```rust
//! use shiguredo_http11::{Request, ResponseDecoder};
//!
//! // リクエストを作成してエンコード
//! let request = Request::new("GET", "/")
//!     .header("Host", "example.com")
//!     .header("Connection", "close");
//! let bytes = request.encode();
//! // bytes を送信...
//!
//! // レスポンスをデコード
//! let mut decoder = ResponseDecoder::new();
//! // 受信データを feed...
//! // decoder.feed(&received_data)?;
//! // if let Some(response) = decoder.decode()? { ... }
//! ```
//!
//! ### サーバー (リクエスト受信、レスポンス送信)
//!
//! ```rust
//! use shiguredo_http11::{RequestDecoder, Response};
//!
//! // リクエストをデコード
//! let mut decoder = RequestDecoder::new();
//! // 受信データを feed...
//! // decoder.feed(&received_data)?;
//! // if let Some(request) = decoder.decode()? { ... }
//!
//! // レスポンスを作成してエンコード
//! let response = Response::new(200, "OK")
//!     .header("Content-Type", "text/plain")
//!     .body(b"Hello, World!".to_vec());
//! let bytes = response.encode();
//! // bytes を送信...
//! ```

pub mod accept;
pub mod auth;
pub mod cache;
pub mod compression;
pub mod conditional;
pub mod content_disposition;
pub mod content_encoding;
pub mod content_language;
pub mod content_location;
pub mod content_type;
pub mod cookie;
pub mod date;
mod decoder;
pub mod digest_fields;
mod encoder;
mod error;
pub mod etag;
pub mod expect;
pub mod host;
mod limits;
pub mod multipart;
pub mod range;
mod request;
mod response;
pub mod trailer;
pub mod upgrade;
pub mod uri;
pub mod vary;

pub use decoder::{
    BodyKind, BodyProgress, HttpHead, RequestDecoder, RequestHead, ResponseDecoder, ResponseHead,
};
pub use encoder::{
    encode_chunk, encode_chunks, encode_request, encode_request_headers, encode_response,
    encode_response_headers, RequestEncoder, ResponseEncoder,
};
pub use error::Error;
pub use limits::DecoderLimits;
pub use request::Request;
pub use response::Response;
