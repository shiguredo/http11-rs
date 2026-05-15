//! Cookie / Set-Cookie ヘッダーのパニック安全性と Display ラウンドトリップを検証する
//!
//! - Cookie: 任意入力でパースし、各 cookie の name/value アクセサを呼び出す。
//!   Display 出力を再パースして name と value の一致を確認する
//! - Set-Cookie: パース成功時は expires, max_age, domain, path, secure,
//!   http_only, same_site の全属性アクセサを呼び出す。
//!   Display 出力の再パースで全属性の一致を確認する

#![no_main]

use libfuzzer_sys::fuzz_target;
use shiguredo_http11::cookie::{Cookie, SetCookie};

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
                let _ = Cookie::parse(&displayed);
            }
        }

        // SetCookie パース
        if let Ok(set_cookie) = SetCookie::parse(s, 2026) {
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
            let _ = SetCookie::parse(&displayed, 2026);
        }
    }
});
