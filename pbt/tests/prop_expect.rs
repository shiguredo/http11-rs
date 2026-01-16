//! Expect ヘッダーのプロパティテスト (expect.rs)

use proptest::prelude::*;
use shiguredo_http11::expect::{Expect, ExpectError};

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

// 引用符付き文字列の中身 (qdtext)
fn qdtext_char() -> impl Strategy<Value = char> {
    prop_oneof![
        Just('\t'),
        Just(' '),
        Just('!'),
        // 0x23-0x5B (# から [) ただし \ を除く
        prop::char::range('#', '['),
        // 0x5D-0x7E (] から ~)
        prop::char::range(']', '~'),
    ]
}

// 引用符付き文字列の中身
fn quoted_string_content() -> impl Strategy<Value = String> {
    proptest::collection::vec(qdtext_char(), 0..=16).prop_map(|chars| chars.into_iter().collect())
}

// ========================================
// ExpectError のテスト
// ========================================

#[test]
fn expect_error_display() {
    let errors = [
        (ExpectError::Empty, "empty Expect header"),
        (ExpectError::InvalidFormat, "invalid Expect header format"),
        (ExpectError::InvalidToken, "invalid Expect token"),
        (ExpectError::InvalidValue, "invalid Expect value"),
    ];

    for (error, expected) in errors {
        assert_eq!(error.to_string(), expected);
    }
}

#[test]
fn expect_error_is_error_trait() {
    let error: Box<dyn std::error::Error> = Box::new(ExpectError::Empty);
    assert_eq!(error.to_string(), "empty Expect header");
}

#[test]
fn expect_error_clone_eq() {
    let error = ExpectError::InvalidToken;
    let cloned = error.clone();
    assert_eq!(error, cloned);
}

// ========================================
// 100-continue のテスト
// ========================================

#[test]
fn expect_100_continue() {
    let expect = Expect::parse("100-continue").unwrap();
    assert!(expect.has_100_continue());
    assert_eq!(expect.items().len(), 1);
    assert!(expect.items()[0].is_100_continue());
}

#[test]
fn expect_100_continue_case_insensitive() {
    // 大文字小文字を区別しない
    let expect = Expect::parse("100-CONTINUE").unwrap();
    assert!(expect.has_100_continue());

    let expect = Expect::parse("100-Continue").unwrap();
    assert!(expect.has_100_continue());
}

#[test]
fn expect_100_continue_roundtrip() {
    let input = "100-continue";
    let expect = Expect::parse(input).unwrap();
    let displayed = expect.to_string();
    let reparsed = Expect::parse(&displayed).unwrap();

    assert_eq!(expect, reparsed);
}

// ========================================
// トークン=値 形式のテスト
// ========================================

