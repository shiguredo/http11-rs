//! Content-Encoding ヘッダーのプロパティテスト

use proptest::prelude::*;
use shiguredo_http11::content_encoding::ContentEncoding;

// ========================================
// Strategy 定義
// ========================================

// 標準的なエンコーディング
fn standard_encoding() -> impl Strategy<Value = &'static str> {
    prop_oneof![
        Just("gzip"),
        Just("deflate"),
        Just("compress"),
        Just("identity"),
    ]
}

// カスタムエンコーディング (token 文字のみ)
fn custom_encoding() -> impl Strategy<Value = String> {
    "[a-zA-Z][a-zA-Z0-9-]{0,15}".prop_map(|s| s)
}

// ========================================
// 単一エンコーディングのテスト
// ========================================

// カスタムエンコーディングのラウンドトリップ
proptest! {
    #[test]
    fn prop_content_encoding_custom_roundtrip(enc in custom_encoding()) {
        let ce = ContentEncoding::parse(&enc).unwrap();

        prop_assert_eq!(ce.encodings().len(), 1);

        // Display で小文字に正規化される
        let display = ce.to_string();
        prop_assert_eq!(display, enc.to_lowercase());
    }
}

// ========================================
// 複数エンコーディングのテスト
// ========================================

// 複数の標準エンコーディング
proptest! {
    #[test]
    fn prop_content_encoding_multiple(encodings in proptest::collection::vec(standard_encoding(), 1..5)) {
        let input = encodings.join(", ");
        let ce = ContentEncoding::parse(&input).unwrap();

        prop_assert_eq!(ce.encodings().len(), encodings.len());
    }
}

// ========================================
// no_panic テスト
// ========================================

proptest! {
    #[test]
    fn prop_content_encoding_parse_no_panic(s in "[ -~]{0,64}") {
        let _ = ContentEncoding::parse(&s);
    }
}
