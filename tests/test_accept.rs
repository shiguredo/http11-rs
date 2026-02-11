//! Accept 系ヘッダーのユニットテスト

use shiguredo_http11::accept::{
    Accept, AcceptCharset, AcceptEncoding, AcceptError, AcceptLanguage, QValue,
};

// ========================================
// AcceptError のテスト
// ========================================

#[test]
fn test_accept_error_display() {
    let errors = [
        (AcceptError::Empty, "empty Accept header"),
        (AcceptError::InvalidFormat, "invalid Accept header format"),
        (AcceptError::InvalidMediaRange, "invalid media range"),
        (AcceptError::InvalidToken, "invalid token"),
        (AcceptError::InvalidParameter, "invalid parameter"),
        (AcceptError::InvalidQValue, "invalid qvalue"),
        (AcceptError::InvalidLanguageTag, "invalid language tag"),
    ];

    for (error, expected) in errors {
        assert_eq!(error.to_string(), expected);
    }
}

// ========================================
// QValue のテスト
// ========================================

// QValue パース (1)
#[test]
fn test_qvalue_parse_one() {
    let q = QValue::parse("1").unwrap();
    assert_eq!(q.value(), 1000);
    assert!((q.as_f32() - 1.0).abs() < f32::EPSILON);
}

// QValue パース (0)
#[test]
fn test_qvalue_parse_zero() {
    let q = QValue::parse("0").unwrap();
    assert_eq!(q.value(), 0);
    assert!((q.as_f32() - 0.0).abs() < f32::EPSILON);
}

// QValue デフォルト
#[test]
fn test_qvalue_default() {
    let q = QValue::default();
    assert_eq!(q.value(), 1000);
}

// QValue エラーケース
#[test]
fn test_qvalue_parse_errors() {
    // 空
    assert!(QValue::parse("").is_err());

    // 範囲外
    assert!(QValue::parse("1.5").is_err());
    assert!(QValue::parse("2").is_err());

    // 不正な形式
    assert!(QValue::parse("abc").is_err());
    assert!(QValue::parse("-0.5").is_err());

    // 桁数オーバー
    assert!(QValue::parse("0.1234").is_err());
    assert!(QValue::parse("1.0001").is_err());
}

// QValue の比較
#[test]
fn test_qvalue_ordering() {
    let q0 = QValue::parse("0").unwrap();
    let q5 = QValue::parse("0.5").unwrap();
    let q1 = QValue::parse("1").unwrap();

    assert!(q0 < q5);
    assert!(q5 < q1);
    assert!(q0 < q1);
}

// QValue 1.000, 1.00, 1.0 形式
#[test]
fn test_qvalue_one_variants() {
    assert_eq!(QValue::parse("1").unwrap().value(), 1000);
    assert_eq!(QValue::parse("1.").unwrap().value(), 1000);
    assert_eq!(QValue::parse("1.0").unwrap().value(), 1000);
    assert_eq!(QValue::parse("1.00").unwrap().value(), 1000);
    assert_eq!(QValue::parse("1.000").unwrap().value(), 1000);
}

// QValue エッジケース
#[test]
fn test_qvalue_edge_cases() {
    // 境界値
    assert_eq!(QValue::parse("0.001").unwrap().value(), 1);
    assert_eq!(QValue::parse("0.999").unwrap().value(), 999);

    // 省略形式
    assert_eq!(QValue::parse("0.1").unwrap().value(), 100);
    assert_eq!(QValue::parse("0.01").unwrap().value(), 10);
}

// ========================================
// Accept のテスト
// ========================================

// Accept 空値テスト
#[test]
fn test_accept_parse_empty() {
    // RFC 9110 Section 5.6.1.2: 空の値は空リストとして受理する
    let accept = Accept::parse("").unwrap();
    assert!(accept.items().is_empty());
    let accept = Accept::parse("   ").unwrap();
    assert!(accept.items().is_empty());
}

// Accept エラーケース
#[test]
fn test_accept_parse_errors() {
    // 不正なメディアレンジ
    assert!(Accept::parse("text").is_err());
    assert!(Accept::parse("*/html").is_err());

    // 重複 q 値
    assert!(Accept::parse("text/html; q=0.5; q=0.8").is_err());
}

// Accept エッジケース
#[test]
fn test_accept_edge_cases() {
    // 空のパートは無視
    let accept = Accept::parse("text/html, , text/plain").unwrap();
    assert_eq!(accept.items().len(), 2);

    // ワイルドカード
    let accept = Accept::parse("*/*").unwrap();
    assert_eq!(accept.items()[0].media_type(), "*");
    assert_eq!(accept.items()[0].subtype(), "*");

    // サブタイプワイルドカード
    let accept = Accept::parse("text/*").unwrap();
    assert_eq!(accept.items()[0].media_type(), "text");
    assert_eq!(accept.items()[0].subtype(), "*");
}

// Accept 引用符付きパラメータ
#[test]
fn test_accept_quoted_param() {
    // 引用符付きパラメータ
    let accept = Accept::parse("text/html; charset=\"utf-8\"").unwrap();
    let item = &accept.items()[0];
    assert_eq!(item.parameters()[0].1, "utf-8");

    // スペースを含む引用符付きパラメータ
    let accept = Accept::parse("text/html; name=\"hello world\"").unwrap();
    let item = &accept.items()[0];
    assert_eq!(item.parameters()[0].1, "hello world");
}

// ========================================
// AcceptCharset のテスト
// ========================================

// AcceptCharset 空値テスト
#[test]
fn test_accept_charset_parse_empty() {
    // RFC 9110 Section 5.6.1.2: 空の値は空リストとして受理する
    let ac = AcceptCharset::parse("").unwrap();
    assert!(ac.items().is_empty());
}

// AcceptCharset エラーケース
#[test]
fn test_accept_charset_errors() {
    // 不正なパラメータ
    assert!(AcceptCharset::parse("utf-8; invalid").is_err());
}

// ========================================
// AcceptEncoding のテスト
// ========================================

// AcceptEncoding 空値テスト
#[test]
fn test_accept_encoding_parse_empty() {
    // RFC 9110 Section 12.5.3: 空の Accept-Encoding はコンテントコーディング不要を意味する
    let ae = AcceptEncoding::parse("").unwrap();
    assert!(ae.items().is_empty());
}

// ========================================
// AcceptLanguage のテスト
// ========================================

// AcceptLanguage 空値テスト
#[test]
fn test_accept_language_parse_empty() {
    // RFC 9110 Section 5.6.1.2: 空の値は空リストとして受理する
    let al = AcceptLanguage::parse("").unwrap();
    assert!(al.items().is_empty());
}

// AcceptLanguage エラーケース
#[test]
fn test_accept_language_errors() {
    // ワイルドカード単独は許可される
    assert!(AcceptLanguage::parse("*").is_ok());
}

// AcceptLanguage タグバリエーション
#[test]
fn test_accept_language_tag_variants() {
    // 基本言語タグ
    assert!(AcceptLanguage::parse("en").is_ok());

    // 言語-地域
    assert!(AcceptLanguage::parse("en-US").is_ok());

    // 言語-スクリプト-地域
    assert!(AcceptLanguage::parse("zh-Hans-CN").is_ok());

    // 不正なタグ (空のサブタグ)
    assert!(AcceptLanguage::parse("en-").is_err());
    assert!(AcceptLanguage::parse("-US").is_err());
}
