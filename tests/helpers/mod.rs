//! `tests/test_*.rs` から共通利用するヘルパー
//!
//! `cargo test` ではそれぞれの `tests/test_*.rs` がクレート扱いされ、`mod helpers;`
//! で取り込んだ場合に未使用 item が dead_code 警告になるため、各サブモジュール側で
//! `#![allow(dead_code)]` を付ける。

pub mod quoted_string;
