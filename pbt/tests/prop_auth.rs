//! HTTP 認証のプロパティテスト (Digest / Bearer / Authorization / AuthChallenge)

use proptest::prelude::*;
use shiguredo_http11::auth::{
    AuthChallenge, AuthError, Authorization, BasicAuth, BearerChallenge, BearerToken, DigestAuth,
    DigestChallenge, ProxyAuthenticate, ProxyAuthorization, WwwAuthenticate,
};

// ========================================
// Strategy 定義
// ========================================

// token68 文字 (A-Z, a-z, 0-9, -, ., _, ~, +, /)
fn token68_char() -> impl Strategy<Value = char> {
    prop_oneof![
        prop::char::range('A', 'Z'),
        prop::char::range('a', 'z'),
        prop::char::range('0', '9'),
        Just('-'),
        Just('.'),
        Just('_'),
        Just('~'),
        Just('+'),
        Just('/'),
    ]
}

fn token68_string(min: usize, max: usize) -> impl Strategy<Value = String> {
    proptest::collection::vec(token68_char(), min..=max)
        .prop_map(|chars| chars.into_iter().collect())
}

// realm / nonce などのパラメータ値
fn param_value() -> impl Strategy<Value = String> {
    "[a-zA-Z0-9._-]{1,32}".prop_map(|s| s)
}

// ========================================
// AuthError のテスト
// ========================================

#[test]
fn prop_auth_error_display() {
    let errors = [
        (AuthError::Empty, "empty authorization header"),
        (AuthError::InvalidFormat, "invalid authorization format"),
        (AuthError::NotBasicScheme, "not basic authentication scheme"),
        (
            AuthError::NotDigestScheme,
            "not digest authentication scheme",
        ),
        (
            AuthError::NotBearerScheme,
            "not bearer authentication scheme",
        ),
        (AuthError::Base64DecodeError, "base64 decode error"),
        (AuthError::Utf8Error, "utf-8 decode error"),
        (AuthError::MissingColon, "missing colon in credentials"),
        (AuthError::InvalidParameter, "invalid auth parameter"),
        (
            AuthError::MissingParameter,
            "missing required auth parameter",
        ),
        (AuthError::InvalidToken, "invalid auth token"),
    ];

    for (error, expected) in errors {
        assert_eq!(error.to_string(), expected);
    }
}

#[test]
fn prop_auth_error_is_error_trait() {
    let error: Box<dyn std::error::Error> = Box::new(AuthError::Empty);
    assert_eq!(error.to_string(), "empty authorization header");
}

// ========================================
// BearerToken のテスト
// ========================================

// BearerToken ラウンドトリップ
proptest! {
    #[test]
    fn prop_bearer_token_roundtrip(token in token68_string(1, 64)) {
        let bearer = BearerToken::parse(&format!("Bearer {}", token)).unwrap();
        let header = bearer.to_header_value();
        let reparsed = BearerToken::parse(&header).unwrap();

        prop_assert_eq!(bearer.token(), reparsed.token());
        prop_assert_eq!(bearer.token(), token.as_str());
    }
}

// BearerToken Display
proptest! {
    #[test]
    fn prop_bearer_token_display(token in token68_string(1, 32)) {
        let bearer = BearerToken::parse(&format!("Bearer {}", token)).unwrap();
        let display = bearer.to_string();

        prop_assert_eq!(display, format!("Bearer {}", token));
    }
}

// BearerToken パースエラー
#[test]
fn prop_bearer_token_parse_errors() {
    // 空
    assert!(matches!(BearerToken::parse(""), Err(AuthError::Empty)));
    // Bearer スキームでない
    assert!(matches!(
        BearerToken::parse("Basic abc"),
        Err(AuthError::NotBearerScheme)
    ));
    // トークンが空
    assert!(matches!(
        BearerToken::parse("Bearer "),
        Err(AuthError::InvalidFormat)
    ));
    // "Bearer" のみ（スペースなし）
    assert!(matches!(
        BearerToken::parse("Bearer"),
        Err(AuthError::InvalidFormat)
    ));
    // 不正な文字を含む
    assert!(matches!(
        BearerToken::parse("Bearer abc def"),
        Err(AuthError::InvalidToken)
    ));
}

// 大文字小文字を区別しない
proptest! {
    #[test]
    fn prop_bearer_token_case_insensitive(token in token68_string(1, 32)) {
        let lower = BearerToken::parse(&format!("bearer {}", token)).unwrap();
        let upper = BearerToken::parse(&format!("Bearer {}", token)).unwrap();

        prop_assert_eq!(lower.token(), upper.token());
    }
}

