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

// ========================================
// src/cache.rs のインラインテストを移動 (PBT重複分を除く)
// ========================================

#[test]
fn test_cache_control_parse_max_age() {
    let cc = CacheControl::parse("max-age=3600").unwrap();
    assert_eq!(cc.max_age(), Some(3600));
}

#[test]
fn test_cache_control_parse_multiple() {
    let cc = CacheControl::parse("max-age=3600, public, no-transform").unwrap();
    assert_eq!(cc.max_age(), Some(3600));
    assert!(cc.is_public());
    assert!(cc.is_no_transform());
}

#[test]
fn test_cache_control_parse_no_store() {
    let cc = CacheControl::parse("no-store").unwrap();
    assert!(cc.is_no_store());
    assert!(!cc.is_cacheable());
}

#[test]
fn test_cache_control_parse_private() {
    let cc = CacheControl::parse("private, max-age=600").unwrap();
    assert!(cc.is_private());
    assert_eq!(cc.max_age(), Some(600));
}

#[test]
fn test_cache_control_parse_no_cache_qualified() {
    // RFC 9111 Section 5.2.2.4: no-cache の修飾形式
    let cc = CacheControl::parse("no-cache=\"Set-Cookie\"").unwrap();
    assert!(cc.is_no_cache());
}

#[test]
fn test_cache_control_parse_private_qualified() {
    // RFC 9111 Section 5.2.2.7: private の修飾形式
    let cc = CacheControl::parse("private=\"Content-Type\"").unwrap();
    assert!(cc.is_private());
}

#[test]
fn test_cache_control_parse_s_maxage() {
    let cc = CacheControl::parse("public, s-maxage=86400").unwrap();
    assert!(cc.is_public());
    assert_eq!(cc.s_maxage(), Some(86400));
}

#[test]
fn test_cache_control_parse_must_revalidate() {
    let cc = CacheControl::parse("max-age=0, must-revalidate").unwrap();
    assert_eq!(cc.max_age(), Some(0));
    assert!(cc.is_must_revalidate());
}

#[test]
fn test_cache_control_parse_immutable() {
    let cc = CacheControl::parse("max-age=31536000, immutable").unwrap();
    assert_eq!(cc.max_age(), Some(31536000));
    assert!(cc.is_immutable());
}

#[test]
fn test_cache_control_parse_empty() {
    // 空文字列はデフォルトの CacheControl として扱う
    let cc = CacheControl::parse("").unwrap();
    assert_eq!(cc, CacheControl::default());
    assert_eq!(cc.max_age(), None);
    assert!(!cc.is_public());
}

#[test]
fn test_cache_control_builder() {
    let cc = CacheControl::new()
        .with_max_age(3600)
        .with_public()
        .with_no_transform();

    assert_eq!(cc.max_age(), Some(3600));
    assert!(cc.is_public());
    assert!(cc.is_no_transform());
}

#[test]
fn test_cache_control_display() {
    let cc = CacheControl::new().with_max_age(3600).with_public();

    let s = cc.to_string();
    assert!(s.contains("max-age=3600"));
    assert!(s.contains("public"));
}

#[test]
fn test_cache_control_is_cacheable() {
    let cc1 = CacheControl::new().with_max_age(3600);
    assert!(cc1.is_cacheable());

    let cc2 = CacheControl::new().with_no_store();
    assert!(!cc2.is_cacheable());

    let cc3 = CacheControl::new().with_public();
    assert!(cc3.is_cacheable());
}

#[test]
fn test_age_parse() {
    let age = Age::parse("120").unwrap();
    assert_eq!(age.seconds(), 120);
}

#[test]
fn test_age_parse_zero() {
    let age = Age::parse("0").unwrap();
    assert_eq!(age.seconds(), 0);
}

#[test]
fn test_age_parse_empty() {
    assert!(Age::parse("").is_err());
}

#[test]
fn test_age_parse_invalid() {
    assert!(Age::parse("abc").is_err());
    assert!(Age::parse("-1").is_err());
    // RFC 9111 Section 1.2.2: 1*DIGIT 制約 (+ は DIGIT ではない)
    assert!(Age::parse("+1").is_err());
}

/// RFC 9111 Section 1.2.2: オーバーフロー時は 2^31 にクランプする
#[test]
fn test_age_parse_overflow_clamp() {
    let age = Age::parse("99999999999999999999999").unwrap();
    assert_eq!(age.seconds(), 2_147_483_648);
}

#[test]
fn test_cache_control_delta_seconds_overflow() {
    let cc = CacheControl::parse("max-age=99999999999999999999999").unwrap();
    assert_eq!(cc.max_age(), Some(2_147_483_648));
}

/// RFC 9111 Section 1.2.2: delta-seconds は 1*DIGIT のみ
#[test]
fn test_cache_control_delta_seconds_plus_sign() {
    assert!(CacheControl::parse("max-age=+3600").is_err());
}

#[test]
fn test_age_display() {
    let age = Age::new(120);
    assert_eq!(age.to_string(), "120");
}

#[test]
fn test_expires_parse() {
    let expires = Expires::parse("Sun, 06 Nov 1994 08:49:37 GMT", 2026).unwrap();
    assert_eq!(expires.date().year(), 1994);
    assert_eq!(expires.date().month(), 11);
    assert_eq!(expires.date().day(), 6);
}

#[test]
fn test_expires_parse_invalid() {
    assert!(Expires::parse("invalid date", 2026).is_err());
    assert!(Expires::parse("", 2026).is_err());
}

#[test]
fn test_expires_display() {
    let expires = Expires::parse("Sun, 06 Nov 1994 08:49:37 GMT", 2026).unwrap();
    let s = expires.to_string();
    assert!(s.contains("1994"));
    assert!(s.contains("Nov"));
}
