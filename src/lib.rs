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
//! use shiguredo_http11::{EncodeError, Request, ResponseDecoder};
//!
//! fn build() -> Result<Vec<u8>, EncodeError> {
//!     // リクエストを作成してエンコード
//!     let request = Request::new("GET", "/")
//!         .unwrap()
//!         .header("Host", "example.com")
//!         .unwrap()
//!         .header("Connection", "close")
//!         .unwrap();
//!     request.encode()
//! }
//! let bytes = build().unwrap();
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
//! use shiguredo_http11::{EncodeError, RequestDecoder, Response, StatusCode};
//!
//! // リクエストをデコード
//! let mut decoder = RequestDecoder::new();
//! // 受信データを feed...
//! // decoder.feed(&received_data)?;
//! // if let Some(request) = decoder.decode()? { ... }
//!
//! fn build() -> Result<Vec<u8>, EncodeError> {
//!     // レスポンスを作成してエンコード
//!     let response = Response::with_status(StatusCode::OK)
//!         .header("Content-Type", "text/plain").unwrap()
//!         .body(b"Hello, World!".to_vec());
//!     response.encode()
//! }
//! let bytes = build().unwrap();
//! // bytes を送信...
//! ```

#![cfg_attr(not(test), no_std)]
// library target (src/) 向けの追加 deny (restriction / pedantic。単体では allow)。
// `not(test)` … lib を cfg(test) なしでビルドするときだけ有効。tests/ は別 crate のため届かない。
// `[workspace.lints.clippy]` に書かない理由 … 同じ package の tests/ にも lint が乗るため。
// src/ 内の #[test] / #[cfg(test)] … clippy.toml の allow-*-in-tests で緩和。
// Clippy 組み込みデフォルト (correctness / style 等) … ここでは触らない (CI の `-D warnings` で昇格)。

// no_std
// `core` で済む import に `std` を使うことを禁止する
#![cfg_attr(not(test), deny(clippy::std_instead_of_core))]
// `alloc` で済む import に `std` を使うことを禁止する
#![cfg_attr(not(test), deny(clippy::std_instead_of_alloc))]
// panic
// panic! を禁止する
#![cfg_attr(not(test), deny(clippy::panic))]
// unreachable! を禁止する
#![cfg_attr(not(test), deny(clippy::unreachable))]
// unwrap を Result / ? に寄せる
#![cfg_attr(not(test), deny(clippy::unwrap_used))]
// expect を明示的なエラー処理に寄せる
#![cfg_attr(not(test), deny(clippy::expect_used))]
// cast
// 黙って桁落ちする as を禁止する
#![cfg_attr(not(test), deny(clippy::cast_possible_truncation))]
// 符号付き変換の情報損失を禁止する
#![cfg_attr(not(test), deny(clippy::cast_sign_loss))]
// 符号付きから符号なしへのラップを禁止する
#![cfg_attr(not(test), deny(clippy::cast_possible_wrap))]
// 浮動小数から整数への精度損失を禁止する
#![cfg_attr(not(test), deny(clippy::cast_precision_loss))]
// as より TryFrom / try_into を優先する
#![cfg_attr(not(test), deny(clippy::checked_conversions))]
// 範囲外で panic しうるインデックス・スライスを禁止する
#![cfg_attr(not(test), deny(clippy::indexing_slicing))]
// Result を返す関数内の panic! / unreachable! 等を禁止する
#![cfg_attr(not(test), deny(clippy::panic_in_result_fn))]
// 長さ未検証の refutable スライスパターンを禁止する
#![cfg_attr(not(test), deny(clippy::index_refutable_slice))]
// ゼロ除算で panic しうる整数除算を禁止する
#![cfg_attr(not(test), deny(clippy::integer_division))]
// ゼロ除算で panic しうる整数剰余を禁止する
#![cfg_attr(not(test), deny(clippy::integer_division_remainder_used))]

extern crate alloc;

pub mod accept;
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
pub mod etag;
pub mod expect;
pub mod host;
mod limits;
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
pub use limits::DecoderLimits;
pub use request::Request;
pub use response::Response;
pub use status_code::{StatusClass, StatusCode};
