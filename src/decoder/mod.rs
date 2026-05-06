//! HTTP/1.1 デコーダーモジュール
//!
//! Sans I/O 設計に基づくストリーミングデコーダーを提供。
//!
//! ## 使い方
//!
//! ### ストリーミング API (推奨)
//!
//! ```rust
//! use shiguredo_http11::{RequestDecoder, BodyKind, BodyProgress};
//!
//! let mut decoder = RequestDecoder::new();
//!
//! // データを投入
//! decoder.feed(b"GET / HTTP/1.1\r\nHost: example.com\r\nContent-Length: 5\r\n\r\nhello").unwrap();
//!
//! // ヘッダーをデコード
//! let (head, body_kind) = decoder.decode_headers().unwrap().unwrap();
//! assert_eq!(head.method, "GET");
//!
//! // ボディをストリーミングで読み取り
//! let mut body = Vec::new();
//! if let BodyKind::ContentLength(_) | BodyKind::Chunked = body_kind {
//!     loop {
//!         // バッファにあるボディデータを消費
//!         if let Some(data) = decoder.peek_body() {
//!             body.extend_from_slice(data);
//!             let len = data.len();
//!             match decoder.consume_body(len).unwrap() {
//!                 BodyProgress::Complete { .. } => break,
//!                 // NeedData (chunked CRLF 不足) でも loop 先頭に戻って peek_body 再試行。
//!                 // peek_body が None なら progress() に fall through する。
//!                 BodyProgress::Advanced | BodyProgress::NeedData => continue,
//!             }
//!         }
//!         // peek_body() が None → 状態機械を進める
//!         match decoder.progress().unwrap() {
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
//! decoder.feed(b"HTTP/1.1 200 OK\r\n\r\nhello world").unwrap();
//!
//! let (_head, body_kind) = decoder.decode_headers().unwrap().unwrap();
//! assert_eq!(body_kind, BodyKind::CloseDelimited);
//!
//! // mark_eof() 前に peek_body() でバッファ内の全ボディデータを消費する
//! let mut body = Vec::new();
//! while let Some(data) = decoder.peek_body() {
//!     body.extend_from_slice(data);
//!     let len = data.len();
//!     decoder.consume_body(len).unwrap();
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
//! decoder.feed(b"GET / HTTP/1.1\r\nHost: example.com\r\n\r\n").unwrap();
//! let request = decoder.decode().unwrap().unwrap();
//! assert_eq!(request.method, "GET");
//! ```

mod body;
mod buffer;
mod head;
mod phase;
mod request;
mod response;

// 公開 API
pub use body::{BodyKind, BodyProgress};
pub use head::{HttpHead, RequestHead, ResponseHead};
pub use request::RequestDecoder;
pub use response::ResponseDecoder;
