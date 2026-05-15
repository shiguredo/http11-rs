//! ETag のパニック安全性と Display ラウンドトリップを検証する
//!
//! - EntityTag: 任意入力でパースし、is_weak/is_strong/tag アクセサと
//!   strong_compare/weak_compare 比較メソッドを呼び出す。
//!   Display 出力の再パースで weak フラグと tag の一致を確認する
//! - ETag リスト: parse_etag_list() でパースし、is_any() と
//!   Display ラウンドトリップを検証する

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
            let _ = EntityTag::parse(&displayed);

            // 比較メソッド
            let _ = etag.strong_compare(&etag);
            let _ = etag.weak_compare(&etag);
        }

        // ETag リストパース
        if let Ok(list) = parse_etag_list(s) {
            let _ = list.is_any();

            // Display 実装のテスト
            let displayed = list.to_string();
            let _ = parse_etag_list(&displayed);
        }
    }
});
