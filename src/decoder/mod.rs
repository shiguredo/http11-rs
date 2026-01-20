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
//! decoder.feed(b"GET / HTTP/1.1\r\nContent-Length: 5\r\n\r\nhello").unwrap();
//!
//! // ヘッダーをデコード
//! let (head, body_kind) = decoder.decode_headers().unwrap().unwrap();
//! assert_eq!(head.method, "GET");
//!
//! // ボディをストリーミングで読み取り
//! let mut body = Vec::new();
//! if let BodyKind::ContentLength(_) | BodyKind::Chunked = body_kind {
//!     loop {
//!         if let Some(data) = decoder.peek_body() {
//!             body.extend_from_slice(data);
//!             let len = data.len();
//!             if let BodyProgress::Complete { .. } = decoder.consume_body(len).unwrap() {
//!                 break;
//!             }
//!         } else {
//!             // peek_body() が None でも consume_body(0) で状態遷移を試みる
//!             // Chunked の場合、チャンクサイズ行や終端チャンクのパースが進む
//!             if let BodyProgress::Complete { .. } = decoder.consume_body(0).unwrap() {
//!                 break;
//!             }
//!             // Continue の場合は追加データが必要（実際の使用ではネットワーク I/O が必要）
//!             break;
//!         }
//!     }
//! }
//! assert_eq!(body, b"hello");
//! ```
//!
//! ### 一括デコード API
//!
//! ```rust
//! use shiguredo_http11::RequestDecoder;
//!
//! let mut decoder = RequestDecoder::new();
//! decoder.feed(b"GET / HTTP/1.1\r\n\r\n").unwrap();
//! let request = decoder.decode().unwrap().unwrap();
//! assert_eq!(request.method, "GET");
//! ```

mod body;
mod head;
mod phase;
mod request;
mod response;

// 公開 API
pub use body::{BodyKind, BodyProgress};
pub use head::{HttpHead, RequestHead, ResponseHead};
pub use request::RequestDecoder;
pub use response::ResponseDecoder;
