//! Cookie のユニットテスト

use shiguredo_http11::cookie::{Cookie, CookieError, SameSite, SetCookie};
use shiguredo_http11::date::HttpDate;

// ========================================
// CookieError のテスト
// ========================================

#[test]
fn test_cookie_error_display() {
    let errors = [
        (CookieError::Empty, "empty cookie"),
        (CookieError::InvalidFormat, "invalid cookie format"),
        (CookieError::InvalidName, "invalid cookie name"),
        (CookieError::InvalidValue, "invalid cookie value"),
        (CookieError::InvalidAttribute, "invalid cookie attribute"),
        (CookieError::InvalidSameSite, "invalid SameSite attribute"),
    ];

    for (error, expected) in errors {
        assert_eq!(error.to_string(), expected);
    }
}

// ========================================
// Expires 属性のテスト
// ========================================

#[test]
fn test_set_cookie_with_expires() {
    // 有効な日付で Expires をテスト
    let date = HttpDate::parse("Sun, 06 Nov 1994 08:49:37 GMT").unwrap();
    let cookie = SetCookie::new("session", "abc123")
        .unwrap()
        .with_expires(date.clone());

    assert_eq!(cookie.expires(), Some(&date));

    let displayed = cookie.to_string();
    assert!(displayed.contains("Expires="));

    let reparsed = SetCookie::parse(&displayed).unwrap();
    assert_eq!(reparsed.expires(), Some(&date));
}

#[test]
fn test_set_cookie_expires_roundtrip() {
    let input = "session=abc123; Expires=Sun, 06 Nov 1994 08:49:37 GMT";
    let cookie = SetCookie::parse(input).unwrap();

    assert!(cookie.expires().is_some());

    let displayed = cookie.to_string();
    let reparsed = SetCookie::parse(&displayed).unwrap();

    assert_eq!(cookie.expires(), reparsed.expires());
}

// ========================================
// 引用符付き値のテスト
// ========================================

#[test]
fn test_cookie_quoted_value() {
    // 引用符付きの値 (cookie-octet のみ)
    let input = "name=\"quotedvalue\"";
    let cookies = Cookie::parse(input).unwrap();

    assert_eq!(cookies.len(), 1);
    assert_eq!(cookies[0].name(), "name");
    assert_eq!(cookies[0].value(), "quotedvalue");
}

#[test]
fn test_cookie_quoted_value_with_space_rejected() {
    // RFC 6265 Section 4.1.1: スペースは cookie-octet ではない
    let input = "name=\"quoted value\"";
    assert!(matches!(
        Cookie::parse(input),
        Err(CookieError::InvalidValue)
    ));
}

#[test]
fn test_set_cookie_quoted_value() {
    // 引用符付きの値 (cookie-octet のみ)
    let input = "name=\"quotedvalue\"; Path=/";
    let cookie = SetCookie::parse(input).unwrap();

    assert_eq!(cookie.name(), "name");
    assert_eq!(cookie.value(), "quotedvalue");
    assert_eq!(cookie.path(), Some("/"));
}

#[test]
fn test_set_cookie_quoted_value_with_space_rejected() {
    // RFC 6265 Section 4.1.1: スペースは cookie-octet ではない
    let input = "name=\"quoted value\"; Path=/";
    assert!(matches!(
        SetCookie::parse(input),
        Err(CookieError::InvalidValue)
    ));
}

// ========================================
// エラーケースのテスト
// ========================================

#[test]
fn test_cookie_parse_errors() {
    // 空
    assert!(matches!(Cookie::parse(""), Err(CookieError::Empty)));
    assert!(matches!(Cookie::parse("   "), Err(CookieError::Empty)));

    // = がない
    assert!(matches!(
        Cookie::parse("invalidcookie"),
        Err(CookieError::InvalidFormat)
    ));

    // 空の名前
    assert!(matches!(
        Cookie::parse("=value"),
        Err(CookieError::InvalidName)
    ));

    // 不正な名前 (スペースを含む)
    assert!(matches!(
        Cookie::parse("bad name=value"),
        Err(CookieError::InvalidName)
    ));
}

