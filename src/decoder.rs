//! HTTP/1.1 デコーダーモジュール
//!
//! Sans I/O 設計に基づくストリーミングデコーダーを提供する。利用方法はクレート
//! 全体のドキュメント (crate root) を参照。

mod body;
mod buffer;
mod head;
mod phase;
mod request;
mod response;

// 公開 API
pub use body::{BodyKind, BodyProgress};
// クレート内部 API (encoder が TE 解析を共有する)
pub(crate) use body::{
    TransferEncodingResult, parse_transfer_encoding_for_request,
    parse_transfer_encoding_for_response,
};
pub use head::{HttpHead, RequestHead, ResponseHead};
pub use request::RequestDecoder;
pub use response::ResponseDecoder;
