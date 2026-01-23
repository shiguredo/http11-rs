//! Content-Encoding ヘッダーのプロパティテスト

use proptest::prelude::*;
use shiguredo_http11::content_encoding::{ContentCoding, ContentEncoding, ContentEncodingError};

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
// ContentEncodingError のテスト
// ========================================

#[test]
fn prop_content_encoding_error_display() {
    let errors = [
        (ContentEncodingError::Empty, "empty Content-Encoding"),
        (
            ContentEncodingError::InvalidFormat,
            "invalid Content-Encoding format",
        ),
        (
            ContentEncodingError::InvalidEncoding,
            "invalid Content-Encoding token",
        ),
    ];

    for (error, expected) in errors {
        assert_eq!(error.to_string(), expected);
    }
}

#[test]
fn prop_content_encoding_error_is_error_trait() {
    let error: Box<dyn std::error::Error> = Box::new(ContentEncodingError::Empty);
    assert_eq!(error.to_string(), "empty Content-Encoding");
}

// ========================================
// ContentCoding のテスト
// ========================================

#[test]
fn prop_content_coding_as_str() {
    assert_eq!(ContentCoding::Gzip.as_str(), "gzip");
    assert_eq!(ContentCoding::Deflate.as_str(), "deflate");
    assert_eq!(ContentCoding::Compress.as_str(), "compress");
    assert_eq!(ContentCoding::Identity.as_str(), "identity");
    assert_eq!(ContentCoding::Other("br".to_string()).as_str(), "br");
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
// パースエラーのテスト
// ========================================

#[test]
fn prop_content_encoding_parse_errors() {
    // 空
    assert!(matches!(
        ContentEncoding::parse(""),
        Err(ContentEncodingError::Empty)
    ));
    assert!(matches!(
        ContentEncoding::parse("   "),
        Err(ContentEncodingError::Empty)
    ));

    // カンマのみ
    assert!(matches!(
        ContentEncoding::parse(",,,"),
        Err(ContentEncodingError::Empty)
    ));

    // 不正なトークン（空白を含む）
    assert!(matches!(
        ContentEncoding::parse("g zip"),
        Err(ContentEncodingError::InvalidEncoding)
    ));

    // 不正なトークン（特殊文字）
    assert!(matches!(
        ContentEncoding::parse("gzip<script>"),
        Err(ContentEncodingError::InvalidEncoding)
    ));
}

// ========================================
// 境界値テスト
// ========================================

// 末尾のカンマは許可される
#[test]
fn prop_content_encoding_trailing_comma() {
    let ce = ContentEncoding::parse("gzip,").unwrap();
    assert_eq!(ce.encodings().len(), 1);
    assert!(ce.has_gzip());

    let ce = ContentEncoding::parse("gzip, deflate,").unwrap();
    assert_eq!(ce.encodings().len(), 2);
}

// 連続したカンマは空のトークンとしてスキップされる
#[test]
fn prop_content_encoding_empty_tokens() {
    let ce = ContentEncoding::parse("gzip,, deflate").unwrap();
    assert_eq!(ce.encodings().len(), 2);
    assert!(ce.has_gzip());
    assert!(ce.has_deflate());
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
