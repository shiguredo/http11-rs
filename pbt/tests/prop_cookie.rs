//! Cookie のプロパティテスト (cookie.rs)

use proptest::prelude::*;
use shiguredo_http11::cookie::{Cookie, CookieError, SameSite, SetCookie};
use shiguredo_http11::date::HttpDate;

// ========================================
// CookieError のテスト
// ========================================

#[test]
fn prop_cookie_error_display() {
    let errors = [
        (CookieError::Empty, "empty cookie"),
        (CookieError::InvalidFormat, "invalid cookie format"),
        (CookieError::InvalidName, "invalid cookie name"),
        (CookieError::InvalidValue, "invalid cookie value"),
        (CookieError::InvalidAttribute, "invalid cookie attribute"),
        (CookieError::InvalidExpires, "invalid Expires attribute"),
        (CookieError::InvalidMaxAge, "invalid Max-Age attribute"),
        (CookieError::InvalidSameSite, "invalid SameSite attribute"),
    ];

    for (error, expected) in errors {
        assert_eq!(error.to_string(), expected);
    }
}

#[test]
fn prop_cookie_error_is_error_trait() {
    let error: Box<dyn std::error::Error> = Box::new(CookieError::Empty);
    assert_eq!(error.to_string(), "empty cookie");
}

#[test]
fn prop_cookie_error_clone_eq() {
    let error = CookieError::InvalidName;
    let cloned = error.clone();
    assert_eq!(error, cloned);
}

// ========================================
// Cookie パースのテスト
// ========================================

// Cookie のラウンドトリップ
proptest! {
    #[test]
    fn prop_cookie_roundtrip(name in "[a-zA-Z][a-zA-Z0-9_-]{0,15}", value in "[a-zA-Z0-9_-]{0,32}") {
        let cookie = Cookie::new(&name, &value).unwrap();
        let displayed = cookie.to_string();
        let cookies = Cookie::parse(&displayed).unwrap();
        prop_assert_eq!(cookies.len(), 1);
        prop_assert_eq!(cookies[0].name(), name.as_str());
        prop_assert_eq!(cookies[0].value(), value.as_str());
    }
}

// 複数 Cookie のパース
proptest! {
    #[test]
    fn prop_cookie_parse_multiple(name1 in "[a-zA-Z][a-zA-Z0-9]{0,7}", value1 in "[a-zA-Z0-9]{0,16}", name2 in "[a-zA-Z][a-zA-Z0-9]{0,7}", value2 in "[a-zA-Z0-9]{0,16}") {
        let cookie_str = format!("{}={}; {}={}", name1, value1, name2, value2);
        let cookies = Cookie::parse(&cookie_str).unwrap();
        prop_assert_eq!(cookies.len(), 2);
        prop_assert_eq!(cookies[0].name(), name1.as_str());
        prop_assert_eq!(cookies[0].value(), value1.as_str());
        prop_assert_eq!(cookies[1].name(), name2.as_str());
        prop_assert_eq!(cookies[1].value(), value2.as_str());
    }
}

// SetCookie のラウンドトリップ
proptest! {
    #[test]
    fn prop_set_cookie_roundtrip(name in "[a-zA-Z][a-zA-Z0-9_-]{0,15}", value in "[a-zA-Z0-9_-]{0,32}") {
        let cookie = SetCookie::new(&name, &value).unwrap();
        let displayed = cookie.to_string();
        let reparsed = SetCookie::parse(&displayed).unwrap();
        prop_assert_eq!(reparsed.name(), name.as_str());
        prop_assert_eq!(reparsed.value(), value.as_str());
    }
}

// SetCookie 属性付きラウンドトリップ
proptest! {
    #[test]
    fn prop_set_cookie_with_attributes(name in "[a-zA-Z][a-zA-Z0-9]{0,7}", value in "[a-zA-Z0-9]{0,16}", path in "/[a-zA-Z0-9_-]{0,16}", max_age in 0i64..=86400) {
        let cookie = SetCookie::new(&name, &value)
            .unwrap()
            .with_path(&path)
            .with_max_age(max_age)
            .with_secure(true)
            .with_http_only(true);

        let displayed = cookie.to_string();
        let reparsed = SetCookie::parse(&displayed).unwrap();

        prop_assert_eq!(reparsed.name(), name.as_str());
        prop_assert_eq!(reparsed.value(), value.as_str());
        prop_assert_eq!(reparsed.path(), Some(path.as_str()));
        prop_assert_eq!(reparsed.max_age(), Some(max_age));
        prop_assert!(reparsed.secure());
        prop_assert!(reparsed.http_only());
    }
}

// SameSite 属性ラウンドトリップ
proptest! {
    #[test]
    fn prop_set_cookie_same_site(name in "[a-zA-Z][a-zA-Z0-9]{0,7}", value in "[a-zA-Z0-9]{0,16}", same_site in prop_oneof![Just(SameSite::Strict), Just(SameSite::Lax), Just(SameSite::None)]) {
        let cookie = SetCookie::new(&name, &value)
            .unwrap()
            .with_same_site(same_site);

        let displayed = cookie.to_string();
        let reparsed = SetCookie::parse(&displayed).unwrap();

        prop_assert_eq!(reparsed.same_site(), Some(same_site));
    }
}

