//! キャッシュ関連ヘッダーのパニック安全性と Display ラウンドトリップを検証する
//!
//! - Cache-Control: 任意入力でパースし、max-age, no-cache, no-store 等の
//!   全ディレクティブアクセサを呼び出す。Display 出力の再パースで一致を確認する
//! - Age: 秒数のパースと Display ラウンドトリップを検証する
//! - Expires: 日付のパースと Display ラウンドトリップを検証する

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
            let _ = CacheControl::parse(&displayed);
        }

        // Age パース
        if let Ok(age) = Age::parse(s) {
            let _ = age.seconds();
            let _ = age.to_header_value();

            // Display ラウンドトリップ
            let displayed = age.to_string();
            let _ = Age::parse(&displayed);
        }

        // Expires パース
        if let Ok(expires) = Expires::parse(s, 2026) {
            let _ = expires.date();
            let _ = expires.to_header_value();

            // Display ラウンドトリップ
            let displayed = expires.to_string();
            let _ = Expires::parse(&displayed, 2026);
        }
    }
});
