//! Basic 認証のプロパティテスト

use proptest::prelude::*;
use shiguredo_http11::auth::{BasicAuth, WwwAuthenticate};

// ========================================
// Basic 認証のテスト
// ========================================

// BasicAuth ラウンドトリップ
proptest! {
    #[test]
    fn basic_auth_roundtrip(username in "[a-zA-Z][a-zA-Z0-9_]{0,15}", password in "[a-zA-Z0-9!@#$%^&*]{0,16}") {
        let auth = BasicAuth::new(&username, &password);
        let header = auth.to_header_value();
        let reparsed = BasicAuth::parse(&header).unwrap();

        prop_assert_eq!(auth.username(), reparsed.username());
        prop_assert_eq!(auth.password(), reparsed.password());
    }
}

// パスワードにコロンを含む場合のラウンドトリップ
proptest! {
    #[test]
    fn basic_auth_password_with_colon(username in "[a-zA-Z][a-zA-Z0-9]{0,7}", password_part1 in "[a-zA-Z0-9]{0,8}", password_part2 in "[a-zA-Z0-9]{0,8}") {
        let password = format!("{}:{}", password_part1, password_part2);
        let auth = BasicAuth::new(&username, &password);
        let header = auth.to_header_value();
        let reparsed = BasicAuth::parse(&header).unwrap();

        prop_assert_eq!(auth.username(), reparsed.username());
        prop_assert_eq!(auth.password(), reparsed.password());
    }
}

// WwwAuthenticate ラウンドトリップ
proptest! {
    #[test]
    fn www_authenticate_roundtrip(realm in "[a-z]{1,8}\\.[a-z]{2,6}") {
        let auth = WwwAuthenticate::basic(&realm);
        let header = auth.to_string();
        let reparsed = WwwAuthenticate::parse(&header).unwrap();

        prop_assert_eq!(auth.realm(), reparsed.realm());
    }
}

// WwwAuthenticate with charset ラウンドトリップ
proptest! {
    #[test]
    fn www_authenticate_with_charset_roundtrip(realm in "[a-z]{1,8}\\.[a-z]{2,6}", charset in prop_oneof![Just("UTF-8"), Just("ISO-8859-1")]) {
        let auth = WwwAuthenticate::basic(&realm).with_charset(charset);
        let header = auth.to_string();
        let reparsed = WwwAuthenticate::parse(&header).unwrap();

        prop_assert_eq!(auth.realm(), reparsed.realm());
        prop_assert_eq!(auth.charset(), reparsed.charset());
    }
}

// 任意の文字列で BasicAuth パースがパニックしない
proptest! {
    #[test]
    fn basic_auth_parse_no_panic(s in "[ -~]{0,64}") {
        let _ = BasicAuth::parse(&s);
    }
}

// 任意の文字列で WwwAuthenticate パースがパニックしない
proptest! {
    #[test]
    fn www_authenticate_parse_no_panic(s in "[ -~]{0,64}") {
        let _ = WwwAuthenticate::parse(&s);
    }
}
