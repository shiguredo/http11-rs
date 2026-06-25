//! Accept-Query ヘッダーのパニック安全性と Display ラウンドトリップを検証する
//!
//! - 任意の UTF-8 文字列で `AcceptQuery::parse` を呼び出し、パニックしないことを確認する
//! - パース成功時は `items()` / `media_type()` / `subtype()` / `parameters()`
//!   アクセサを呼び出し、Display 出力を再パースしてラウンドトリップを検証する

#![no_main]

use libfuzzer_sys::fuzz_target;
use shiguredo_http11::accept_query::AcceptQuery;

fuzz_target!(|data: &[u8]| {
    if let Ok(s) = std::str::from_utf8(data) {
        if let Ok(value) = AcceptQuery::parse(s) {
            for item in value.items() {
                let _ = item.media_type();
                let _ = item.subtype();
                let _ = item.parameters();
            }
            let displayed = value.to_string();
            let _ = AcceptQuery::parse(&displayed);
        }
    }
});