// 任意の文字列で BearerToken パースがパニックしない
proptest! {
    #[test]
    fn prop_bearer_token_parse_no_panic(s in "[ -~]{0,64}") {
        let _ = BearerToken::parse(&s);
    }
}

// ========================================
// BearerChallenge のテスト
// ========================================

// BearerChallenge ラウンドトリップ
proptest! {
    #[test]
    fn prop_bearer_challenge_roundtrip(realm in param_value(), error in prop_oneof![Just("invalid_token"), Just("invalid_request"), Just("insufficient_scope")]) {
        let header = format!("Bearer realm=\"{}\", error=\"{}\"", realm, error);
        let challenge = BearerChallenge::parse(&header).unwrap();

        prop_assert_eq!(challenge.param("realm"), Some(realm.as_str()));
        prop_assert_eq!(challenge.param("error"), Some(error));
    }
}

// BearerChallenge Display
proptest! {
    #[test]
    fn prop_bearer_challenge_display(realm in param_value()) {
        let header = format!("Bearer realm=\"{}\"", realm);
        let challenge = BearerChallenge::parse(&header).unwrap();
        let display = challenge.to_string();

        prop_assert!(display.starts_with("Bearer "));
    }
}

// 任意の文字列で BearerChallenge パースがパニックしない
proptest! {
    #[test]
    fn prop_bearer_challenge_parse_no_panic(s in "[ -~]{0,64}") {
        let _ = BearerChallenge::parse(&s);
    }
}

// ========================================
// DigestAuth のテスト
// ========================================

// DigestAuth ラウンドトリップ
proptest! {
    #[test]
    fn prop_digest_auth_roundtrip(
        username in param_value(),
        realm in param_value(),
        nonce in param_value(),
        uri in "/[a-z/]{1,16}",
        response in "[a-f0-9]{32}"
    ) {
        let header = format!(
            "Digest username=\"{}\", realm=\"{}\", nonce=\"{}\", uri=\"{}\", response=\"{}\"",
            username, realm, nonce, uri, response
        );
        let auth = DigestAuth::parse(&header).unwrap();

        prop_assert_eq!(auth.username(), Some(username.as_str()));
        prop_assert_eq!(auth.realm(), Some(realm.as_str()));
        prop_assert_eq!(auth.nonce(), Some(nonce.as_str()));
        prop_assert_eq!(auth.uri(), Some(uri.as_str()));
        prop_assert_eq!(auth.response(), Some(response.as_str()));

        // to_header_value で再エンコードできる
        let header_value = auth.to_header_value();
        prop_assert!(header_value.starts_with("Digest "));
    }
}

// DigestAuth Display
proptest! {
    #[test]
    fn prop_digest_auth_display(
        username in param_value(),
        realm in param_value(),
        nonce in param_value(),
        uri in "/[a-z]{1,8}",
        response in "[a-f0-9]{32}"
    ) {
        let header = format!(
            "Digest username=\"{}\", realm=\"{}\", nonce=\"{}\", uri=\"{}\", response=\"{}\"",
            username, realm, nonce, uri, response
        );
        let auth = DigestAuth::parse(&header).unwrap();
        let display = auth.to_string();

        prop_assert!(display.starts_with("Digest "));
    }
}

// DigestAuth パースエラー
#[test]
fn prop_digest_auth_parse_errors() {
    // 空
    assert!(matches!(DigestAuth::parse(""), Err(AuthError::Empty)));
    // Digest スキームでない
    assert!(matches!(
        DigestAuth::parse("Basic abc"),
        Err(AuthError::NotDigestScheme)
    ));
    // 必須パラメータが足りない
    assert!(matches!(
        DigestAuth::parse("Digest username=\"test\""),
        Err(AuthError::MissingParameter)
    ));
}

// param で大文字小文字を区別しない
proptest! {
    #[test]
    fn prop_digest_auth_param_case_insensitive(
        username in param_value(),
        realm in param_value(),
        nonce in param_value(),
        uri in "/[a-z]{1,8}",
        response in "[a-f0-9]{32}"
    ) {
        let header = format!(
            "Digest username=\"{}\", realm=\"{}\", nonce=\"{}\", uri=\"{}\", response=\"{}\"",
            username, realm, nonce, uri, response
        );
        let auth = DigestAuth::parse(&header).unwrap();

        prop_assert_eq!(auth.param("USERNAME"), Some(username.as_str()));
        prop_assert_eq!(auth.param("REALM"), Some(realm.as_str()));
    }
}

