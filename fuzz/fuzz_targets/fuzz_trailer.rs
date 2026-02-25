//! Trailer ヘッダーのパニック安全性と Display ラウンドトリップを検証する

#![no_main]

use libfuzzer_sys::fuzz_target;
use shiguredo_http11::trailer::Trailer;

fuzz_target!(|data: &[u8]| {
    if let Ok(s) = std::str::from_utf8(data) {
        if let Ok(value) = Trailer::parse(s) {
            let _ = value.fields();
            let displayed = value.to_string();
            let _ = Trailer::parse(&displayed);
        }
    }
});