proptest! {
    #[test]
    fn expect_token_value_roundtrip(t in token(), v in token_value()) {
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
    fn expect_quoted_value_roundtrip(t in token(), v in quoted_string_content()) {
        let input = format!("{}=\"{}\"", t, v);
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
    fn expect_multiple_items(
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
    fn expect_with_100_continue(t in token(), v in token_value()) {
        let input = format!("{}={}, 100-continue", t, v);
        let expect = Expect::parse(&input).unwrap();

        prop_assert!(expect.has_100_continue());
        prop_assert_eq!(expect.items().len(), 2);

        // ラウンドトリップ
        let displayed = expect.to_string();
        let reparsed = Expect::parse(&displayed).unwrap();
        prop_assert_eq!(expect, reparsed);
    }
}

// ========================================
// エスケープシーケンスのテスト
// ========================================

#[test]
fn expect_escaped_backslash() {
    let input = r#"token="value\\with\\backslash""#;
    let expect = Expect::parse(input).unwrap();
    assert_eq!(expect.items()[0].value(), Some("value\\with\\backslash"));

    // ラウンドトリップ
    let displayed = expect.to_string();
    let reparsed = Expect::parse(&displayed).unwrap();
    assert_eq!(expect, reparsed);
}

#[test]
fn expect_escaped_quote() {
    let input = r#"token="value\"with\"quotes""#;
    let expect = Expect::parse(input).unwrap();
    assert_eq!(expect.items()[0].value(), Some("value\"with\"quotes"));

    // ラウンドトリップ
    let displayed = expect.to_string();
    let reparsed = Expect::parse(&displayed).unwrap();
    assert_eq!(expect, reparsed);
}

#[test]
fn expect_mixed_escapes() {
    let input = r#"token="\\\"mixed\\\"escapes\\""#;
    let expect = Expect::parse(input).unwrap();
    assert_eq!(expect.items()[0].value(), Some("\\\"mixed\\\"escapes\\"));

    // ラウンドトリップ
    let displayed = expect.to_string();
    let reparsed = Expect::parse(&displayed).unwrap();
    assert_eq!(expect, reparsed);
}

// ========================================
// Expectation メソッドのテスト
// ========================================

proptest! {
    #[test]
    fn expectation_token_method(t in token()) {
        let input = format!("{}", t);
        let expect = Expect::parse(&input).unwrap();

        prop_assert_eq!(expect.items()[0].token(), t.to_ascii_lowercase());
    }
}

proptest! {
    #[test]
    fn expectation_value_method(t in token(), v in token_value()) {
        let input = format!("{}={}", t, v);
        let expect = Expect::parse(&input).unwrap();

        prop_assert_eq!(expect.items()[0].value(), Some(v.as_str()));
    }
}

#[test]
fn expectation_value_none() {
    let expect = Expect::parse("token").unwrap();
    assert_eq!(expect.items()[0].value(), None);
}

#[test]
fn expectation_is_100_continue() {
    let expect = Expect::parse("100-continue").unwrap();
    assert!(expect.items()[0].is_100_continue());

    let expect = Expect::parse("other-token").unwrap();
    assert!(!expect.items()[0].is_100_continue());
}

// ========================================
// パースエラーのテスト
// ========================================

#[test]
fn expect_parse_errors() {
    // 空
    assert!(matches!(Expect::parse(""), Err(ExpectError::Empty)));
    assert!(matches!(Expect::parse("   "), Err(ExpectError::Empty)));

    // 不正なトークン (スペースを含む)
    assert!(matches!(
        Expect::parse("bad value"),
        Err(ExpectError::InvalidToken)
    ));

    // 空のトークン
    assert!(matches!(
        Expect::parse("=value"),
        Err(ExpectError::InvalidFormat)
    ));

    // 空の値
    assert!(matches!(
        Expect::parse("token="),
        Err(ExpectError::InvalidValue)
    ));

    // 閉じ引用符がない
    assert!(matches!(
        Expect::parse("token=\"unclosed"),
        Err(ExpectError::InvalidValue)
    ));

    // 不正な値 (引用符付きでもトークンでもない)
    assert!(matches!(
        Expect::parse("token=bad value"),
        Err(ExpectError::InvalidValue)
    ));

    // 空のパート
    assert!(matches!(
        Expect::parse("token,,other"),
        Err(ExpectError::InvalidFormat)
    ));
}

// ========================================
// Display のテスト
// ========================================

proptest! {
    #[test]
    fn expect_display_roundtrip(t in token()) {
        let input = t.clone();
        let expect = Expect::parse(&input).unwrap();
        let displayed = expect.to_string();
        let reparsed = Expect::parse(&displayed).unwrap();

        prop_assert_eq!(expect, reparsed);
    }
}

#[test]
fn expectation_display_with_quoting() {
    // 引用符が必要な値
    let expect = Expect::parse("token=\"value with spaces\"").unwrap();
    let displayed = expect.to_string();
    assert!(displayed.contains("\""));
    assert!(displayed.contains("value with spaces"));
}

#[test]
fn expectation_display_without_quoting() {
    // 引用符が不要な値
    let expect = Expect::parse("token=simple").unwrap();
    let displayed = expect.to_string();
    assert_eq!(displayed, "token=simple");
}

// ========================================
// Clone と PartialEq のテスト
// ========================================

proptest! {
    #[test]
    fn expect_clone_eq(t in token(), v in token_value()) {
        let input = format!("{}={}", t, v);
        let expect = Expect::parse(&input).unwrap();
        let cloned = expect.clone();

        prop_assert_eq!(expect, cloned);
    }
}

// ========================================
// パニック安全性テスト
// ========================================

proptest! {
    #[test]
    fn expect_parse_no_panic(data in proptest::collection::vec(any::<u8>(), 0..128)) {
        if let Ok(s) = std::str::from_utf8(&data) {
            let _ = Expect::parse(s);
        }
    }
}

// ========================================
// 値に特殊文字を含むテスト
// ========================================

#[test]
fn expect_value_with_comma() {
    // カンマは引用符で囲む必要がある
    let input = r#"token="value,with,commas""#;
    let expect = Expect::parse(input).unwrap();
    assert_eq!(expect.items()[0].value(), Some("value,with,commas"));

    // ラウンドトリップ
    let displayed = expect.to_string();
    let reparsed = Expect::parse(&displayed).unwrap();
    assert_eq!(expect, reparsed);
}

#[test]
fn expect_value_with_equals() {
    // = は引用符で囲む必要がある
    let input = r#"token="value=with=equals""#;
    let expect = Expect::parse(input).unwrap();
    assert_eq!(expect.items()[0].value(), Some("value=with=equals"));

    // ラウンドトリップ
    let displayed = expect.to_string();
    let reparsed = Expect::parse(&displayed).unwrap();
    assert_eq!(expect, reparsed);
}

// ========================================
// 空白のトリムテスト
// ========================================

#[test]
fn expect_whitespace_handling() {
    // 前後の空白
    let expect = Expect::parse("  100-continue  ").unwrap();
    assert!(expect.has_100_continue());

    // カンマ周りの空白
    let expect = Expect::parse("token=value  ,  100-continue").unwrap();
    assert_eq!(expect.items().len(), 2);
    assert!(expect.has_100_continue());
}

// ========================================
// 引用符内のエスケープされていない文字
// ========================================

#[test]
fn expect_quoted_with_special_chars() {
    // タブ文字
    let input = "token=\"value\twith\ttabs\"";
    let expect = Expect::parse(input).unwrap();
    assert_eq!(expect.items()[0].value(), Some("value\twith\ttabs"));
}

// ========================================
// 複合テスト
// ========================================

proptest! {
    #[test]
    fn expect_complex_roundtrip(
        t1 in token(),
        v1 in quoted_string_content(),
        t2 in token(),
        v2 in token_value()
    ) {
        // 引用符付き値 + トークン値 + 100-continue
        let input = format!("{}=\"{}\", {}={}, 100-continue", t1, v1, t2, v2);
        let expect = Expect::parse(&input).unwrap();

        prop_assert_eq!(expect.items().len(), 3);
        prop_assert!(expect.has_100_continue());

        // ラウンドトリップ
        let displayed = expect.to_string();
        let reparsed = Expect::parse(&displayed).unwrap();
        prop_assert_eq!(expect, reparsed);
    }
}
