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
