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

    let reparsed = SetCookie::parse(&displayed, 2026).unwrap();
    assert_eq!(reparsed.expires(), Some(&date));
}

#[test]
fn test_set_cookie_expires_roundtrip() {
    let input = "session=abc123; Expires=Sun, 06 Nov 1994 08:49:37 GMT";
    let cookie = SetCookie::parse(input, 2026).unwrap();

    assert!(cookie.expires().is_some());

    let displayed = cookie.to_string();
    let reparsed = SetCookie::parse(&displayed, 2026).unwrap();

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
    let cookie = SetCookie::parse(input, 2026).unwrap();

    assert_eq!(cookie.name(), "name");
    assert_eq!(cookie.value(), "quotedvalue");
    assert_eq!(cookie.path(), Some("/"));
}

#[test]
fn test_set_cookie_quoted_value_with_space_rejected() {
    // RFC 6265 Section 4.1.1: スペースは cookie-octet ではない
    let input = "name=\"quoted value\"; Path=/";
    assert!(matches!(
        SetCookie::parse(input, 2026),
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
    assert!(matches!(
        SetCookie::parse("", 2026),
        Err(CookieError::Empty)
    ));

    // = がない
    assert!(matches!(
        SetCookie::parse("invalidcookie", 2026),
        Err(CookieError::InvalidFormat)
    ));

    // RFC 6265 Section 5.2.2: 不正な Max-Age は無視される (エラーにならない)
    let cookie = SetCookie::parse("name=value; Max-Age=notanumber", 2026).unwrap();
    assert!(cookie.max_age().is_none());

    // RFC 6265 Section 5.2.2: 先頭が "+" は DIGIT でも "-" でもないため無視される
    let cookie = SetCookie::parse("name=value; Max-Age=+10", 2026).unwrap();
    assert!(cookie.max_age().is_none());

    // 不正な SameSite
    assert!(matches!(
        SetCookie::parse("name=value; SameSite=Invalid", 2026),
        Err(CookieError::InvalidSameSite)
    ));

    // RFC 6265 Section 5.2.1: 不正な Expires は無視される (エラーにならない)
    let cookie = SetCookie::parse("name=value; Expires=not a date", 2026).unwrap();
    assert!(cookie.expires().is_none());
}

// ========================================
// Set-Cookie Domain 属性の正規化テスト (RFC 6265 Section 5.2.3)
// ========================================

#[test]
fn test_set_cookie_domain_normalization() {
    // 先頭の "." を除去する
    let cookie = SetCookie::parse("name=value; Domain=.example.com", 2026).unwrap();
    assert_eq!(cookie.domain(), Some("example.com"));

    // 小文字に変換する
    let cookie = SetCookie::parse("name=value; Domain=Example.COM", 2026).unwrap();
    assert_eq!(cookie.domain(), Some("example.com"));

    // 先頭の "." 除去と小文字化の両方を適用する
    let cookie = SetCookie::parse("name=value; Domain=.Example.COM", 2026).unwrap();
    assert_eq!(cookie.domain(), Some("example.com"));

    // "." のみの場合は無視する
    let cookie = SetCookie::parse("name=value; Domain=.", 2026).unwrap();
    assert!(cookie.domain().is_none());

    // 空の場合は無視する
    let cookie = SetCookie::parse("name=value; Domain=", 2026).unwrap();
    assert!(cookie.domain().is_none());
}

// ========================================
// Set-Cookie Domain 属性の RFC 1034 subdomain 構文準拠テスト (issue 0067)
// ========================================

#[test]
fn test_set_cookie_domain_multi_leading_dot_rejected() {
    // strip 後の値が "." で始まる (= 元入力に leading dot が 2 つ以上) ケースは
    // 意味のあるホスト名にならないため無視する。
    // parse -> to_string -> parse の fixed-point 性を担保するための strict 化。

    // ".." → strip で "." 残留 → 無視
    let cookie = SetCookie::parse("name=value; Domain=..", 2026).unwrap();
    assert!(cookie.domain().is_none());

    // "..." → strip で ".." 残留 → 無視
    let cookie = SetCookie::parse("name=value; Domain=...", 2026).unwrap();
    assert!(cookie.domain().is_none());

    // "..foo" → strip で ".foo" 残留 → 無視
    let cookie = SetCookie::parse("name=value; Domain=..foo", 2026).unwrap();
    assert!(cookie.domain().is_none());
}

#[test]
fn test_set_cookie_domain_non_ldh_rejected() {
    // RFC 6265 Section 4.1.1 + RFC 1034 Section 3.5: domain-value は LDH (letter/digit/hyphen)
    // と "." のみを許容する。RFC 6265bis Section 6.3 で IDN は punycode (LDH) 必須と規定。

    // 空白を含む → 無視 (".trim()" は edge のみで内部は残る)
    let cookie = SetCookie::parse("name=value; Domain=foo bar", 2026).unwrap();
    assert!(cookie.domain().is_none());

    // NUL を含む → 無視
    let cookie = SetCookie::parse("name=value; Domain=foo\0bar", 2026).unwrap();
    assert!(cookie.domain().is_none());

    // 制御文字を含む → 無視
    let cookie = SetCookie::parse("name=value; Domain=foo\u{6}bar", 2026).unwrap();
    assert!(cookie.domain().is_none());

    // strip 後に non-LDH が出るケース (leading dot の直後に空白) → 無視
    let cookie = SetCookie::parse("name=value; Domain=. foo", 2026).unwrap();
    assert!(cookie.domain().is_none());

    // 非 ASCII (生 UTF-8) → 無視 (IDN は punycode で渡される想定)
    let cookie = SetCookie::parse("name=value; Domain=日本.example", 2026).unwrap();
    assert!(cookie.domain().is_none());
}

#[test]
fn test_set_cookie_domain_intermediate_dot_preserved() {
    // 中間の連続 dot は parser では弾かない (strip 対象は leading のみ)。
    // Display 出力は元値をそのまま吐き、再 parse でも変化しないので roundtrip は閉じる。
    let cookie = SetCookie::parse("name=value; Domain=foo..bar", 2026).unwrap();
    assert_eq!(cookie.domain(), Some("foo..bar"));
    let reparsed = SetCookie::parse(&cookie.to_string(), 2026).unwrap();
    assert_eq!(reparsed.domain(), Some("foo..bar"));
}

#[test]
fn test_set_cookie_domain_trailing_dot_preserved() {
    // trailing dot (FQDN を明示する形式) は LDH+dot のみで構成されるため受理する。
    let cookie = SetCookie::parse("name=value; Domain=foo.bar.", 2026).unwrap();
    assert_eq!(cookie.domain(), Some("foo.bar."));
    let reparsed = SetCookie::parse(&cookie.to_string(), 2026).unwrap();
    assert_eq!(reparsed.domain(), Some("foo.bar."));
}

#[test]
fn test_set_cookie_domain_hyphen_preserved() {
    // hyphen は LDH に含まれるため受理する。RFC 1034/1123 的に leading/trailing hyphen の
    // label は invalid だが、本実装はそこまで踏み込まない (roundtrip は閉じる)。
    let cookie = SetCookie::parse("name=value; Domain=foo-bar.example", 2026).unwrap();
    assert_eq!(cookie.domain(), Some("foo-bar.example"));
    let reparsed = SetCookie::parse(&cookie.to_string(), 2026).unwrap();
    assert_eq!(reparsed.domain(), Some("foo-bar.example"));
}

#[test]
fn test_set_cookie_domain_multi_leading_dot_roundtrip_closed() {
    // issue 0067 の crash 入力の最小再現: Display -> 再 parse で domain が一致する
    // (旧実装では Some(".") → None で不一致だった)。
    let cookie = SetCookie::parse("3=; Domain=..", 2026).unwrap();
    let reparsed = SetCookie::parse(&cookie.to_string(), 2026).unwrap();
    assert_eq!(cookie.domain(), reparsed.domain());
    assert!(cookie.domain().is_none());
}

#[test]
fn test_set_cookie_domain_leading_dot_space_roundtrip_closed() {
    // issue 0067 の派生 crash 入力 (regression-0067-non-ldh) の最小再現:
    // "Domain=. " (leading dot + space + ...) は旧実装で
    // strip 後に leading space が残ったまま store されていたが、
    // Display 出力を再 parse すると attr_value.trim() で space が削られ不一致になる。
    // 本修正で non-LDH 文字を含む domain 値を一律 reject するようにし、roundtrip を閉じる。
    let cookie = SetCookie::parse("2n=; domain=. foo", 2026).unwrap();
    let reparsed = SetCookie::parse(&cookie.to_string(), 2026).unwrap();
    assert_eq!(cookie.domain(), reparsed.domain());
    assert!(cookie.domain().is_none());
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
    let cookie = SetCookie::parse("name=value; ", 2026).unwrap();
    assert_eq!(cookie.name(), "name");

    // 連続するセミコロン (空パートは無視)
    let cookie = SetCookie::parse("name=value;; Secure", 2026).unwrap();
    assert!(cookie.secure());
}

// ========================================
// 未知の属性のテスト
// ========================================

#[test]
fn test_set_cookie_unknown_attribute() {
    // 未知の属性は無視される
    let cookie = SetCookie::parse("name=value; UnknownAttr=something; Secure", 2026).unwrap();
    assert_eq!(cookie.name(), "name");
    assert!(cookie.secure());

    // 値なしの未知の属性
    let cookie = SetCookie::parse("name=value; UnknownFlag; HttpOnly", 2026).unwrap();
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
