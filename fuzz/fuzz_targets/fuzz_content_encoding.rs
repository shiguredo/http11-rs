//! Content-Encoding ヘッダーのパニック安全性と Display ラウンドトリップを検証する

#![no_main]

use libfuzzer_sys::fuzz_target;
use shiguredo_http11::content_encoding::ContentEncoding;

fuzz_target!(|data: &[u8]| {
    if let Ok(s) = std::str::from_utf8(data)
        && let Ok(value) = ContentEncoding::parse(s)
    {
        for encoding in value.encodings() {
            let _ = encoding.as_str();
        }
        let _ = value.has_gzip();
        let _ = value.has_deflate();
        let _ = value.has_compress();
        let _ = value.has_identity();
        let displayed = value.to_string();
        let _ = ContentEncoding::parse(&displayed);
    }
});
