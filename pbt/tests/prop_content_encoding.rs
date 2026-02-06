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

// 大文字小文字混在のエンコーディング
fn mixed_case_encoding() -> impl Strategy<Value = String> {
    prop_oneof![
        Just("GZIP".to_string()),
        Just("Gzip".to_string()),
        Just("gZiP".to_string()),
        Just("DEFLATE".to_string()),
        Just("Deflate".to_string()),
        Just("COMPRESS".to_string()),
        Just("IDENTITY".to_string()),
    ]
}

// カスタムエンコーディング (token 文字のみ)
fn custom_encoding() -> impl Strategy<Value = String> {
    "[a-zA-Z][a-zA-Z0-9-]{0,15}".prop_map(|s| s)
}

// ========================================
// 単一エンコーディングのテスト
// ========================================

// 標準エンコーディングのラウンドトリップ
proptest! {
    #[test]
    fn prop_content_encoding_standard_roundtrip(enc in standard_encoding()) {
        let ce = ContentEncoding::parse(enc).unwrap();

        prop_assert_eq!(ce.encodings().len(), 1);

        // Display で正規化される
        let display = ce.to_string();
        prop_assert_eq!(display, enc);
    }
}

// 大文字小文字混在のパース（正規化される）
proptest! {
    #[test]
    fn prop_content_encoding_case_insensitive(enc in mixed_case_encoding()) {
        let ce = ContentEncoding::parse(&enc).unwrap();

        prop_assert_eq!(ce.encodings().len(), 1);

        // Display で小文字に正規化される
        let display = ce.to_string();
        prop_assert_eq!(display, enc.to_lowercase());
    }
}

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

// gzip を含む場合の has_gzip
proptest! {
    #[test]
    fn prop_content_encoding_has_gzip(prefix in proptest::collection::vec(standard_encoding(), 0..3)) {
        let mut all = prefix.clone();
        all.push("gzip");

        let input = all.join(", ");
        let ce = ContentEncoding::parse(&input).unwrap();

        prop_assert!(ce.has_gzip());
    }
}

// deflate を含む場合の has_deflate
proptest! {
    #[test]
    fn prop_content_encoding_has_deflate(prefix in proptest::collection::vec(standard_encoding(), 0..3)) {
        let mut all = prefix.clone();
        all.push("deflate");

        let input = all.join(", ");
        let ce = ContentEncoding::parse(&input).unwrap();

        prop_assert!(ce.has_deflate());
    }
}

// compress を含む場合の has_compress
proptest! {
    #[test]
    fn prop_content_encoding_has_compress(prefix in proptest::collection::vec(standard_encoding(), 0..3)) {
        let mut all = prefix.clone();
        all.push("compress");

        let input = all.join(", ");
        let ce = ContentEncoding::parse(&input).unwrap();

        prop_assert!(ce.has_compress());
    }
}

// identity を含む場合の has_identity
proptest! {
    #[test]
    fn prop_content_encoding_has_identity(prefix in proptest::collection::vec(standard_encoding(), 0..3)) {
        let mut all = prefix.clone();
        all.push("identity");

        let input = all.join(", ");
        let ce = ContentEncoding::parse(&input).unwrap();

        prop_assert!(ce.has_identity());
    }
}

// ========================================
// Clone と PartialEq のテスト
// ========================================

proptest! {
    #[test]
    fn prop_content_encoding_clone_eq(encodings in proptest::collection::vec(standard_encoding(), 1..5)) {
        let input = encodings.join(", ");
        let ce = ContentEncoding::parse(&input).unwrap();
        let cloned = ce.clone();

        prop_assert_eq!(ce, cloned);
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
