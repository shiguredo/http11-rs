#![no_main]

use libfuzzer_sys::fuzz_target;
use shiguredo_http11::etag::{EntityTag, parse_etag_list};

fuzz_target!(|data: &[u8]| {
    // UTF-8 文字列として解釈できる場合のみテスト
    if let Ok(s) = std::str::from_utf8(data) {
        // EntityTag パース
        if let Ok(etag) = EntityTag::parse(s) {
            let _ = etag.is_weak();
            let _ = etag.is_strong();
            let _ = etag.tag();

            // Display 実装のテスト
            let displayed = etag.to_string();

            // ラウンドトリップ
            if let Ok(reparsed) = EntityTag::parse(&displayed) {
                assert_eq!(etag.is_weak(), reparsed.is_weak());
                assert_eq!(etag.tag(), reparsed.tag());
            }

            // 比較メソッド
            let _ = etag.strong_compare(&etag);
            let _ = etag.weak_compare(&etag);
        }

        // ETag リストパース
        if let Ok(list) = parse_etag_list(s) {
            let _ = list.is_any();

            // Display 実装のテスト
            let displayed = list.to_string();

            // ラウンドトリップ
            if let Ok(reparsed) = parse_etag_list(&displayed) {
                assert_eq!(list.is_any(), reparsed.is_any());
            }
        }
    }
});