#[test]
fn test_set_cookie_parse_errors() {
    // 空
    assert!(matches!(SetCookie::parse(""), Err(CookieError::Empty)));

    // = がない
    assert!(matches!(
        SetCookie::parse("invalidcookie"),
        Err(CookieError::InvalidFormat)
    ));

    // RFC 6265 Section 5.2.2: 不正な Max-Age は無視される (エラーにならない)
    let cookie = SetCookie::parse("name=value; Max-Age=notanumber").unwrap();
    assert!(cookie.max_age().is_none());

    // 不正な SameSite
    assert!(matches!(
        SetCookie::parse("name=value; SameSite=Invalid"),
        Err(CookieError::InvalidSameSite)
    ));

    // RFC 6265 Section 5.2.1: 不正な Expires は無視される (エラーにならない)
    let cookie = SetCookie::parse("name=value; Expires=not a date").unwrap();
    assert!(cookie.expires().is_none());
}

// ========================================
// 空パートのテスト
// ========================================

#[test]
fn test_cookie_empty_part() {
    // セミコロンの後に空白のみ
    let cookies = Cookie::parse("name=value; ").unwrap();
    assert_eq!(cookies.len(), 1);

    // 連続するセミコロン
    let cookies = Cookie::parse("name=value;;other=val").unwrap();
    assert_eq!(cookies.len(), 2);
}

#[test]
fn test_set_cookie_empty_part() {
    // セミコロンの後に空白のみ
    let cookie = SetCookie::parse("name=value; ").unwrap();
    assert_eq!(cookie.name(), "name");

    // 連続するセミコロン (空パートは無視)
    let cookie = SetCookie::parse("name=value;; Secure").unwrap();
    assert!(cookie.secure());
}

// ========================================
// 未知の属性のテスト
// ========================================

#[test]
fn test_set_cookie_unknown_attribute() {
    // 未知の属性は無視される
    let cookie = SetCookie::parse("name=value; UnknownAttr=something; Secure").unwrap();
    assert_eq!(cookie.name(), "name");
    assert!(cookie.secure());

    // 値なしの未知の属性
    let cookie = SetCookie::parse("name=value; UnknownFlag; HttpOnly").unwrap();
    assert!(cookie.http_only());
}

// ========================================
// SameSite のデフォルトのテスト
// ========================================

#[test]
fn test_same_site_default() {
    // SameSite のデフォルトは Lax
    let default = SameSite::default();
    assert_eq!(default, SameSite::Lax);
}

// ========================================
// Cookie::new / SetCookie::new のエラーテスト
// ========================================

#[test]
fn test_cookie_new_invalid_name() {
    // 空の名前
    assert!(matches!(
        Cookie::new("", "value"),
        Err(CookieError::InvalidName)
    ));

    // 不正な名前 (スペースを含む)
    assert!(matches!(
        Cookie::new("bad name", "value"),
        Err(CookieError::InvalidName)
    ));

    // 不正な名前 (制御文字を含む)
    assert!(matches!(
        Cookie::new("bad\tname", "value"),
        Err(CookieError::InvalidName)
    ));
}

#[test]
fn test_cookie_new_invalid_value() {
    // 不正な値 (制御文字を含む)
    assert!(matches!(
        Cookie::new("name", "bad\x00value"),
        Err(CookieError::InvalidValue)
    ));
}

#[test]
fn test_set_cookie_new_invalid_name() {
    // 空の名前
    assert!(matches!(
        SetCookie::new("", "value"),
        Err(CookieError::InvalidName)
    ));

    // 不正な名前
    assert!(matches!(
        SetCookie::new("bad name", "value"),
        Err(CookieError::InvalidName)
    ));
}

#[test]
fn test_set_cookie_new_invalid_value() {
    // 不正な値
    assert!(matches!(
        SetCookie::new("name", "bad\x00value"),
        Err(CookieError::InvalidValue)
    ));
}

// ========================================
// 空の Cookie リストのテスト
// ========================================

#[test]
fn test_cookie_parse_only_semicolons() {
    // セミコロンのみ（Cookie が 0 個になるケース）
    assert!(matches!(Cookie::parse(";;;"), Err(CookieError::Empty)));
}
