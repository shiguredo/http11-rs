//! HTTP 認証のプロパティテスト (Digest / Bearer / Authorization / AuthChallenge)

use proptest::prelude::*;
use shiguredo_http11::auth::{
    AuthChallenge, Authorization, BasicAuth, BearerChallenge, BearerToken, DigestAuth,
    DigestChallenge, ProxyAuthenticate, ProxyAuthorization,
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
