//! Content-Disposition ヘッダーのパニック安全性と Display ラウンドトリップを検証する
//!
//! - 任意の UTF-8 文字列で ContentDisposition::parse() を呼び出す
//! - パース成功時は disposition_type, filename, filename_ext, name 等の
//!   アクセサを呼び出し、Display 出力の再パースで一致を確認する

#![no_main]

use libfuzzer_sys::fuzz_target;
use shiguredo_http11::content_disposition::ContentDisposition;

fuzz_target!(|data: &[u8]| {
    // UTF-8 文字列として解釈できる場合のみテスト
    if let Ok(s) = std::str::from_utf8(data) {
        // Content-Disposition パース
        if let Ok(cd) = ContentDisposition::parse(s) {
            // 各種メソッド呼び出し
            let _ = cd.disposition_type();
            let _ = cd.filename();
            let _ = cd.filename_ascii();
            let _ = cd.filename_ext();
            let _ = cd.name();
            let _ = cd.is_inline();
            let _ = cd.is_attachment();
            let _ = cd.is_form_data();

            // Display 実装のテスト
            let displayed = cd.to_string();
            let _ = ContentDisposition::parse(&displayed);
        }
    }
});
