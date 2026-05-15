//! Digest Fields のユニットテスト

use shiguredo_http11::digest_fields::{ContentDigest, DigestFieldsError, WantContentDigest};

// ========================================
// DigestFieldsError のテスト
// ========================================

#[test]
fn test_digest_fields_error_display() {
    let errors = [
        (DigestFieldsError::Empty, "empty digest field"),
        (
            DigestFieldsError::InvalidFormat,
            "invalid digest field format",
        ),
        (
            DigestFieldsError::InvalidAlgorithm,
            "invalid digest algorithm",
        ),
        (
            DigestFieldsError::InvalidByteSequence,
            "invalid digest byte sequence",
        ),
        (DigestFieldsError::InvalidBase64, "invalid digest base64"),
        (
            DigestFieldsError::InvalidPreference,
            "invalid digest preference",
        ),
    ];

    for (error, expected) in errors {
        assert_eq!(error.to_string(), expected);
    }
}

// ========================================
// エラーケースのテスト
// ========================================

#[test]
fn test_content_digest_parse_errors() {
    // 空
    assert!(matches!(
        ContentDigest::parse(""),
        Err(DigestFieldsError::Empty)
    ));

    // 空白のみ
    assert!(matches!(
        ContentDigest::parse("   "),
        Err(DigestFieldsError::Empty)
    ));

    // = がない
    assert!(matches!(
        ContentDigest::parse("sha-256"),
        Err(DigestFieldsError::InvalidFormat)
    ));

    // : がない (byte sequence エラー)
    assert!(matches!(
        ContentDigest::parse("sha-256=YWJj"),
        Err(DigestFieldsError::InvalidByteSequence)
    ));

    // 閉じる : がない
    assert!(matches!(
        ContentDigest::parse("sha-256=:YWJj"),
        Err(DigestFieldsError::InvalidByteSequence)
    ));

    // 不正な Base64
    assert!(matches!(
        ContentDigest::parse("sha-256=:bad*:"),
        Err(DigestFieldsError::InvalidBase64)
    ));

    // アルゴリズムが空
    assert!(matches!(
        ContentDigest::parse("=:YWJj:"),
        Err(DigestFieldsError::InvalidAlgorithm)
    ));

    // 空のパート (カンマの後に何もない) は RFC 9110 Section 5.6.1.2 によりスキップされる
    assert!(ContentDigest::parse("sha-256=:YWJj:,").is_ok());
}

#[test]
fn test_want_digest_parse_errors() {
    // 空
    assert!(matches!(
        WantContentDigest::parse(""),
        Err(DigestFieldsError::Empty)
    ));

    // 優先度が 10 を超える
    assert!(matches!(
        WantContentDigest::parse("sha-256=11"),
        Err(DigestFieldsError::InvalidPreference)
    ));

    // 数値でない優先度
    assert!(matches!(
        WantContentDigest::parse("sha-256=abc"),
        Err(DigestFieldsError::InvalidPreference)
    ));

    // 空の優先度
    assert!(matches!(
        WantContentDigest::parse("sha-256="),
        Err(DigestFieldsError::InvalidPreference)
    ));

    // 不正なアルゴリズム名
    assert!(matches!(
        WantContentDigest::parse("sha@256=5"),
        Err(DigestFieldsError::InvalidAlgorithm)
    ));
}

#[test]
fn test_byte_sequence_trailing_content_error() {
    // : の後に余分な内容がある
    assert!(matches!(
        ContentDigest::parse("sha-256=:YWJj:extra"),
        Err(DigestFieldsError::InvalidByteSequence)
    ));
}

// ========================================
// 特殊ケースのテスト
// ========================================

// 大文字アルゴリズム名 (正規化される)
#[test]
fn test_algorithm_case_normalization() {
    let digest = ContentDigest::parse("SHA-256=:YWJj:").unwrap();
    assert_eq!(digest.items()[0].algorithm(), "sha-256");
    assert!(digest.get("sha-256").is_some());
    assert!(digest.get("SHA-256").is_some());
}

// 空白を含む入力
#[test]
fn test_whitespace_handling() {
    // 前後の空白
    let digest = ContentDigest::parse("  sha-256=:YWJj:  ").unwrap();
    assert_eq!(digest.items().len(), 1);

    // カンマの周りの空白
    let digest = ContentDigest::parse("sha-256=:YWJj: , sha-512=:Zg==:").unwrap();
    assert_eq!(digest.items().len(), 2);
}

// 境界値の優先度
#[test]
fn test_preference_boundary_values() {
    // 最小値
    let want = WantContentDigest::parse("sha-256=0").unwrap();
    assert_eq!(want.get("sha-256"), Some(0));

    // 最大値
    let want = WantContentDigest::parse("sha-256=10").unwrap();
    assert_eq!(want.get("sha-256"), Some(10));
}

/// 末尾カンマを受理 (RFC 9110 Section 5.6.1.2)
#[test]
fn test_digest_trailing_comma_accepted() {
    let digest = ContentDigest::parse("sha-256=:YWJj:,").unwrap();
    assert_eq!(digest.items().len(), 1);
}

/// 先頭カンマを受理 (RFC 9110 Section 5.6.1.2)
#[test]
fn test_digest_leading_comma_accepted() {
    let digest = ContentDigest::parse(",sha-256=:YWJj:").unwrap();
    assert_eq!(digest.items().len(), 1);
}

/// 空要素のみはエラー (RFC 9110 Section 5.6.1.2)
#[test]
fn test_digest_empty_only_error() {
    let result = ContentDigest::parse(",");
    assert!(result.is_err());
}
