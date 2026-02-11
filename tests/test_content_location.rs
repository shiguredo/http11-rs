//! Content-Location のユニットテスト

use shiguredo_http11::content_location::{ContentLocation, ContentLocationError};

// ========================================
// ContentLocationError のテスト
// ========================================

#[test]
fn test_content_location_error_display() {
    let errors = [
        (ContentLocationError::Empty, "empty Content-Location"),
        (
            ContentLocationError::InvalidUri,
            "invalid Content-Location URI",
        ),
    ];

    for (error, expected) in errors {
        assert_eq!(error.to_string(), expected);
    }
}

// ========================================
// パースエラーのテスト
// ========================================

#[test]
fn test_content_location_parse_errors() {
    // 空
    assert!(matches!(
        ContentLocation::parse(""),
        Err(ContentLocationError::Empty)
    ));
    assert!(matches!(
        ContentLocation::parse("   "),
        Err(ContentLocationError::Empty)
    ));

    // 不正な URI (IPv6 閉じ括弧なし)
    assert!(matches!(
        ContentLocation::parse("http://[::1"),
        Err(ContentLocationError::InvalidUri)
    ));

    // http/https URI で "://" がない場合は不正 (RFC 9110 Section 4.2)
    assert!(matches!(
        ContentLocation::parse("http:foo"),
        Err(ContentLocationError::InvalidUri)
    ));
    assert!(matches!(
        ContentLocation::parse("https:bar"),
        Err(ContentLocationError::InvalidUri)
    ));
}

#[test]
fn test_content_location_http_with_authority_ok() {
    // http:// 付きは正常
    let cl = ContentLocation::parse("http://example.com/path").unwrap();
    assert_eq!(cl.uri().host(), Some("example.com"));
}

#[test]
fn test_content_location_non_http_without_authority_ok() {
    // http/https でないスキームは "://" なしでも OK
    let cl = ContentLocation::parse("urn:isbn:0451450523").unwrap();
    assert_eq!(cl.uri().scheme(), Some("urn"));
}
