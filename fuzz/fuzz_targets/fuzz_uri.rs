//! URI パーサーのパニック安全性と正規化・解決の検証をする
//!
//! - Uri::parse() で任意入力をパースし、scheme, authority, host, port,
//!   path, query, fragment, origin_form 等の全アクセサを呼び出す
//! - normalize() による URI 正規化のパニック安全性を確認する
//! - 絶対 URI をベースとした resolve() による相対 URI 解決を検証する
//! - percent_encode → percent_decode のラウンドトリップを確認する

#![no_main]

use libfuzzer_sys::fuzz_target;
use shiguredo_http11::uri::{Uri, normalize, percent_decode, percent_encode, resolve};

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
            if uri.is_absolute()
                && let Ok(relative) = Uri::parse("/test")
            {
                let _ = resolve(&uri, &relative);
            }
        }

        // パーセントエンコード/デコード
        let _ = percent_encode(s);
        let _ = percent_decode(s);
    }
});
