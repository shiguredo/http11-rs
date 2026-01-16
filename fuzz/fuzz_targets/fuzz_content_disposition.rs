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

            // ラウンドトリップ
            if let Ok(reparsed) = ContentDisposition::parse(&displayed) {
                assert_eq!(cd.disposition_type(), reparsed.disposition_type());
                // filename* が存在しない場合のみ filename が一致
                if cd.filename_ext().is_none() {
                    assert_eq!(cd.filename(), reparsed.filename());
                }
                assert_eq!(cd.name(), reparsed.name());
            }
        }
    }
});
