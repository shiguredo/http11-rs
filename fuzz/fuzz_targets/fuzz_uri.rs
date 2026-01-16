#![no_main]

use libfuzzer_sys::fuzz_target;
use shiguredo_http11::uri::{Uri, percent_decode, percent_encode, resolve, normalize};

fuzz_target!(|data: &[u8]| {
    // UTF-8 文字列として解釈できる場合のみテスト
    if let Ok(s) = std::str::from_utf8(data) {
        // URI パース
        if let Ok(uri) = Uri::parse(s) {
            // パース成功したら各種操作を実行
            let _ = uri.scheme();
            let _ = uri.authority();
            let _ = uri.host();
            let _ = uri.port();
            let _ = uri.path();
            let _ = uri.query();
            let _ = uri.fragment();
            let _ = uri.origin_form();
            let _ = uri.is_absolute();
            let _ = uri.is_relative();
            let _ = uri.as_str();

            // 正規化
            let _ = normalize(&uri);

            // 相対 URI 解決 (base として使用)
            if uri.is_absolute() {
                if let Ok(relative) = Uri::parse("/test") {
                    let _ = resolve(&uri, &relative);
                }
            }
        }

        // パーセントエンコード/デコード
        let encoded = percent_encode(s);
        // エンコードした結果をデコードしてラウンドトリップ確認
        if let Ok(decoded) = percent_decode(&encoded) {
            assert_eq!(decoded, s, "roundtrip failed");
        }

        // パーセントデコード (任意の入力)
        let _ = percent_decode(s);
    }
});
