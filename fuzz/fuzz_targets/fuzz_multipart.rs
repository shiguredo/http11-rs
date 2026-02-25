//! MultipartParser の任意バイト列に対するパニック安全性を検証する
//!
//! - 複数の boundary 文字列 ("boundary", "----WebKitFormBoundary", "abc123",
//!   "---") を用いて、同じ任意データに対するパースを試行する
//! - 各パートの name, filename, content_type, body, body_str, is_file
//!   アクセサを呼び出し、パニックしないことを確認する

#![no_main]

use libfuzzer_sys::fuzz_target;
use shiguredo_http11::multipart::MultipartParser;

fuzz_target!(|data: &[u8]| {
    // 様々な境界でパースを試行
    let boundaries = ["boundary", "----WebKitFormBoundary", "abc123", "---"];

    for boundary in boundaries {
        let mut parser = MultipartParser::new(boundary);
        parser.feed(data);

        // パニックしなければ OK
        while let Ok(Some(part)) = parser.next_part() {
            let _ = part.name();
            let _ = part.filename();
            let _ = part.content_type();
            let _ = part.body();
            let _ = part.body_str();
            let _ = part.is_file();
        }
    }
});
