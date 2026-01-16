#![no_main]

use libfuzzer_sys::fuzz_target;
use shiguredo_http11::cache::{Age, CacheControl, Expires};

fuzz_target!(|data: &[u8]| {
    // UTF-8 文字列として解釈できる場合のみテスト
    if let Ok(s) = std::str::from_utf8(data) {
        // CacheControl パース
        if let Ok(cc) = CacheControl::parse(s) {
            let _ = cc.max_age();
            let _ = cc.s_maxage();
            let _ = cc.max_stale();
            let _ = cc.min_fresh();
            let _ = cc.stale_while_revalidate();
            let _ = cc.stale_if_error();
            let _ = cc.is_no_cache();
            let _ = cc.is_no_store();
            let _ = cc.is_no_transform();
            let _ = cc.is_only_if_cached();
            let _ = cc.is_must_revalidate();
            let _ = cc.is_proxy_revalidate();
            let _ = cc.is_must_understand();
            let _ = cc.is_public();
            let _ = cc.is_private();
            let _ = cc.is_immutable();
            let _ = cc.is_cacheable();
            let _ = cc.to_header_value();

            // Display ラウンドトリップ
            let displayed = cc.to_string();
            if let Ok(reparsed) = CacheControl::parse(&displayed) {
                assert_eq!(cc.max_age(), reparsed.max_age());
                assert_eq!(cc.is_no_cache(), reparsed.is_no_cache());
                assert_eq!(cc.is_no_store(), reparsed.is_no_store());
            }
        }

        // Age パース
        if let Ok(age) = Age::parse(s) {
            let _ = age.seconds();
            let _ = age.to_header_value();

            // Display ラウンドトリップ
            let displayed = age.to_string();
            let reparsed = Age::parse(&displayed).unwrap();
            assert_eq!(age.seconds(), reparsed.seconds());
        }

        // Expires パース
        if let Ok(expires) = Expires::parse(s) {
            let _ = expires.date();
            let _ = expires.to_header_value();

            // Display ラウンドトリップ
            let displayed = expires.to_string();
            if let Ok(reparsed) = Expires::parse(&displayed) {
                assert_eq!(expires.date().year(), reparsed.date().year());
            }
        }
    }
});
