//! キャッシュヘッダーのユニットテスト

use shiguredo_http11::cache::{Age, CacheControl, CacheError, Expires};

// ========================================
// CacheError のテスト
// ========================================

#[test]
fn test_cache_error_display() {
    let errors = [
        (CacheError::Empty, "empty cache header"),
        (CacheError::InvalidFormat, "invalid cache header format"),
        (CacheError::InvalidNumber, "invalid number in cache header"),
        (CacheError::InvalidDate, "invalid date in cache header"),
    ];

    for (error, expected) in errors {
        assert_eq!(error.to_string(), expected);
    }
}

// ========================================
// CacheControl のテスト
// ========================================

// max-stale 値なし
#[test]
fn test_cache_control_max_stale_without_value() {
    let cc = CacheControl::parse("max-stale").unwrap();
    assert_eq!(cc.max_stale(), Some(u64::MAX));
}

// パースエラー
#[test]
fn test_cache_control_parse_errors() {
    // 空文字列はデフォルトの CacheControl として扱う
    let cc = CacheControl::parse("").unwrap();
    assert_eq!(cc, CacheControl::default());
    let cc = CacheControl::parse("   ").unwrap();
    assert_eq!(cc, CacheControl::default());

    // 不正な数値
    assert!(matches!(
        CacheControl::parse("max-age=abc"),
        Err(CacheError::InvalidNumber)
    ));
    assert!(matches!(
        CacheControl::parse("max-age=-1"),
        Err(CacheError::InvalidNumber)
    ));
}

// Default trait
#[test]
fn test_cache_control_default() {
    let cc = CacheControl::default();
    assert_eq!(cc.max_age(), None);
    assert!(!cc.is_no_cache());
    assert!(!cc.is_public());
}

// ========================================
// Age のテスト
// ========================================

// Age 0
#[test]
fn test_age_zero() {
    let age = Age::new(0);
    assert_eq!(age.seconds(), 0);
    assert_eq!(age.to_string(), "0");

    let parsed = Age::parse("0").unwrap();
    assert_eq!(parsed.seconds(), 0);
}

// Age パースエラー
#[test]
fn test_age_parse_errors() {
    // 空
    assert!(matches!(Age::parse(""), Err(CacheError::Empty)));
    assert!(matches!(Age::parse("   "), Err(CacheError::Empty)));

    // 不正な数値
    assert!(matches!(Age::parse("abc"), Err(CacheError::InvalidNumber)));
    assert!(matches!(Age::parse("-1"), Err(CacheError::InvalidNumber)));
    assert!(matches!(Age::parse("1.5"), Err(CacheError::InvalidNumber)));
}

// ========================================
// Expires のテスト
// ========================================

// Expires パースエラー
#[test]
fn test_expires_parse_errors() {
    // 不正な日付形式
    assert!(matches!(
        Expires::parse("invalid date", 2026),
        Err(CacheError::InvalidDate)
    ));
    assert!(matches!(
        Expires::parse("2024-01-01", 2026),
        Err(CacheError::InvalidDate)
    ));
}

// Expires to_header_value
#[test]
fn test_expires_to_header_value() {
    let expires = Expires::parse("Sun, 06 Nov 1994 08:49:37 GMT", 2026).unwrap();
    let header = expires.to_header_value();
    assert!(header.contains("1994"));
    assert!(header.contains("Nov"));
}

// ========================================
// quoted-string の対称検査 (RFC 9110 Section 5.6.4)
// ========================================

/// RFC 9110 §5.6.4: quoted-string は両端 DQUOTE。partial quote は reject する
#[test]
fn test_cache_control_rejects_partial_open_quote() {
    // 開き引用符のみ (`max-age="3600`)
    let result = CacheControl::parse("max-age=\"3600");
    assert!(
        result.is_err(),
        "片端 DQUOTE のみの partial quote は reject される想定"
    );
}

#[test]
fn test_cache_control_rejects_partial_close_quote() {
    // 閉じ引用符のみ (`max-age=3600"`)
    let result = CacheControl::parse("max-age=3600\"");
    assert!(
        result.is_err(),
        "片端 DQUOTE のみの partial quote は reject される想定"
    );
}

#[test]
fn test_cache_control_accepts_both_sides_quoted() {
    let cc = CacheControl::parse("max-age=\"3600\"").unwrap();
    assert_eq!(cc.max_age(), Some(3600));
}

#[test]
fn test_cache_control_accepts_unquoted() {
    let cc = CacheControl::parse("max-age=3600").unwrap();
    assert_eq!(cc.max_age(), Some(3600));
}
