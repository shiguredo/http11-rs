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
}
