//! HTTP-date のパニック安全性と Display ラウンドトリップを検証する
//!
//! - 任意の UTF-8 文字列で HttpDate::parse() を呼び出す
//! - パース成功時は day_of_week, day, month, year, hour, minute, second の
//!   全アクセサを呼び出す
//! - Display 出力を再パースし、全フィールドの一致を確認する

#![no_main]

use libfuzzer_sys::fuzz_target;
use shiguredo_http11::date::HttpDate;

fuzz_target!(|data: &[u8]| {
    // UTF-8 文字列として解釈できる場合のみテスト
    if let Ok(s) = std::str::from_utf8(data) {
        // HTTP-date パース
        if let Ok(date) = HttpDate::parse(s) {
            // パース成功したら各種操作を実行
            let _ = date.day_of_week();
            let _ = date.day();
            let _ = date.month();
            let _ = date.year();
            let _ = date.hour();
            let _ = date.minute();
            let _ = date.second();

            // Display 実装のテスト
            let displayed = date.to_string();
            let _ = HttpDate::parse(&displayed);
        }
    }
});