// 任意の文字列で Cookie パースがパニックしない
proptest! {
    #[test]
    fn prop_cookie_parse_no_panic(s in "[ -~]{0,64}") {
        let _ = Cookie::parse(&s);
    }
}

// 任意の文字列で SetCookie パースがパニックしない
proptest! {
    #[test]
    fn prop_set_cookie_parse_no_panic(s in "[ -~]{0,64}") {
        let _ = SetCookie::parse(&s);
    }
}

// Domain 属性付き SetCookie
proptest! {
    #[test]
    fn prop_set_cookie_with_domain(name in "[a-zA-Z][a-zA-Z0-9]{0,7}", value in "[a-zA-Z0-9]{0,16}", domain in "[a-z]{1,8}\\.[a-z]{2,4}") {
        let cookie = SetCookie::new(&name, &value).unwrap().with_domain(&domain);

        let displayed = cookie.to_string();
        let reparsed = SetCookie::parse(&displayed).unwrap();

        prop_assert_eq!(reparsed.domain(), Some(domain.as_str()));
    }
}

// ========================================
// Expires 属性のテスト
// ========================================

#[test]
fn prop_set_cookie_with_expires() {
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
fn prop_set_cookie_expires_roundtrip() {
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
fn prop_cookie_quoted_value() {
    // 引用符付きの値
    let input = "name=\"quoted value\"";
    let cookies = Cookie::parse(input).unwrap();

    assert_eq!(cookies.len(), 1);
    assert_eq!(cookies[0].name(), "name");
    assert_eq!(cookies[0].value(), "quoted value");
}

#[test]
fn prop_set_cookie_quoted_value() {
    // 引用符付きの値
    let input = "name=\"quoted value\"; Path=/";
    let cookie = SetCookie::parse(input).unwrap();

    assert_eq!(cookie.name(), "name");
    assert_eq!(cookie.value(), "quoted value");
    assert_eq!(cookie.path(), Some("/"));
}

// ========================================
// エラーケースのテスト
// ========================================

#[test]
fn prop_cookie_parse_errors() {
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
fn prop_set_cookie_parse_errors() {
    // 空
    assert!(matches!(SetCookie::parse(""), Err(CookieError::Empty)));

    // = がない
    assert!(matches!(
        SetCookie::parse("invalidcookie"),
        Err(CookieError::InvalidFormat)
    ));

    // 不正な Max-Age
    assert!(matches!(
        SetCookie::parse("name=value; Max-Age=notanumber"),
        Err(CookieError::InvalidMaxAge)
    ));

    // 不正な SameSite
    assert!(matches!(
        SetCookie::parse("name=value; SameSite=Invalid"),
        Err(CookieError::InvalidSameSite)
    ));

    // 不正な Expires
    assert!(matches!(
        SetCookie::parse("name=value; Expires=not a date"),
        Err(CookieError::InvalidExpires)
    ));
}

// ========================================
// 空パートのテスト
// ========================================

#[test]
fn prop_cookie_empty_part() {
    // セミコロンの後に空白のみ
    let cookies = Cookie::parse("name=value; ").unwrap();
    assert_eq!(cookies.len(), 1);

    // 連続するセミコロン
    let cookies = Cookie::parse("name=value;;other=val").unwrap();
    assert_eq!(cookies.len(), 2);
}

#[test]
fn prop_set_cookie_empty_part() {
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
fn prop_set_cookie_unknown_attribute() {
    // 未知の属性は無視される
    let cookie = SetCookie::parse("name=value; UnknownAttr=something; Secure").unwrap();
    assert_eq!(cookie.name(), "name");
    assert!(cookie.secure());

    // 値なしの未知の属性
    let cookie = SetCookie::parse("name=value; UnknownFlag; HttpOnly").unwrap();
    assert!(cookie.http_only());
}

// ========================================
// Clone と PartialEq のテスト
// ========================================

#[test]
fn prop_cookie_clone_eq() {
    let cookie = Cookie::new("name", "value").unwrap();
    let cloned = cookie.clone();
    assert_eq!(cookie, cloned);
}

#[test]
fn prop_set_cookie_clone_eq() {
    let cookie = SetCookie::new("name", "value")
        .unwrap()
        .with_path("/")
        .with_secure(true);
    let cloned = cookie.clone();
    assert_eq!(cookie, cloned);
}

#[test]
fn prop_same_site_default() {
    // SameSite のデフォルトは Lax
    let default = SameSite::default();
    assert_eq!(default, SameSite::Lax);
}

// ========================================
// Cookie::new / SetCookie::new のエラーテスト
// ========================================

#[test]
fn prop_cookie_new_invalid_name() {
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
fn prop_cookie_new_invalid_value() {
    // 不正な値 (制御文字を含む)
    assert!(matches!(
        Cookie::new("name", "bad\x00value"),
        Err(CookieError::InvalidValue)
    ));
}

#[test]
fn prop_set_cookie_new_invalid_name() {
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
fn prop_set_cookie_new_invalid_value() {
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
fn prop_cookie_parse_only_semicolons() {
    // セミコロンのみ（Cookie が 0 個になるケース）
    assert!(matches!(Cookie::parse(";;;"), Err(CookieError::Empty)));
}
