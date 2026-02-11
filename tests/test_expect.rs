//! Expect ヘッダーのユニットテスト

use shiguredo_http11::expect::{Expect, ExpectError};

// ========================================
// ExpectError のテスト
// ========================================

#[test]
fn test_expect_error_display() {
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

// ========================================
// 100-continue のテスト
// ========================================

#[test]
fn test_expect_100_continue() {
    let expect = Expect::parse("100-continue").unwrap();
    assert!(expect.has_100_continue());
    assert_eq!(expect.items().len(), 1);
    assert!(expect.items()[0].is_100_continue());
}

#[test]
fn test_expect_100_continue_case_insensitive() {
    // 大文字小文字を区別しない
    let expect = Expect::parse("100-CONTINUE").unwrap();
    assert!(expect.has_100_continue());

    let expect = Expect::parse("100-Continue").unwrap();
    assert!(expect.has_100_continue());
}

#[test]
fn test_expect_100_continue_roundtrip() {
    let input = "100-continue";
    let expect = Expect::parse(input).unwrap();
    let displayed = expect.to_string();
    let reparsed = Expect::parse(&displayed).unwrap();

    assert_eq!(expect, reparsed);
}

// ========================================
// エスケープシーケンスのテスト
// ========================================

#[test]
fn test_expect_escaped_backslash() {
    let input = r#"token="value\\with\\backslash""#;
    let expect = Expect::parse(input).unwrap();
    assert_eq!(expect.items()[0].value(), Some("value\\with\\backslash"));

    // ラウンドトリップ
    let displayed = expect.to_string();
    let reparsed = Expect::parse(&displayed).unwrap();
    assert_eq!(expect, reparsed);
}

#[test]
fn test_expect_escaped_quote() {
    let input = r#"token="value\"with\"quotes""#;
    let expect = Expect::parse(input).unwrap();
    assert_eq!(expect.items()[0].value(), Some("value\"with\"quotes"));

    // ラウンドトリップ
    let displayed = expect.to_string();
    let reparsed = Expect::parse(&displayed).unwrap();
    assert_eq!(expect, reparsed);
}

#[test]
fn test_expect_mixed_escapes() {
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

#[test]
fn test_expectation_value_none() {
    let expect = Expect::parse("token").unwrap();
    assert_eq!(expect.items()[0].value(), None);
}

#[test]
fn test_expectation_is_100_continue() {
    let expect = Expect::parse("100-continue").unwrap();
    assert!(expect.items()[0].is_100_continue());

    let expect = Expect::parse("other-token").unwrap();
    assert!(!expect.items()[0].is_100_continue());
}

// ========================================
// パースエラーのテスト
// ========================================

#[test]
fn test_expect_parse_errors() {
    // RFC 9110 Section 5.6.1.2: 空フィールド値は空リストとして受理する
    let expect = Expect::parse("").unwrap();
    assert!(expect.items().is_empty());
    let expect = Expect::parse("   ").unwrap();
    assert!(expect.items().is_empty());

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

    // RFC 9110 Section 5.6.1.2: 空要素は無視する
    let expect = Expect::parse("token,,other").unwrap();
    assert_eq!(expect.items().len(), 2);
}

// ========================================
// Display のテスト
// ========================================

#[test]
fn test_expectation_display_with_quoting() {
    // 引用符が必要な値
    let expect = Expect::parse("token=\"value with spaces\"").unwrap();
    let displayed = expect.to_string();
    assert!(displayed.contains("\""));
    assert!(displayed.contains("value with spaces"));
}

#[test]
fn test_expectation_display_without_quoting() {
    // 引用符が不要な値
    let expect = Expect::parse("token=simple").unwrap();
    let displayed = expect.to_string();
    assert_eq!(displayed, "token=simple");
}

// ========================================
// 値に特殊文字を含むテスト
// ========================================

#[test]
fn test_expect_value_with_comma() {
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
fn test_expect_value_with_equals() {
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
fn test_expect_whitespace_handling() {
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
fn test_expect_quoted_with_special_chars() {
    // タブ文字
    let input = "token=\"value\twith\ttabs\"";
    let expect = Expect::parse(input).unwrap();
    assert_eq!(expect.items()[0].value(), Some("value\twith\ttabs"));
}
