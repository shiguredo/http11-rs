//! Content-Encoding のユニットテスト

use shiguredo_http11::content_encoding::{ContentCoding, ContentEncoding, ContentEncodingError};

// ========================================
// ContentEncodingError のテスト
// ========================================

#[test]
fn test_content_encoding_error_display() {
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

// ========================================
// ContentCoding のテスト
// ========================================

#[test]
fn test_content_coding_as_str() {
    assert_eq!(ContentCoding::Gzip.as_str(), "gzip");
    assert_eq!(ContentCoding::Deflate.as_str(), "deflate");
    assert_eq!(ContentCoding::Compress.as_str(), "compress");
    assert_eq!(ContentCoding::Identity.as_str(), "identity");
    assert_eq!(ContentCoding::Other("br".to_string()).as_str(), "br");
}

// ========================================
// パースエラーのテスト
// ========================================

// RFC 9110 Section 5.6.1.2: 空の値は空リストとして受理する
#[test]
fn test_content_encoding_parse_empty() {
    let ce = ContentEncoding::parse("").unwrap();
    assert!(ce.encodings().is_empty());

    let ce = ContentEncoding::parse("   ").unwrap();
    assert!(ce.encodings().is_empty());

    let ce = ContentEncoding::parse(",,,").unwrap();
    assert!(ce.encodings().is_empty());
}

#[test]
fn test_content_encoding_parse_errors() {
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

#[test]
fn test_content_encoding_trailing_comma() {
    let ce = ContentEncoding::parse("gzip,").unwrap();
    assert_eq!(ce.encodings().len(), 1);
    assert!(ce.has_gzip());

    let ce = ContentEncoding::parse("gzip, deflate,").unwrap();
    assert_eq!(ce.encodings().len(), 2);
}

#[test]
fn test_content_encoding_empty_tokens() {
    let ce = ContentEncoding::parse("gzip,, deflate").unwrap();
    assert_eq!(ce.encodings().len(), 2);
    assert!(ce.has_gzip());
    assert!(ce.has_deflate());
}
