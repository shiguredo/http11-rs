//! 条件付きリクエストのユニットテスト

use shiguredo_http11::conditional::{
    ConditionalError, IfMatch, IfModifiedSince, IfNoneMatch, IfRange, IfUnmodifiedSince,
};
use shiguredo_http11::date::HttpDate;
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
        IfModifiedSince::parse("invalid date", 2026),
        Err(ConditionalError::DateError)
    ));
    assert!(matches!(
        IfModifiedSince::parse("2024-01-01", 2026),
        Err(ConditionalError::DateError)
    ));
}

// ========================================
// IfUnmodifiedSince のテスト
// ========================================

#[test]
fn test_if_unmodified_since_parse_errors() {
    assert!(matches!(
        IfUnmodifiedSince::parse("invalid date", 2026),
        Err(ConditionalError::DateError)
    ));
}

// ========================================
// IfRange のテスト
// ========================================

#[test]
fn test_if_range_parse_errors() {
    // 空
    assert!(matches!(
        IfRange::parse("", 2026),
        Err(ConditionalError::Empty)
    ));
    assert!(matches!(
        IfRange::parse("   ", 2026),
        Err(ConditionalError::Empty)
    ));

    // 不正な形式
    assert!(matches!(
        IfRange::parse("invalid", 2026),
        Err(ConditionalError::DateError)
    ));
}

// ========================================
// src/conditional.rs のインラインテストを移動
// ========================================

#[test]
fn test_if_match_single() {
    let im = IfMatch::parse("\"abc\"").unwrap();
    let etag = EntityTag::strong("abc").unwrap();
    assert!(im.matches(&etag));

    let other = EntityTag::strong("xyz").unwrap();
    assert!(!im.matches(&other));
}

#[test]
fn test_if_match_multiple() {
    let im = IfMatch::parse("\"a\", \"b\", \"c\"").unwrap();
    assert!(im.matches(&EntityTag::strong("b").unwrap()));
    assert!(!im.matches(&EntityTag::strong("d").unwrap()));
}

#[test]
fn test_if_match_any() {
    let im = IfMatch::parse("*").unwrap();
    assert!(im.is_any());
    assert!(im.matches(&EntityTag::strong("anything").unwrap()));
}

#[test]
fn test_if_match_weak_not_match() {
    // If-Match は Strong 比較を使用するため、Weak ETag は一致しない
    let im = IfMatch::parse("W/\"abc\"").unwrap();
    let etag = EntityTag::strong("abc").unwrap();
    assert!(!im.matches(&etag));
}

#[test]
fn test_if_none_match_single() {
    let inm = IfNoneMatch::parse("\"abc\"").unwrap();
    let etag = EntityTag::strong("abc").unwrap();
    assert!(!inm.matches(&etag)); // 一致するので処理しない

    let other = EntityTag::strong("xyz").unwrap();
    assert!(inm.matches(&other)); // 一致しないので処理する
}

#[test]
fn test_if_none_match_any() {
    let inm = IfNoneMatch::parse("*").unwrap();
    assert!(inm.is_any());
    // * は全てに一致するので、どの ETag でも処理しない
    assert!(!inm.matches(&EntityTag::strong("anything").unwrap()));
}

#[test]
fn test_if_none_match_weak() {
    // If-None-Match は Weak 比較を使用
    let inm = IfNoneMatch::parse("W/\"abc\"").unwrap();
    let etag = EntityTag::strong("abc").unwrap();
    assert!(!inm.matches(&etag)); // Weak 比較で一致するので処理しない
}

#[test]
fn test_if_modified_since() {
    let ims = IfModifiedSince::parse("Sun, 06 Nov 1994 08:49:37 GMT", 2026).unwrap();
    assert_eq!(ims.date().day(), 6);
    assert_eq!(ims.date().month(), 11);
    assert_eq!(ims.date().year(), 1994);
}

#[test]
fn test_if_modified_since_is_modified() {
    // If-Modified-Since: 1994-11-06
    let ims = IfModifiedSince::parse("Sun, 06 Nov 1994 08:49:37 GMT", 2026).unwrap();

    // last-modified が同じ → false (304)
    let same = HttpDate::parse("Sun, 06 Nov 1994 08:49:37 GMT").unwrap();
    assert!(!ims.is_modified(&same));

    // last-modified が古い → false (304)
    let older = HttpDate::parse("Sat, 05 Nov 1994 08:49:37 GMT").unwrap();
    assert!(!ims.is_modified(&older));

    // last-modified が新しい → true (処理する)
    let newer = HttpDate::parse("Mon, 07 Nov 1994 08:49:37 GMT").unwrap();
    assert!(ims.is_modified(&newer));
}

#[test]
fn test_if_unmodified_since() {
    let ius = IfUnmodifiedSince::parse("Sun, 06 Nov 1994 08:49:37 GMT", 2026).unwrap();
    assert_eq!(ius.date().day(), 6);
}

#[test]
fn test_if_range_etag() {
    let ir = IfRange::parse("\"abc123\"", 2026).unwrap();
    assert!(ir.is_etag());
    assert_eq!(ir.etag().unwrap().tag(), "abc123");
}

#[test]
fn test_if_range_weak_etag() {
    let ir = IfRange::parse("W/\"abc123\"", 2026).unwrap();
    assert!(ir.is_etag());
    assert!(ir.etag().unwrap().is_weak());
}

#[test]
fn test_if_range_date() {
    let ir = IfRange::parse("Sun, 06 Nov 1994 08:49:37 GMT", 2026).unwrap();
    assert!(ir.is_date());
    assert_eq!(ir.date().unwrap().day(), 6);
}

#[test]
fn test_if_match_display() {
    let im = IfMatch::parse("\"a\", \"b\"").unwrap();
    assert_eq!(im.to_string(), "\"a\", \"b\"");
}

#[test]
fn test_if_range_display() {
    let ir = IfRange::parse("\"abc\"", 2026).unwrap();
    assert_eq!(ir.to_string(), "\"abc\"");
}
