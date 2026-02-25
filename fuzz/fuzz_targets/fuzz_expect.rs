//! Expect ヘッダーのパニック安全性と Display ラウンドトリップを検証する
//!
//! - 任意の UTF-8 文字列で Expect::parse() を呼び出す
//! - パース成功時は has_100_continue() と各 item の token/value/is_100_continue
//!   アクセサを呼び出す
//! - Display 出力を再パースしてパニック安全性を確認する

#![no_main]

use libfuzzer_sys::fuzz_target;
use shiguredo_http11::expect::Expect;

fuzz_target!(|data: &[u8]| {
    if let Ok(s) = std::str::from_utf8(data) {
        if let Ok(value) = Expect::parse(s) {
            let _ = value.has_100_continue();
            for item in value.items() {
                let _ = item.token();
                let _ = item.value();
                let _ = item.is_100_continue();
            }
            let displayed = value.to_string();
            let _ = Expect::parse(&displayed);
        }
    }
});
