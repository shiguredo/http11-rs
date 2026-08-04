//! `tests/test_*.rs` から共通利用するヘルパー
//!
//! 本ファイルはテストクレートのルートとして扱われる。クレートルートのサブモジュールは
//! ルートファイルと同じディレクトリからしか解決できない (Rust のモジュール解決規則) ため、
//! ファイル構成を `<module>.rs` + `<module>/<submodule>.rs` に保つ目的で
//! `#[path]` で明示する。

#[path = "helpers/quoted_string.rs"]
pub mod quoted_string;
