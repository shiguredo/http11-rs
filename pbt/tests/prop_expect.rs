//! Expect ヘッダーのプロパティテスト (expect.rs)

use proptest::prelude::*;
use shiguredo_http11::expect::Expect;

// ========================================
// Strategy 定義
// ========================================

// トークン文字 (RFC 9110)
fn token_char() -> impl Strategy<Value = char> {
    prop_oneof![
        Just('!'),
        Just('#'),
        Just('$'),
        Just('%'),
        Just('&'),
        Just('\''),
        Just('*'),
        Just('+'),
        Just('-'),
        Just('.'),
        prop::char::range('0', '9'),
        prop::char::range('A', 'Z'),
        Just('^'),
        Just('_'),
        Just('`'),
        prop::char::range('a', 'z'),
        Just('|'),
        Just('~'),
    ]
}

// トークン
fn token() -> impl Strategy<Value = String> {
    proptest::collection::vec(token_char(), 1..=16).prop_map(|chars| chars.into_iter().collect())
}

// 引用符不要の値 (トークン)
fn token_value() -> impl Strategy<Value = String> {
    token()
}

// 引用符付き文字列の中身 (qdtext + quoted-pair)
fn quoted_string_char() -> impl Strategy<Value = char> {
    prop_oneof![
        Just('\t'),
        Just(' '),
        Just('!'),
        // 0x23-0x5B (# から [) ただし \ を除く
        prop::char::range('#', '['),
        // \ (エスケープ対象)
        Just('\\'),
        // 0x5D-0x7E (] から ~)
        prop::char::range(']', '~'),
    ]
}

// 引用符付き文字列の中身
fn quoted_string_content() -> impl Strategy<Value = String> {
    proptest::collection::vec(quoted_string_char(), 0..=16)
        .prop_map(|chars| chars.into_iter().collect())
}

// quoted-string 用エスケープ (\ → \\, " → \")
fn escape_for_quoted_string(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"")
}

// ========================================
// トークン=値 形式のテスト
// ========================================

proptest! {
    #[test]
    fn prop_expect_token_value_roundtrip(t in token(), v in token_value()) {
        let input = format!("{}={}", t, v);
        let expect = Expect::parse(&input).unwrap();

        prop_assert_eq!(expect.items().len(), 1);
        prop_assert_eq!(expect.items()[0].token(), t.to_ascii_lowercase());
        prop_assert_eq!(expect.items()[0].value(), Some(v.as_str()));

        // ラウンドトリップ
        let displayed = expect.to_string();
        let reparsed = Expect::parse(&displayed).unwrap();
        prop_assert_eq!(expect, reparsed);
    }
}

// 引用符付き値
proptest! {
    #[test]
    fn prop_expect_quoted_value_roundtrip(t in token(), v in quoted_string_content()) {
        let escaped = escape_for_quoted_string(&v);
        let input = format!("{}=\"{}\"", t, escaped);
        let expect = Expect::parse(&input).unwrap();

        prop_assert_eq!(expect.items().len(), 1);
        prop_assert_eq!(expect.items()[0].token(), t.to_ascii_lowercase());
        prop_assert_eq!(expect.items()[0].value(), Some(v.as_str()));

        // ラウンドトリップ
        let displayed = expect.to_string();
        let reparsed = Expect::parse(&displayed).unwrap();
        prop_assert_eq!(expect, reparsed);
    }
}

// ========================================
// 複数 expectation のテスト
// ========================================

proptest! {
    #[test]
    fn prop_expect_multiple_items(
        t1 in token(),
        v1 in token_value(),
        t2 in token()
    ) {
        let input = format!("{}={}, {}", t1, v1, t2);
        let expect = Expect::parse(&input).unwrap();

        prop_assert_eq!(expect.items().len(), 2);
        prop_assert_eq!(expect.items()[0].token(), t1.to_ascii_lowercase());
        prop_assert_eq!(expect.items()[0].value(), Some(v1.as_str()));
        prop_assert_eq!(expect.items()[1].token(), t2.to_ascii_lowercase());
        prop_assert_eq!(expect.items()[1].value(), None);

        // ラウンドトリップ
        let displayed = expect.to_string();
        let reparsed = Expect::parse(&displayed).unwrap();
        prop_assert_eq!(expect, reparsed);
    }
}

// 100-continue を含む複数 expectation
proptest! {
    #[test]
    fn prop_expect_with_100_continue(t in token(), v in token_value()) {
        let input = format!("{}={}, 100-continue", t, v);
        let expect = Expect::parse(&input).unwrap();

        prop_assert!(expect.has_100_continue());
        prop_assert_eq!(expect.items().len(), 2);
        prop_assert!(!expect.items()[0].is_100_continue());
        prop_assert!(expect.items()[1].is_100_continue());

        // ラウンドトリップ
        let displayed = expect.to_string();
        let reparsed = Expect::parse(&displayed).unwrap();
        prop_assert_eq!(expect, reparsed);
    }
}

// ========================================
// 複合テスト
// ========================================

proptest! {
    #[test]
    fn prop_expect_complex_roundtrip(
        t1 in token(),
        v1 in quoted_string_content(),
        t2 in token(),
        v2 in token_value()
    ) {
        // 引用符付き値 + トークン値 + 100-continue
        let escaped_v1 = escape_for_quoted_string(&v1);
        let input = format!("{}=\"{}\", {}={}, 100-continue", t1, escaped_v1, t2, v2);
        let expect = Expect::parse(&input).unwrap();

        prop_assert_eq!(expect.items().len(), 3);
        prop_assert!(expect.has_100_continue());

        // ラウンドトリップ
        let displayed = expect.to_string();
        let reparsed = Expect::parse(&displayed).unwrap();
        prop_assert_eq!(expect, reparsed);
    }
}
