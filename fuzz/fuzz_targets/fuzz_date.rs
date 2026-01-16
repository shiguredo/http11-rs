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

            // Display 出力を再パース (ラウンドトリップ)
            if let Ok(reparsed) = HttpDate::parse(&displayed) {
                assert_eq!(date.day(), reparsed.day());
                assert_eq!(date.month(), reparsed.month());
                assert_eq!(date.year(), reparsed.year());
                assert_eq!(date.hour(), reparsed.hour());
                assert_eq!(date.minute(), reparsed.minute());
                assert_eq!(date.second(), reparsed.second());
            }
        }
    }
});