// 任意の文字列で DigestAuth パースがパニックしない
proptest! {
    #[test]
    fn prop_digest_auth_parse_no_panic(s in "[ -~]{0,128}") {
        let _ = DigestAuth::parse(&s);
    }
}

// ========================================
// DigestChallenge のテスト
// ========================================

// DigestChallenge ラウンドトリップ
proptest! {
    #[test]
    fn prop_digest_challenge_roundtrip(realm in param_value(), nonce in param_value()) {
        let header = format!("Digest realm=\"{}\", nonce=\"{}\"", realm, nonce);
        let challenge = DigestChallenge::parse(&header).unwrap();

        prop_assert_eq!(challenge.realm(), Some(realm.as_str()));
        prop_assert_eq!(challenge.nonce(), Some(nonce.as_str()));

        // to_header_value で再エンコードできる
        let header_value = challenge.to_header_value();
        prop_assert!(header_value.starts_with("Digest "));
    }
}

// DigestChallenge Display
proptest! {
    #[test]
    fn prop_digest_challenge_display(realm in param_value(), nonce in param_value()) {
        let header = format!("Digest realm=\"{}\", nonce=\"{}\"", realm, nonce);
        let challenge = DigestChallenge::parse(&header).unwrap();
        let display = challenge.to_string();

        prop_assert!(display.starts_with("Digest "));
    }
}

// DigestChallenge パースエラー
#[test]
fn prop_digest_challenge_parse_errors() {
    // 空
    assert!(matches!(DigestChallenge::parse(""), Err(AuthError::Empty)));
    // Digest スキームでない
    assert!(matches!(
        DigestChallenge::parse("Basic realm=\"test\""),
        Err(AuthError::NotDigestScheme)
    ));
    // 必須パラメータが足りない
    assert!(matches!(
        DigestChallenge::parse("Digest realm=\"test\""),
        Err(AuthError::MissingParameter)
    ));
}

// 任意の文字列で DigestChallenge パースがパニックしない
proptest! {
    #[test]
    fn prop_digest_challenge_parse_no_panic(s in "[ -~]{0,64}") {
        let _ = DigestChallenge::parse(&s);
    }
}

// ========================================
// Authorization enum のテスト
// ========================================

// Authorization::Basic ラウンドトリップ
proptest! {
    #[test]
    fn prop_authorization_basic_roundtrip(username in "[a-zA-Z][a-zA-Z0-9]{0,7}", password in "[a-zA-Z0-9]{0,16}") {
        let auth = BasicAuth::new(&username, &password);
        let header = auth.to_header_value();
        let parsed = Authorization::parse(&header).unwrap();

        if let Authorization::Basic(basic) = parsed {
            prop_assert_eq!(basic.username(), username.as_str());
            prop_assert_eq!(basic.password(), password.as_str());
        } else {
            prop_assert!(false, "Expected Authorization::Basic");
        }
    }
}

// Authorization::Bearer ラウンドトリップ
proptest! {
    #[test]
    fn prop_authorization_bearer_roundtrip(token in token68_string(1, 32)) {
        let header = format!("Bearer {}", token);
        let parsed = Authorization::parse(&header).unwrap();

        if let Authorization::Bearer(bearer) = &parsed {
            prop_assert_eq!(bearer.token(), token.as_str());
        } else {
            prop_assert!(false, "Expected Authorization::Bearer");
        }

        // to_header_value
        let header_value = parsed.to_header_value();
        prop_assert_eq!(header_value, format!("Bearer {}", token));
    }
}

// Authorization::Digest ラウンドトリップ
proptest! {
    #[test]
    fn prop_authorization_digest_roundtrip(
        username in param_value(),
        realm in param_value(),
        nonce in param_value(),
        uri in "/[a-z]{1,8}",
        response in "[a-f0-9]{32}"
    ) {
        let header = format!(
            "Digest username=\"{}\", realm=\"{}\", nonce=\"{}\", uri=\"{}\", response=\"{}\"",
            username, realm, nonce, uri, response
        );
        let parsed = Authorization::parse(&header).unwrap();

        if let Authorization::Digest(digest) = &parsed {
            prop_assert_eq!(digest.username(), Some(username.as_str()));
        } else {
            prop_assert!(false, "Expected Authorization::Digest");
        }

        // to_header_value
        let header_value = parsed.to_header_value();
        prop_assert!(header_value.starts_with("Digest "));
    }
}

