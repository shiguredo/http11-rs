//! Decoder のユニットテスト
//!
//! 元は `tests/test_decoder.rs` に集約されていたが、ファイル肥大化に伴い
//! 目的別のサブモジュールに分割した。
//! - `head`: ステータス行・リクエスト行のパース、Host ヘッダー、HTTP バージョン検証
//! - `body`: ボディキンド判定 (Content-Length / chunked / close-delimited / None) と
//!   chunked トレーラー / chunked CRLF 分割到着、peek_body_decompressed
//! - `streaming`: feed / decode_headers / decode / consume_body / take_remaining と
//!   CONNECT トンネルモード周辺
//! - `direct_buffer`: 直接書き込み API (mut_buf / advance_buf / available_buf)
//! - `decode_body`: 旧 `tests/test_decode_body.rs` に存在したボディデコード詳細テスト
//!   (Transfer-Encoding token 検証、chunk-ext ABNF、HTTP バージョン別 TE 拒否、
//!   IPv6 ブラケット検証など)

mod body;
mod decode_body;
mod direct_buffer;
mod head;
mod streaming;
