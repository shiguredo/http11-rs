#![no_main]

use libfuzzer_sys::fuzz_target;
use shiguredo_http11::cookie::{Cookie, SetCookie};

fn cookie_fuzz_normalize_value(value: &str) -> &str {
    let trimmed = value.trim();
    if trimmed.starts_with('"') && trimmed.ends_with('"') && trimmed.len() >= 2 {
        &trimmed[1..trimmed.len() - 1]
    } else {
        trimmed
    }
}

fuzz_target!(|data: &[u8]| {
    // UTF-8 文字列として解釈できる場合のみテスト
    if let Ok(s) = std::str::from_utf8(data) {
        // Cookie パース
        if let Ok(cookies) = Cookie::parse(s) {
            for cookie in &cookies {
                let _ = cookie.name();
                let _ = cookie.value();
                // Display 実装のテスト
                let displayed = cookie.to_string();
                // ラウンドトリップ
                if let Ok(reparsed) = Cookie::parse(&displayed) {
                    assert_eq!(reparsed.len(), 1);
                    assert_eq!(cookie.name(), reparsed[0].name());
                    assert_eq!(
                        cookie_fuzz_normalize_value(cookie.value()),
                        reparsed[0].value()
                    );
                }
            }
        }

        // SetCookie パース
        if let Ok(set_cookie) = SetCookie::parse(s) {
            let _ = set_cookie.name();
            let _ = set_cookie.value();
            let _ = set_cookie.expires();
            let _ = set_cookie.max_age();
            let _ = set_cookie.domain();
            let _ = set_cookie.path();
            let _ = set_cookie.secure();
            let _ = set_cookie.http_only();
            let _ = set_cookie.same_site();

            // Display 実装のテスト
            let displayed = set_cookie.to_string();

            // ラウンドトリップ (単純な name=value のみ確実に一致)
            if let Ok(reparsed) = SetCookie::parse(&displayed) {
                assert_eq!(set_cookie.name(), reparsed.name());
                assert_eq!(
                    cookie_fuzz_normalize_value(set_cookie.value()),
                    reparsed.value()
                );
                assert_eq!(set_cookie.path(), reparsed.path());
                assert_eq!(set_cookie.domain(), reparsed.domain());
                assert_eq!(set_cookie.max_age(), reparsed.max_age());
                assert_eq!(set_cookie.secure(), reparsed.secure());
                assert_eq!(set_cookie.http_only(), reparsed.http_only());
                assert_eq!(set_cookie.same_site(), reparsed.same_site());
            }
        }
    }
});