// Authorization Display
proptest! {
    #[test]
    fn prop_authorization_display(token in token68_string(1, 32)) {
        let header = format!("Bearer {}", token);
        let parsed = Authorization::parse(&header).unwrap();
        let display = parsed.to_string();

        prop_assert_eq!(display, format!("Bearer {}", token));
    }
}

// Authorization パースエラー
#[test]
fn prop_authorization_parse_errors() {
    // 空
    assert!(matches!(Authorization::parse(""), Err(AuthError::Empty)));
    // 不明なスキーム
    assert!(matches!(
        Authorization::parse("Unknown token"),
        Err(AuthError::InvalidFormat)
    ));
}

// 大文字小文字を区別しない
#[test]
fn prop_authorization_case_insensitive() {
    assert!(Authorization::parse("basic dXNlcjpwYXNz").is_ok());
    assert!(Authorization::parse("Basic dXNlcjpwYXNz").is_ok());
    assert!(Authorization::parse("bearer token123").is_ok());
    assert!(Authorization::parse("Bearer token123").is_ok());
    assert!(
        Authorization::parse(
            "digest username=\"a\", realm=\"b\", nonce=\"c\", uri=\"/\", response=\"d\""
        )
        .is_ok()
    );
    assert!(
        Authorization::parse(
            "Digest username=\"a\", realm=\"b\", nonce=\"c\", uri=\"/\", response=\"d\""
        )
        .is_ok()
    );
}

// 任意の文字列で Authorization パースがパニックしない
proptest! {
    #[test]
    fn prop_authorization_parse_no_panic(s in "[ -~]{0,128}") {
        let _ = Authorization::parse(&s);
    }
}

// ========================================
// AuthChallenge enum のテスト
// ========================================

// AuthChallenge::Basic ラウンドトリップ
proptest! {
    #[test]
    fn prop_auth_challenge_basic_roundtrip(realm in param_value()) {
        let header = format!("Basic realm=\"{}\"", realm);
        let parsed = AuthChallenge::parse(&header).unwrap();

        if let AuthChallenge::Basic(basic) = &parsed {
            prop_assert_eq!(basic.realm(), realm.as_str());
        } else {
            prop_assert!(false, "Expected AuthChallenge::Basic");
        }

        // to_header_value
        let header_value = parsed.to_header_value();
        prop_assert!(header_value.starts_with("Basic "));
    }
}

// AuthChallenge::Bearer ラウンドトリップ
proptest! {
    #[test]
    fn prop_auth_challenge_bearer_roundtrip(realm in param_value()) {
        let header = format!("Bearer realm=\"{}\"", realm);
        let parsed = AuthChallenge::parse(&header).unwrap();

        if let AuthChallenge::Bearer(_) = &parsed {
            // OK
        } else {
            prop_assert!(false, "Expected AuthChallenge::Bearer");
        }

        // to_header_value
        let header_value = parsed.to_header_value();
        prop_assert!(header_value.starts_with("Bearer "));
    }
}

// AuthChallenge::Digest ラウンドトリップ
proptest! {
    #[test]
    fn prop_auth_challenge_digest_roundtrip(realm in param_value(), nonce in param_value()) {
        let header = format!("Digest realm=\"{}\", nonce=\"{}\"", realm, nonce);
        let parsed = AuthChallenge::parse(&header).unwrap();

        if let AuthChallenge::Digest(digest) = &parsed {
            prop_assert_eq!(digest.realm(), Some(realm.as_str()));
            prop_assert_eq!(digest.nonce(), Some(nonce.as_str()));
        } else {
            prop_assert!(false, "Expected AuthChallenge::Digest");
        }

        // to_header_value
        let header_value = parsed.to_header_value();
        prop_assert!(header_value.starts_with("Digest "));
    }
}

// AuthChallenge Display
proptest! {
    #[test]
    fn prop_auth_challenge_display(realm in param_value()) {
        let header = format!("Basic realm=\"{}\"", realm);
        let parsed = AuthChallenge::parse(&header).unwrap();
        let display = parsed.to_string();

        prop_assert!(display.starts_with("Basic "));
    }
}

// AuthChallenge パースエラー
#[test]
fn prop_auth_challenge_parse_errors() {
    // 空
    assert!(matches!(AuthChallenge::parse(""), Err(AuthError::Empty)));
    // 不明なスキーム
    assert!(matches!(
        AuthChallenge::parse("Unknown param=\"value\""),
        Err(AuthError::InvalidFormat)
    ));
}

