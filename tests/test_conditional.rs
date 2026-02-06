//! 条件付きリクエストのユニットテスト

use shiguredo_http11::conditional::{
    ConditionalError, IfMatch, IfModifiedSince, IfNoneMatch, IfRange, IfUnmodifiedSince,
};
use shiguredo_http11::etag::EntityTag;

// ========================================
// ConditionalError のテスト
// ========================================

#[test]
fn test_conditional_error_display() {
    let errors = [
        (ConditionalError::Empty, "empty conditional header"),
        (
            ConditionalError::InvalidFormat,
            "invalid conditional header format",
        ),
        (
            ConditionalError::ETagError,
            "invalid etag in conditional header",
        ),
        (
            ConditionalError::DateError,
            "invalid date in conditional header",
        ),
    ];

    for (error, expected) in errors {
        assert_eq!(error.to_string(), expected);
    }
}

// ========================================
// IfMatch のテスト
// ========================================

#[test]
fn test_if_match_wildcard() {
    let im = IfMatch::parse("*").unwrap();
    assert!(im.is_any());
    assert!(im.matches(&EntityTag::strong("anything").unwrap()));
    assert_eq!(im.to_string(), "*");
}

// ========================================
// IfNoneMatch のテスト
// ========================================

#[test]
fn test_if_none_match_wildcard() {
    let inm = IfNoneMatch::parse("*").unwrap();
    assert!(inm.is_any());
    // * は全てに一致するので、どの ETag でも処理しない
    assert!(!inm.matches(&EntityTag::strong("anything").unwrap()));
}

// ========================================
// IfModifiedSince のテスト
// ========================================

#[test]
fn test_if_modified_since_parse_errors() {
    assert!(matches!(
        IfModifiedSince::parse("invalid date"),
        Err(ConditionalError::DateError)
    ));
    assert!(matches!(
        IfModifiedSince::parse("2024-01-01"),
        Err(ConditionalError::DateError)
    ));
}

// ========================================
// IfUnmodifiedSince のテスト
// ========================================

#[test]
fn test_if_unmodified_since_parse_errors() {
    assert!(matches!(
        IfUnmodifiedSince::parse("invalid date"),
        Err(ConditionalError::DateError)
    ));
}

// ========================================
// IfRange のテスト
// ========================================

#[test]
fn test_if_range_parse_errors() {
    // 空
    assert!(matches!(IfRange::parse(""), Err(ConditionalError::Empty)));
    assert!(matches!(
        IfRange::parse("   "),
        Err(ConditionalError::Empty)
    ));

    // 不正な形式
    assert!(matches!(
        IfRange::parse("invalid"),
        Err(ConditionalError::DateError)
    ));
}
