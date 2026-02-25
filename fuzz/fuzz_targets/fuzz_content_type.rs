//! Content-Type ヘッダーのパニック安全性と Display ラウンドトリップを検証する
//!
//! - 任意の UTF-8 文字列で ContentType::parse() を呼び出す
//! - パース成功時は media_type, subtype, charset, boundary 等のアクセサと
//!   is_text, is_json, is_multipart 等の判定メソッドを呼び出す
//! - Display 出力を再パースし、media_type と subtype の一致を確認する

#![no_main]

use libfuzzer_sys::fuzz_target;
use shiguredo_http11::content_type::ContentType;

fuzz_target!(|data: &[u8]| {
    // UTF-8 文字列として解釈できる場合のみテスト
    if let Ok(s) = std::str::from_utf8(data) {
        // Content-Type パース
        if let Ok(ct) = ContentType::parse(s) {
            // パース成功したら各種操作を実行
            let _ = ct.media_type();
            let _ = ct.subtype();
            let _ = ct.mime_type();
            let _ = ct.charset();
            let _ = ct.boundary();
            let _ = ct.parameters();
            let _ = ct.is_text();
            let _ = ct.is_json();
            let _ = ct.is_multipart();
            let _ = ct.is_form_data();
            let _ = ct.is_form_urlencoded();

            // Display 実装のテスト
            let displayed = ct.to_string();

            // Display 出力を再パース (ラウンドトリップ)
            if let Ok(reparsed) = ContentType::parse(&displayed) {
                assert_eq!(ct.media_type(), reparsed.media_type());
                assert_eq!(ct.subtype(), reparsed.subtype());
            }
        }
    }
});
