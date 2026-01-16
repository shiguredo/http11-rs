#![no_main]

use libfuzzer_sys::fuzz_target;
use shiguredo_http11::range::{AcceptRanges, ContentRange, Range};

fuzz_target!(|data: &[u8]| {
    // UTF-8 文字列として解釈できる場合のみテスト
    if let Ok(s) = std::str::from_utf8(data) {
        // Range パース
        if let Ok(range) = Range::parse(s) {
            let _ = range.unit();
            let _ = range.is_bytes();
            let _ = range.ranges();
            let _ = range.first();

            // Display ラウンドトリップ
            let displayed = range.to_string();
            if let Ok(reparsed) = Range::parse(&displayed) {
                assert_eq!(range.unit(), reparsed.unit());
                assert_eq!(range.ranges().len(), reparsed.ranges().len());
            }

            // to_bounds テスト
            for spec in range.ranges() {
                let _ = spec.to_bounds(1000);
                let _ = spec.to_bounds(0);
            }
        }

        // Content-Range パース
        if let Ok(cr) = ContentRange::parse(s) {
            let _ = cr.unit();
            let _ = cr.start();
            let _ = cr.end();
            let _ = cr.complete_length();
            let _ = cr.length();
            let _ = cr.is_unsatisfied();

            // Display ラウンドトリップ
            let displayed = cr.to_string();
            if let Ok(reparsed) = ContentRange::parse(&displayed) {
                assert_eq!(cr.start(), reparsed.start());
                assert_eq!(cr.end(), reparsed.end());
            }
        }

        // Accept-Ranges パース
        if let Ok(ar) = AcceptRanges::parse(s) {
            let _ = ar.units();
            let _ = ar.accepts_bytes();
            let _ = ar.is_none();
        }
    }
});
