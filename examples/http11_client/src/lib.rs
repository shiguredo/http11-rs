//! http11_client の library 部分
//!
//! integration test (`tests/`) や他コンポーネントから本サンプルの内部関数を呼ぶための
//! 薄い library レイヤー。CLI のフロントエンドは `src/main.rs` 側に置く。

pub mod decompressor;
mod transport;
mod url;

pub use transport::{http_request, https_request};
pub use url::parse_url;
