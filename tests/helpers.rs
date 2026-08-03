//! `tests/test_*.rs` から共通利用するヘルパー
//!
//! `cargo test` ではそれぞれの `tests/test_*.rs` がクレート扱いされ、`mod helpers;`
//! で取り込んだ場合に未使用 item が dead_code 警告になるため、各サブモジュール側で
//! `#![allow(dead_code)]` を付ける。
//!
//! 本ファイルはテストクレートのルートとして扱われる。クレートルートのサブモジュールは
//! ルートファイルと同じディレクトリからしか解決できない (Rust のモジュール解決規則) ため、
//! ファイル構成を `<module>.rs` + `<module>/<submodule>.rs` に保つ目的で
//! `#[path]` で明示する。

#[path = "helpers/quoted_string.rs"]
pub mod quoted_string;
