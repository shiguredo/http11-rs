//! Basic 認証のプロパティテスト

use proptest::prelude::*;
use shiguredo_http11::auth::{BasicAuth, WwwAuthenticate};

// ========================================
// Basic 認証のテスト
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

// パスワードにコロンを含む場合のラウンドトリップ
proptest! {
    #[test]
    fn prop_basic_auth_password_with_colon(username in "[a-zA-Z][a-zA-Z0-9]{0,7}", password_part1 in "[a-zA-Z0-9]{0,8}", password_part2 in "[a-zA-Z0-9]{0,8}") {
        let password = format!("{}:{}", password_part1, password_part2);
        let auth = BasicAuth::new(&username, &password).unwrap();
        let header = auth.to_header_value();
        let reparsed = BasicAuth::parse(&header).unwrap();

        prop_assert_eq!(auth.username(), reparsed.username());
        prop_assert_eq!(auth.password(), reparsed.password());
    }
}

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
