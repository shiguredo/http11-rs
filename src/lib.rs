//! # shiguredo_http11
//!
//! 依存なしの HTTP/1.1 スタイル テキストプロトコルライブラリ (Sans I/O)
//!
//! ## 特徴
//!
//! - **依存なし**: `core` / `alloc` のみ (no_std 対応)
//! - **Sans I/O**: I/O を完全に分離した設計
//! - **柔軟性**: HTTP/1.1, RTSP/1.0, RTSP/2.0 等に対応
//!
//! ## 使い方
//!
//! ### クライアント (リクエスト送信、レスポンス受信)
//!
//! ```rust
//! use shiguredo_http11::{EncodeError, Request};
//!
//! fn build() -> Result<Vec<u8>, EncodeError> {
//!     // リクエストを作成してエンコード
//!     let request = Request::new("GET", "/")
//!         .expect("リクエスト / レスポンスの構築は成功するはず (実装バグ)")
//!         .header("Host", "example.com")
//!         .expect("リクエスト / レスポンスの構築は成功するはず (実装バグ)")
//!         .header("Connection", "close")
//!         .expect("リクエスト / レスポンスの構築は成功するはず (実装バグ)");
//!     request.encode()
//! }
//! let bytes = build().expect("リクエスト / レスポンスの構築は成功するはず (実装バグ)");
//! // bytes を送信...
//! ```
//!
//! レスポンスの受信は「ストリーミング API」の節を参照。
//!
//! ### サーバー (リクエスト受信、レスポンス送信)
//!
//! ```rust
//! use shiguredo_http11::{EncodeError, Response, StatusCode};
//!
//! fn build() -> Result<Vec<u8>, EncodeError> {
//!     // レスポンスを作成してエンコード
//!     let response = Response::with_status(StatusCode::OK)
//!         .header("Content-Type", "text/plain").expect("リクエスト / レスポンスの構築は成功するはず (実装バグ)")
//!         .body(b"Hello, World!".to_vec());
//!     response.encode()
//! }
//! let bytes = build().expect("リクエスト / レスポンスの構築は成功するはず (実装バグ)");
//! // bytes を送信...
//! ```
//!
//! リクエストの受信は「ストリーミング API」の節を参照。
//!
//! ### ストリーミング API (推奨)
//!
//! ```rust
//! use shiguredo_http11::{RequestDecoder, BodyKind, BodyProgress};
//!
//! let mut decoder = RequestDecoder::new();
//!
//! // データを投入
//! decoder.feed(b"GET / HTTP/1.1\r\nHost: example.com\r\nContent-Length: 5\r\n\r\nhello").expect("デコードは成功するはず (実装バグ)");
//!
//! // ヘッダーをデコード
//! let (head, body_kind) = decoder.decode_headers().expect("結果は存在するはず (実装バグ)").expect("デコードは成功するはず (実装バグ)");
//! assert_eq!(head.method(), "GET");
//!
//! // ボディをストリーミングで読み取り
//! let mut body = Vec::new();
//! if let BodyKind::ContentLength(_) | BodyKind::Chunked = body_kind {
//!     loop {
//!         // バッファにあるボディデータを消費
//!         if let Some(data) = decoder.peek_body() {
//!             body.extend_from_slice(data);
//!             let len = data.len();
//!             match decoder.consume_body(len).expect("デコードは成功するはず (実装バグ)") {
//!                 BodyProgress::Complete { .. } => break,
//!                 // NeedData (chunked CRLF 不足) でも loop 先頭に戻って peek_body 再試行。
//!                 // peek_body が None なら progress() に fall through する。
//!                 BodyProgress::Advanced | BodyProgress::NeedData => continue,
//!             }
//!         }
//!         // peek_body() が None → 状態機械を進める
//!         match decoder.progress().expect("デコードは成功するはず (実装バグ)") {
//!             BodyProgress::Complete { .. } => break,
//!             // 状態が進んだ: peek_body 再試行のため loop 先頭へ
//!             BodyProgress::Advanced => continue,
//!             // バッファ不足: 実際の使用ではネットワーク I/O に戻って追加データを得る
//!             BodyProgress::NeedData => break,
//!         }
//!     }
//! }
//! assert_eq!(body, b"hello");
//! ```
//!
//! ### close-delimited ボディ (`ResponseDecoder` 専用)
//!
//! `mark_eof()` は `ResponseDecoder` にのみ存在する。
//! リクエストでは close-delimited を使わない。
//!
//! ```rust
//! use shiguredo_http11::{ResponseDecoder, BodyKind};
//!
//! let mut decoder = ResponseDecoder::new();
//! decoder.feed(b"HTTP/1.1 200 OK\r\n\r\nhello world").expect("デコードは成功するはず (実装バグ)");
//!
//! let (_head, body_kind) = decoder.decode_headers().expect("結果は存在するはず (実装バグ)").expect("デコードは成功するはず (実装バグ)");
//! assert_eq!(body_kind, BodyKind::CloseDelimited);
//!
//! // mark_eof() 前に peek_body() でバッファ内の全ボディデータを消費する
//! let mut body = Vec::new();
//! while let Some(data) = decoder.peek_body() {
//!     body.extend_from_slice(data);
//!     let len = data.len();
//!     decoder.consume_body(len).expect("デコードは成功するはず (実装バグ)");
//! }
//! // I/O レイヤーが接続切断を検知したら mark_eof() を呼ぶ。
//! // mark_eof() 後は phase が Complete に遷移し peek_body() は None を返す。
//! decoder.mark_eof();
//! assert_eq!(body, b"hello world");
//! ```
//!
//! ### 一括デコード API
//!
//! ```rust
//! use shiguredo_http11::RequestDecoder;
//!
//! let mut decoder = RequestDecoder::new();
//! decoder.feed(b"GET / HTTP/1.1\r\nHost: example.com\r\n\r\n").expect("デコードは成功するはず (実装バグ)");
//! let request = decoder.decode().expect("結果は存在するはず (実装バグ)").expect("デコードは成功するはず (実装バグ)");
//! assert_eq!(request.method(), "GET");
//! ```

#![cfg_attr(not(test), no_std)]
// 公開 API のドキュメント欠落を防ぐ
#![warn(missing_docs)]
extern crate alloc;

pub mod accept;
pub mod accept_query;
pub mod auth;
mod base64;
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
mod header_name;

pub mod etag;
pub mod expect;
pub mod host;
mod limits;
mod method;
pub mod multipart;
pub mod range;
mod request;
pub mod request_target;
mod response;
pub mod status_code;
pub mod trailer;
pub mod upgrade;
pub mod uri;
mod validate;
pub mod vary;

pub use decoder::{
    BodyKind, BodyProgress, HttpHead, RequestDecoder, RequestHead, ResponseDecoder, ResponseHead,
};
pub use encoder::{
    RequestEncoder, ResponseEncoder, encode_chunk, encode_chunks, encode_request,
    encode_request_headers, encode_response, encode_response_headers,
};
pub use error::{EncodeError, Error};
pub use header_name::{HeaderName, HeaderNameError};
pub use limits::DecoderLimits;
pub use method::{Method, MethodError};
pub use request::Request;
pub use response::Response;
pub use status_code::{StatusClass, StatusCode};
pub use uri::{Scheme, SchemeError};
