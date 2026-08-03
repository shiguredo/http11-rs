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
    let im = IfMatch::parse("*").expect("条件付きリクエストのパースは成功するはず (実装バグ)");
    assert!(im.is_any());
    assert!(
        im.matches(
            &EntityTag::strong("anything")
                .expect("条件付きリクエストのパースは成功するはず (実装バグ)")
        )
    );
    assert_eq!(im.to_string(), "*");
}

// ========================================
// IfNoneMatch のテスト
// ========================================

#[test]
fn test_if_none_match_wildcard() {
    let inm = IfNoneMatch::parse("*").expect("条件付きリクエストのパースは成功するはず (実装バグ)");
    assert!(inm.is_any());
    // * は全てに一致するので、どの ETag でも処理しない
    assert!(
        !inm.matches(
            &EntityTag::strong("anything")
                .expect("条件付きリクエストのパースは成功するはず (実装バグ)")
        )
    );
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
    let im =
        IfMatch::parse("\"abc\"").expect("条件付きリクエストのパースは成功するはず (実装バグ)");
    let etag =
        EntityTag::strong("abc").expect("条件付きリクエストのパースは成功するはず (実装バグ)");
    assert!(im.matches(&etag));

    let other =
        EntityTag::strong("xyz").expect("条件付きリクエストのパースは成功するはず (実装バグ)");
    assert!(!im.matches(&other));
}

#[test]
fn test_if_match_multiple() {
    let im = IfMatch::parse("\"a\", \"b\", \"c\"")
        .expect("条件付きリクエストのパースは成功するはず (実装バグ)");
    assert!(im.matches(
        &EntityTag::strong("b").expect("条件付きリクエストのパースは成功するはず (実装バグ)")
    ));
    assert!(!im.matches(
        &EntityTag::strong("d").expect("条件付きリクエストのパースは成功するはず (実装バグ)")
    ));
}

#[test]
fn test_if_match_any() {
    let im = IfMatch::parse("*").expect("条件付きリクエストのパースは成功するはず (実装バグ)");
    assert!(im.is_any());
    assert!(
        im.matches(
            &EntityTag::strong("anything")
                .expect("条件付きリクエストのパースは成功するはず (実装バグ)")
        )
    );
}

#[test]
fn test_if_match_weak_not_match() {
    // If-Match は Strong 比較を使用するため、Weak ETag は一致しない
    let im =
        IfMatch::parse("W/\"abc\"").expect("条件付きリクエストのパースは成功するはず (実装バグ)");
    let etag =
        EntityTag::strong("abc").expect("条件付きリクエストのパースは成功するはず (実装バグ)");
    assert!(!im.matches(&etag));
}

#[test]
fn test_if_none_match_single() {
    let inm =
        IfNoneMatch::parse("\"abc\"").expect("条件付きリクエストのパースは成功するはず (実装バグ)");
    let etag =
        EntityTag::strong("abc").expect("条件付きリクエストのパースは成功するはず (実装バグ)");
    assert!(!inm.matches(&etag)); // 一致するので処理しない

    let other =
        EntityTag::strong("xyz").expect("条件付きリクエストのパースは成功するはず (実装バグ)");
    assert!(inm.matches(&other)); // 一致しないので処理する
}

#[test]
fn test_if_none_match_any() {
    let inm = IfNoneMatch::parse("*").expect("条件付きリクエストのパースは成功するはず (実装バグ)");
    assert!(inm.is_any());
    // * は全てに一致するので、どの ETag でも処理しない
    assert!(
        !inm.matches(
            &EntityTag::strong("anything")
                .expect("条件付きリクエストのパースは成功するはず (実装バグ)")
        )
    );
}

#[test]
fn test_if_none_match_weak() {
    // If-None-Match は Weak 比較を使用
    let inm = IfNoneMatch::parse("W/\"abc\"")
        .expect("条件付きリクエストのパースは成功するはず (実装バグ)");
    let etag =
        EntityTag::strong("abc").expect("条件付きリクエストのパースは成功するはず (実装バグ)");
    assert!(!inm.matches(&etag)); // Weak 比較で一致するので処理しない
}

#[test]
fn test_if_modified_since() {
    let ims = IfModifiedSince::parse("Sun, 06 Nov 1994 08:49:37 GMT", 2026)
        .expect("条件付きリクエストのパースは成功するはず (実装バグ)");
    assert_eq!(ims.date().day(), 6);
    assert_eq!(ims.date().month(), 11);
    assert_eq!(ims.date().year(), 1994);
}

#[test]
fn test_if_modified_since_is_modified() {
    // If-Modified-Since: 1994-11-06
    let ims = IfModifiedSince::parse("Sun, 06 Nov 1994 08:49:37 GMT", 2026)
        .expect("条件付きリクエストのパースは成功するはず (実装バグ)");

    // last-modified が同じ → false (304)
    let same = HttpDate::parse("Sun, 06 Nov 1994 08:49:37 GMT")
        .expect("条件付きリクエストのパースは成功するはず (実装バグ)");
    assert!(!ims.is_modified(&same));

    // last-modified が古い → false (304)
    let older = HttpDate::parse("Sat, 05 Nov 1994 08:49:37 GMT")
        .expect("条件付きリクエストのパースは成功するはず (実装バグ)");
    assert!(!ims.is_modified(&older));

    // last-modified が新しい → true (処理する)
    let newer = HttpDate::parse("Mon, 07 Nov 1994 08:49:37 GMT")
        .expect("条件付きリクエストのパースは成功するはず (実装バグ)");
    assert!(ims.is_modified(&newer));
}

#[test]
fn test_if_unmodified_since() {
    let ius = IfUnmodifiedSince::parse("Sun, 06 Nov 1994 08:49:37 GMT", 2026)
        .expect("条件付きリクエストのパースは成功するはず (実装バグ)");
    assert_eq!(ius.date().day(), 6);
}

#[test]
fn test_if_range_etag() {
    let ir = IfRange::parse("\"abc123\"", 2026)
        .expect("条件付きリクエストのパースは成功するはず (実装バグ)");
    assert!(ir.is_etag());
    assert_eq!(
        ir.etag()
            .expect("条件付きリクエストのパースは成功するはず (実装バグ)")
            .tag(),
        "abc123"
    );
}

#[test]
fn test_if_range_weak_etag() {
    let ir = IfRange::parse("W/\"abc123\"", 2026)
        .expect("条件付きリクエストのパースは成功するはず (実装バグ)");
    assert!(ir.is_etag());
    assert!(
        ir.etag()
            .expect("条件付きリクエストのパースは成功するはず (実装バグ)")
            .is_weak()
    );
}

#[test]
fn test_if_range_date() {
    let ir = IfRange::parse("Sun, 06 Nov 1994 08:49:37 GMT", 2026)
        .expect("条件付きリクエストのパースは成功するはず (実装バグ)");
    assert!(ir.is_date());
    assert_eq!(
        ir.date()
            .expect("条件付きリクエストのパースは成功するはず (実装バグ)")
            .day(),
        6
    );
}

#[test]
fn test_if_match_display() {
    let im = IfMatch::parse("\"a\", \"b\"")
        .expect("条件付きリクエストのパースは成功するはず (実装バグ)");
    assert_eq!(im.to_string(), "\"a\", \"b\"");
}

#[test]
fn test_if_range_display() {
    let ir = IfRange::parse("\"abc\"", 2026)
        .expect("条件付きリクエストのパースは成功するはず (実装バグ)");
    assert_eq!(ir.to_string(), "\"abc\"");
}
