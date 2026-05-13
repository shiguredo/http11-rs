//! HTTP 認証のプロパティテスト (Digest / Bearer / Authorization / AuthChallenge)

use proptest::prelude::*;
use shiguredo_http11::auth::{
    AuthChallenge, Authorization, BasicAuth, BearerChallenge, BearerToken, DigestAuth,
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

// quoted-string の qdtext として許容される char (obs-text の Unicode scalar 拡張を含む)
//
// RFC 9110 Section 5.6.4 の qdtext ABNF (オクテット表現) を、char 単位走査の本実装に
// 合わせて Unicode scalar に拡張解釈する (issue 0059)。
fn qdtext_char() -> impl Strategy<Value = char> {
    prop_oneof![
        Just('\t'),
        Just(' '),
        Just('!'),
        prop::char::range('#', '['), // 0x23-0x5B (DQUOTE と backslash を除く)
        prop::char::range(']', '~'), // 0x5D-0x7E
        // obs-text を Unicode scalar として opaque 保持する範囲。surrogate (`U+D800..=U+DFFF`)
        // は char 型で構築不能なので、shrink バイアスを surrogate 跨ぎで歪めないため二分割する。
        prop::char::range('\u{80}', '\u{D7FF}'),
        prop::char::range('\u{E000}', '\u{10FFFF}'),
    ]
}

// quoted-string で囲まれた realm 値 (obs-text 含む)
fn qdtext_realm() -> impl Strategy<Value = String> {
    proptest::collection::vec(qdtext_char(), 1..16).prop_map(|chars| chars.into_iter().collect())
}

// スキーム名のランダムケーシング ("Basic" -> "basic", "BASIC", "bAsIc" など)
fn randomize_case(scheme: &'static str) -> impl Strategy<Value = String> {
    proptest::collection::vec(proptest::bool::ANY, scheme.len()).prop_map(move |bools| {
        scheme
            .chars()
            .zip(bools.iter())
            .map(|(c, &upper)| {
                if upper {
                    c.to_uppercase().to_string()
                } else {
                    c.to_lowercase().to_string()
                }
            })
            .collect()
    })
}

// コロンを含むパスワード
fn password_with_colon() -> impl Strategy<Value = String> {
    ("[a-zA-Z0-9]{0,8}", "[a-zA-Z0-9]{0,8}").prop_map(|(a, b)| format!("{}:{}", a, b))
}

// ========================================
// BasicAuth のテスト
// ========================================

// BasicAuth ラウンドトリップ
proptest! {
    #[test]
    fn prop_basic_auth_roundtrip(username in "[a-zA-Z][a-zA-Z0-9_]{0,15}", password in "[a-zA-Z0-9!@#$%^&*]{0,16}") {
        let auth = BasicAuth::new(&username, &password).unwrap();
        let header = auth.to_header_value();
        let reparsed = BasicAuth::parse(&header).unwrap();

        prop_assert_eq!(auth.username(), reparsed.username());
        prop_assert_eq!(auth.password(), reparsed.password());
    }
}

// BasicAuth ラウンドトリップ (コロンを含むパスワード)
proptest! {
    #[test]
    fn prop_basic_auth_colon_in_password(username in "[a-zA-Z][a-zA-Z0-9]{0,7}", password in password_with_colon()) {
        let auth = BasicAuth::new(&username, &password).unwrap();
        let header = auth.to_header_value();
        let reparsed = BasicAuth::parse(&header).unwrap();

        prop_assert_eq!(reparsed.username(), username.as_str());
        prop_assert_eq!(reparsed.password(), password.as_str());
    }
}

// BasicAuth スキーム名の大文字小文字を区別しない
proptest! {
    #[test]
    fn prop_basic_auth_case_insensitive(
        scheme in randomize_case("Basic"),
        username in "[a-zA-Z][a-zA-Z0-9]{0,7}",
        password in "[a-zA-Z0-9]{0,16}",
    ) {
        let auth = BasicAuth::new(&username, &password).unwrap();
        let canonical = auth.to_header_value();
        // スキーム名を差し替え
        let header = format!("{} {}", scheme, &canonical["Basic ".len()..]);
        let parsed = BasicAuth::parse(&header).unwrap();

        prop_assert_eq!(parsed.username(), username.as_str());
        prop_assert_eq!(parsed.password(), password.as_str());
    }
}

// ========================================
// WwwAuthenticate のテスト
// ========================================

// WwwAuthenticate charset UTF-8 付きラウンドトリップ
proptest! {
    #[test]
    fn prop_www_authenticate_charset_roundtrip(
        realm in param_value(),
    ) {
        let auth = WwwAuthenticate::basic(&realm).with_charset_utf8();
        let header = auth.to_string();
        let reparsed = WwwAuthenticate::parse(&header).unwrap();

        prop_assert_eq!(reparsed.realm(), realm.as_str());
        prop_assert_eq!(reparsed.charset(), Some("UTF-8"));
    }
}

// WwwAuthenticate スキーム名の大文字小文字を区別しない
proptest! {
    #[test]
    fn prop_www_authenticate_case_insensitive(
        scheme in randomize_case("Basic"),
        realm in param_value(),
    ) {
        let header = format!("{} realm=\"{}\"", scheme, realm);
        let parsed = WwwAuthenticate::parse(&header).unwrap();

        prop_assert_eq!(parsed.realm(), realm.as_str());
    }
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

// 大文字小文字を区別しない
proptest! {
    #[test]
    fn prop_bearer_token_case_insensitive(token in token68_string(1, 32)) {
        let lower = BearerToken::parse(&format!("bearer {}", token)).unwrap();
        let upper = BearerToken::parse(&format!("Bearer {}", token)).unwrap();

        prop_assert_eq!(lower.token(), upper.token());
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

// ========================================
// Authorization enum のテスト
// ========================================

// Authorization::Basic ラウンドトリップ
proptest! {
    #[test]
    fn prop_authorization_basic_roundtrip(username in "[a-zA-Z][a-zA-Z0-9]{0,7}", password in "[a-zA-Z0-9]{0,16}") {
        let auth = BasicAuth::new(&username, &password).unwrap();
        let header = auth.to_header_value();
        let parsed = Authorization::parse(&header).unwrap();

        if let Authorization::Basic(basic) = parsed {
            prop_assert_eq!(basic.username(), username.as_str());
            prop_assert_eq!(basic.password(), password.as_str());
        } else {
            prop_assert!(false, "Authorization::Basic を期待");
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
            prop_assert!(false, "Authorization::Bearer を期待");
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
            prop_assert!(false, "Authorization::Digest を期待");
        }

        // to_header_value
        let header_value = parsed.to_header_value();
        prop_assert!(header_value.starts_with("Digest "));
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
            prop_assert!(false, "AuthChallenge::Basic を期待");
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
            prop_assert!(false, "AuthChallenge::Bearer を期待");
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
            prop_assert!(false, "AuthChallenge::Digest を期待");
        }

        // to_header_value
        let header_value = parsed.to_header_value();
        prop_assert!(header_value.starts_with("Digest "));
    }
}

// ========================================
// Authorization スキーム名の大文字小文字を区別しない
// ========================================

proptest! {
    #[test]
    fn prop_authorization_case_insensitive_basic(
        scheme in randomize_case("Basic"),
        username in "[a-zA-Z][a-zA-Z0-9]{0,7}",
        password in "[a-zA-Z0-9]{0,16}",
    ) {
        let auth = BasicAuth::new(&username, &password).unwrap();
        let canonical = auth.to_header_value();
        let header = format!("{} {}", scheme, &canonical["Basic ".len()..]);
        let parsed = Authorization::parse(&header).unwrap();

        if let Authorization::Basic(basic) = parsed {
            prop_assert_eq!(basic.username(), username.as_str());
        } else {
            prop_assert!(false, "Authorization::Basic を期待");
        }
    }
}

proptest! {
    #[test]
    fn prop_authorization_case_insensitive_bearer(
        scheme in randomize_case("Bearer"),
        token in token68_string(1, 32),
    ) {
        let header = format!("{} {}", scheme, token);
        let parsed = Authorization::parse(&header).unwrap();

        if let Authorization::Bearer(bearer) = parsed {
            prop_assert_eq!(bearer.token(), token.as_str());
        } else {
            prop_assert!(false, "Authorization::Bearer を期待");
        }
    }
}

proptest! {
    #[test]
    fn prop_authorization_case_insensitive_digest(
        scheme in randomize_case("Digest"),
        username in param_value(),
        realm in param_value(),
        nonce in param_value(),
        uri in "/[a-z]{1,8}",
        response in "[a-f0-9]{32}",
    ) {
        let header = format!(
            "{} username=\"{}\", realm=\"{}\", nonce=\"{}\", uri=\"{}\", response=\"{}\"",
            scheme, username, realm, nonce, uri, response
        );
        let parsed = Authorization::parse(&header).unwrap();

        if let Authorization::Digest(digest) = parsed {
            prop_assert_eq!(digest.username(), Some(username.as_str()));
        } else {
            prop_assert!(false, "Authorization::Digest を期待");
        }
    }
}

// ========================================
// AuthChallenge スキーム名の大文字小文字を区別しない
// ========================================

proptest! {
    #[test]
    fn prop_auth_challenge_case_insensitive_basic(
        scheme in randomize_case("Basic"),
        realm in param_value(),
    ) {
        let header = format!("{} realm=\"{}\"", scheme, realm);
        let parsed = AuthChallenge::parse(&header).unwrap();

        if let AuthChallenge::Basic(basic) = parsed {
            prop_assert_eq!(basic.realm(), realm.as_str());
        } else {
            prop_assert!(false, "AuthChallenge::Basic を期待");
        }
    }
}

proptest! {
    #[test]
    fn prop_auth_challenge_case_insensitive_bearer(
        scheme in randomize_case("Bearer"),
        realm in param_value(),
    ) {
        let header = format!("{} realm=\"{}\"", scheme, realm);
        let parsed = AuthChallenge::parse(&header).unwrap();

        if let AuthChallenge::Bearer(_) = parsed {
            // OK
        } else {
            prop_assert!(false, "AuthChallenge::Bearer を期待");
        }
    }
}

proptest! {
    #[test]
    fn prop_auth_challenge_case_insensitive_digest(
        scheme in randomize_case("Digest"),
        realm in param_value(),
        nonce in param_value(),
    ) {
        let header = format!("{} realm=\"{}\", nonce=\"{}\"", scheme, realm, nonce);
        let parsed = AuthChallenge::parse(&header).unwrap();

        if let AuthChallenge::Digest(digest) = parsed {
            prop_assert_eq!(digest.realm(), Some(realm.as_str()));
        } else {
            prop_assert!(false, "AuthChallenge::Digest を期待");
        }
    }
}

// ========================================
// ProxyAuthorization のテスト
// ========================================

// ProxyAuthorization ラウンドトリップ
proptest! {
    #[test]
    fn prop_proxy_authorization_roundtrip(username in "[a-zA-Z][a-zA-Z0-9]{0,7}", password in "[a-zA-Z0-9]{0,16}") {
        let auth = BasicAuth::new(&username, &password).unwrap();
        let header = auth.to_header_value();
        let proxy_auth = ProxyAuthorization::parse(&header).unwrap();

        if let Authorization::Basic(basic) = proxy_auth.authorization() {
            prop_assert_eq!(basic.username(), username.as_str());
            prop_assert_eq!(basic.password(), password.as_str());
        } else {
            prop_assert!(false, "Authorization::Basic を期待");
        }

        // to_header_value
        let header_value = proxy_auth.to_header_value();
        prop_assert!(header_value.starts_with("Basic "));
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
            prop_assert!(false, "AuthChallenge::Basic を期待");
        }

        // to_header_value
        let header_value = proxy_auth.to_header_value();
        prop_assert!(header_value.starts_with("Basic "));
    }
}

// ========================================
// WwwAuthenticate のテスト
// ========================================

// WwwAuthenticate ラウンドトリップ
proptest! {
    #[test]
    fn prop_www_authenticate_roundtrip(realm in "[a-z]{1,8}\\.[a-z]{2,6}") {
        let auth = WwwAuthenticate::basic(&realm);
        let header = auth.to_string();
        let reparsed = WwwAuthenticate::parse(&header).unwrap();

        prop_assert_eq!(auth.realm(), reparsed.realm());
    }
}

// WwwAuthenticate with charset UTF-8 ラウンドトリップ
proptest! {
    #[test]
    fn prop_www_authenticate_with_charset_utf8_roundtrip(realm in "[a-z]{1,8}\\.[a-z]{2,6}") {
        let auth = WwwAuthenticate::basic(&realm).with_charset_utf8();
        let header = auth.to_string();
        let reparsed = WwwAuthenticate::parse(&header).unwrap();

        prop_assert_eq!(reparsed.realm(), realm.as_str());
        prop_assert_eq!(reparsed.charset(), Some("UTF-8"));
    }
}

// ========================================
// obs-text Unicode scalar 拡張のラウンドトリップ (issue 0059)
// ========================================

// WwwAuthenticate (Basic realm=...) に obs-text を含む UTF-8 char が含まれても
// `parse -> to_string -> parse` のラウンドトリップで一致する。
proptest! {
    #[test]
    fn prop_www_authenticate_obs_text_roundtrip(realm in qdtext_realm()) {
        let input = format!("Basic realm=\"{}\"", realm);
        let parsed = WwwAuthenticate::parse(&input).unwrap();
        prop_assert_eq!(parsed.realm(), realm.as_str());

        let displayed = parsed.to_string();
        let reparsed = WwwAuthenticate::parse(&displayed).unwrap();
        prop_assert_eq!(reparsed.realm(), realm.as_str());
    }
}

// DigestChallenge の realm / nonce に obs-text を含む UTF-8 char が含まれても
// ラウンドトリップで一致する。
proptest! {
    #[test]
    fn prop_digest_challenge_obs_text_roundtrip(
        realm in qdtext_realm(),
        nonce in qdtext_realm(),
    ) {
        let input = format!("Digest realm=\"{}\", nonce=\"{}\"", realm, nonce);
        let parsed = DigestChallenge::parse(&input).unwrap();
        prop_assert_eq!(parsed.realm(), Some(realm.as_str()));
        prop_assert_eq!(parsed.nonce(), Some(nonce.as_str()));

        let displayed = parsed.to_header_value();
        let reparsed = DigestChallenge::parse(&displayed).unwrap();
        prop_assert_eq!(reparsed.realm(), Some(realm.as_str()));
        prop_assert_eq!(reparsed.nonce(), Some(nonce.as_str()));
    }
}

// ========================================
// auth-param の hard cap (issue 0047)
// ========================================

proptest! {
    /// 33..=200 個のパラメータは `TooManyParameters` を返す
    #[test]
    fn prop_auth_challenge_too_many_params(count in 33usize..=200) {
        let mut s = String::from("Bearer ");
        for i in 0..count {
            if i > 0 {
                s.push_str(", ");
            }
            s.push_str(&format!("p{}=\"v\"", i));
        }
        let result = AuthChallenge::parse(&s);
        prop_assert!(matches!(
            result,
            Err(shiguredo_http11::auth::AuthError::TooManyParameters)
        ));
    }
}

proptest! {
    /// 1..=32 個のパラメータは正常に parse される
    #[test]
    fn prop_auth_challenge_at_most_32_params_ok(count in 1usize..=32) {
        let mut s = String::from("Bearer ");
        for i in 0..count {
            if i > 0 {
                s.push_str(", ");
            }
            s.push_str(&format!("p{}=\"v\"", i));
        }
        let result = AuthChallenge::parse(&s);
        prop_assert!(result.is_ok(), "count={}: {:?}", count, result);
    }
}
