//! Content-Language ヘッダーのパニック安全性と Display ラウンドトリップを検証する

#![no_main]

use libfuzzer_sys::fuzz_target;
use shiguredo_http11::content_language::ContentLanguage;

fuzz_target!(|data: &[u8]| {
    if let Ok(s) = std::str::from_utf8(data)
        && let Ok(value) = ContentLanguage::parse(s)
    {
        let _ = value.tags();
        let displayed = value.to_string();
        let _ = ContentLanguage::parse(&displayed);
    }
});
