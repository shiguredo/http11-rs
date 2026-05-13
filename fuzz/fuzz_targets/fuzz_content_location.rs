//! Content-Location ヘッダーのパニック安全性と Display ラウンドトリップを検証する

#![no_main]

use libfuzzer_sys::fuzz_target;
use shiguredo_http11::content_location::ContentLocation;

fuzz_target!(|data: &[u8]| {
    if let Ok(s) = std::str::from_utf8(data)
        && let Ok(value) = ContentLocation::parse(s)
    {
        let _ = value.uri();
        let _ = value.uri().as_str();
        let displayed = value.to_string();
        let _ = ContentLocation::parse(&displayed);
    }
});