// 大文字小文字を区別しない
#[test]
fn prop_auth_challenge_case_insensitive() {
    assert!(AuthChallenge::parse("basic realm=\"test\"").is_ok());
    assert!(AuthChallenge::parse("Basic realm=\"test\"").is_ok());
    assert!(AuthChallenge::parse("bearer realm=\"test\"").is_ok());
    assert!(AuthChallenge::parse("Bearer realm=\"test\"").is_ok());
    assert!(AuthChallenge::parse("digest realm=\"a\", nonce=\"b\"").is_ok());
    assert!(AuthChallenge::parse("Digest realm=\"a\", nonce=\"b\"").is_ok());
}

// 任意の文字列で AuthChallenge パースがパニックしない
proptest! {
    #[test]
    fn prop_auth_challenge_parse_no_panic(s in "[ -~]{0,128}") {
        let _ = AuthChallenge::parse(&s);
    }
}

// ========================================
// ProxyAuthorization のテスト
// ========================================

// ProxyAuthorization ラウンドトリップ
proptest! {
    #[test]
    fn prop_proxy_authorization_roundtrip(username in "[a-zA-Z][a-zA-Z0-9]{0,7}", password in "[a-zA-Z0-9]{0,16}") {
        let auth = BasicAuth::new(&username, &password);
        let header = auth.to_header_value();
        let proxy_auth = ProxyAuthorization::parse(&header).unwrap();

        if let Authorization::Basic(basic) = proxy_auth.authorization() {
            prop_assert_eq!(basic.username(), username.as_str());
            prop_assert_eq!(basic.password(), password.as_str());
        } else {
            prop_assert!(false, "Expected Authorization::Basic");
        }

        // to_header_value
        let header_value = proxy_auth.to_header_value();
        prop_assert!(header_value.starts_with("Basic "));
    }
}

// ProxyAuthorization Display
proptest! {
    #[test]
    fn prop_proxy_authorization_display(token in token68_string(1, 32)) {
        let header = format!("Bearer {}", token);
        let proxy_auth = ProxyAuthorization::parse(&header).unwrap();
        let display = proxy_auth.to_string();

        prop_assert_eq!(display, format!("Bearer {}", token));
    }
}

// 任意の文字列で ProxyAuthorization パースがパニックしない
proptest! {
    #[test]
    fn prop_proxy_authorization_parse_no_panic(s in "[ -~]{0,128}") {
        let _ = ProxyAuthorization::parse(&s);
    }
}

// ========================================
// ProxyAuthenticate のテスト
// ========================================

// ProxyAuthenticate ラウンドトリップ
proptest! {
    #[test]
    fn prop_proxy_authenticate_roundtrip(realm in param_value()) {
        let header = format!("Basic realm=\"{}\"", realm);
        let proxy_auth = ProxyAuthenticate::parse(&header).unwrap();

        if let AuthChallenge::Basic(basic) = proxy_auth.challenge() {
            prop_assert_eq!(basic.realm(), realm.as_str());
        } else {
            prop_assert!(false, "Expected AuthChallenge::Basic");
        }

        // to_header_value
        let header_value = proxy_auth.to_header_value();
        prop_assert!(header_value.starts_with("Basic "));
    }
}

// ProxyAuthenticate Display
proptest! {
    #[test]
    fn prop_proxy_authenticate_display(realm in param_value()) {
        let header = format!("Basic realm=\"{}\"", realm);
        let proxy_auth = ProxyAuthenticate::parse(&header).unwrap();
        let display = proxy_auth.to_string();

        prop_assert!(display.starts_with("Basic "));
    }
}

// 任意の文字列で ProxyAuthenticate パースがパニックしない
proptest! {
    #[test]
    fn prop_proxy_authenticate_parse_no_panic(s in "[ -~]{0,128}") {
        let _ = ProxyAuthenticate::parse(&s);
    }
}

// ========================================
// BasicAuth / WwwAuthenticate 追加テスト
// ========================================

// BasicAuth Display
proptest! {
    #[test]
    fn prop_basic_auth_display(username in "[a-zA-Z][a-zA-Z0-9]{0,7}", password in "[a-zA-Z0-9]{0,16}") {
        let auth = BasicAuth::new(&username, &password);
        let display = auth.to_string();

        prop_assert!(display.starts_with("Basic "));
        prop_assert_eq!(display, auth.to_header_value());
    }
}

// WwwAuthenticate to_header_value
proptest! {
    #[test]
    fn prop_www_authenticate_to_header_value(realm in param_value()) {
        let auth = WwwAuthenticate::basic(&realm);
        let header_value = auth.to_header_value();
        let display = auth.to_string();

        prop_assert_eq!(header_value, display);
    }
}
