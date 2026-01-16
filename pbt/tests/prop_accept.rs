//! Accept 系ヘッダーのプロパティテスト

use proptest::prelude::*;
use shiguredo_http11::accept::{
    Accept, AcceptCharset, AcceptEncoding, AcceptError, AcceptLanguage, QValue,
};

// ========================================
// Strategy 定義
// ========================================

// HTTP トークン文字 (RFC 7230) - 安全な文字のみ使用
fn accept_token_string(max_len: usize) -> impl Strategy<Value = String> {
    "[a-zA-Z0-9._-]{1,8}".prop_filter_map("ensure max length", move |s| {
        if s.len() <= max_len {
            Some(s)
        } else {
            Some(s[..max_len].to_string())
        }
    })
}

fn accept_token_or_star(max_len: usize) -> impl Strategy<Value = String> {
    prop_oneof![Just("*".to_string()), accept_token_string(max_len)]
}

fn accept_media_type_token() -> impl Strategy<Value = String> {
    "[a-z]{1,8}".prop_map(|s| s)
}

fn accept_media_subtype_token() -> impl Strategy<Value = String> {
    "[a-z0-9-]{1,8}".prop_map(|s| s)
}

fn accept_media_range() -> impl Strategy<Value = String> {
    prop_oneof![
        Just("*/*".to_string()),
        (accept_media_type_token(), accept_media_subtype_token())
            .prop_map(|(media_type, subtype)| format!("{}/{}", media_type, subtype)),
        accept_media_type_token().prop_map(|media_type| format!("{}/{}", media_type, "*")),
    ]
}

fn accept_qvalue_string(value: u16) -> String {
    if value >= 1000 {
        return "1".to_string();
    }
    if value == 0 {
        return "0".to_string();
    }

    let mut frac = format!("{:03}", value);
    while frac.ends_with('0') {
        frac.pop();
    }
    format!("0.{}", frac)
}

fn accept_language_subtag() -> impl Strategy<Value = String> {
    "[A-Za-z0-9]{1,8}".prop_map(|s| s)
}

fn accept_language_tag() -> impl Strategy<Value = String> {
    proptest::collection::vec(accept_language_subtag(), 1..=3).prop_map(|parts| parts.join("-"))
}

// 一般的なメディアタイプ
fn common_media_type() -> impl Strategy<Value = &'static str> {
    prop_oneof![
        Just("text"),
        Just("application"),
        Just("image"),
        Just("audio"),
        Just("video"),
        Just("multipart"),
    ]
}

// 一般的なサブタイプ
fn common_subtype() -> impl Strategy<Value = &'static str> {
    prop_oneof![
        Just("html"),
        Just("plain"),
        Just("json"),
        Just("xml"),
        Just("javascript"),
        Just("*"),
    ]
}

// 一般的な charset
fn common_charset() -> impl Strategy<Value = &'static str> {
    prop_oneof![
        Just("utf-8"),
        Just("iso-8859-1"),
        Just("us-ascii"),
        Just("shift_jis"),
    ]
}

// 一般的なエンコーディング
fn common_encoding() -> impl Strategy<Value = &'static str> {
    prop_oneof![
        Just("gzip"),
        Just("deflate"),
        Just("br"),
        Just("identity"),
        Just("*"),
    ]
}

// 一般的な言語タグ
fn common_language() -> impl Strategy<Value = &'static str> {
    prop_oneof![
        Just("en"),
        Just("en-US"),
        Just("ja"),
        Just("ja-JP"),
        Just("de"),
        Just("fr"),
        Just("*"),
    ]
}

// ========================================
// AcceptError のテスト
// ========================================

#[test]
fn accept_error_display() {
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

#[test]
fn accept_error_is_error_trait() {
    let error: Box<dyn std::error::Error> = Box::new(AcceptError::Empty);
    assert_eq!(error.to_string(), "empty Accept header");
}

#[test]
fn accept_error_clone_eq() {
    let error = AcceptError::InvalidQValue;
    let cloned = error.clone();
    assert_eq!(error, cloned);
}

// ========================================
// QValue のテスト
// ========================================

// QValue パース (1)
#[test]
fn qvalue_parse_one() {
    let q = QValue::parse("1").unwrap();
    assert_eq!(q.value(), 1000);
    assert!((q.as_f32() - 1.0).abs() < f32::EPSILON);
}

// QValue パース (0)
#[test]
fn qvalue_parse_zero() {
    let q = QValue::parse("0").unwrap();
    assert_eq!(q.value(), 0);
    assert!((q.as_f32() - 0.0).abs() < f32::EPSILON);
}

// QValue パース (小数)
proptest! {
    #[test]
    fn qvalue_parse_decimal(value in 0u16..=1000u16) {
        let q_str = accept_qvalue_string(value);
        let q = QValue::parse(&q_str).unwrap();
        prop_assert_eq!(q.value(), value);
    }
}

// QValue Display のラウンドトリップ
proptest! {
    #[test]
    fn qvalue_display_roundtrip(value in 0u16..=1000u16) {
        let q_str = accept_qvalue_string(value);
        let q = QValue::parse(&q_str).unwrap();
        let displayed = q.to_string();
        let reparsed = QValue::parse(&displayed).unwrap();
        prop_assert_eq!(q.value(), reparsed.value());
    }
}

// QValue デフォルト
#[test]
fn qvalue_default() {
    let q = QValue::default();
    assert_eq!(q.value(), 1000);
}

// QValue エラーケース
#[test]
fn qvalue_parse_errors() {
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
fn qvalue_ordering() {
    let q0 = QValue::parse("0").unwrap();
    let q5 = QValue::parse("0.5").unwrap();
    let q1 = QValue::parse("1").unwrap();

    assert!(q0 < q5);
    assert!(q5 < q1);
    assert!(q0 < q1);
}

// QValue 1.000, 1.00, 1.0 形式
#[test]
fn qvalue_one_variants() {
    assert_eq!(QValue::parse("1").unwrap().value(), 1000);
    assert_eq!(QValue::parse("1.").unwrap().value(), 1000);
    assert_eq!(QValue::parse("1.0").unwrap().value(), 1000);
    assert_eq!(QValue::parse("1.00").unwrap().value(), 1000);
    assert_eq!(QValue::parse("1.000").unwrap().value(), 1000);
}

// ========================================
// Accept のテスト
// ========================================

// Accept のラウンドトリップ
proptest! {
    #[test]
    fn accept_roundtrip(
        ranges in proptest::collection::vec(accept_media_range(), 1..4),
        qvalues in proptest::collection::vec(0u16..=1000, 1..4)
    ) {
        let mut parts = Vec::new();
        for (idx, range) in ranges.iter().enumerate() {
            let q = qvalues[idx % qvalues.len()];
            let part = if q >= 1000 {
                range.clone()
            } else {
                format!("{}; q={}", range, accept_qvalue_string(q))
            };
            parts.push(part);
        }
        let header = parts.join(", ");
        let parsed = Accept::parse(&header).unwrap();
        let displayed = parsed.to_string();
        let reparsed = Accept::parse(&displayed).unwrap();
        prop_assert_eq!(parsed, reparsed);
    }
}

// Accept 一般的なメディアタイプ
proptest! {
    #[test]
    fn accept_common_types(
        media_type in common_media_type(),
        subtype in common_subtype()
    ) {
        let header = format!("{}/{}", media_type, subtype);
        let result = Accept::parse(&header);
        prop_assert!(result.is_ok());

        let accept = result.unwrap();
        prop_assert_eq!(accept.items().len(), 1);
    }
}

// Accept パラメータ付き
proptest! {
    #[test]
    fn accept_with_params(
        media_type in "[a-z]{1,8}",
        subtype in "[a-z]{1,8}",
        // "q" は予約されているので除外
        param_name in "[a-pr-z]{1,8}",
        param_value in "[a-zA-Z0-9]{1,8}"
    ) {
        let header = format!("{}/{}; {}={}", media_type, subtype, param_name, param_value);
        let result = Accept::parse(&header);
        prop_assert!(result.is_ok());

        let accept = result.unwrap();
        let item = &accept.items()[0];
        prop_assert_eq!(item.parameters().len(), 1);
        prop_assert_eq!(&item.parameters()[0].0, &param_name.to_ascii_lowercase());
        prop_assert_eq!(&item.parameters()[0].1, &param_value);
    }
}

// Accept 複数アイテム
proptest! {
    #[test]
    fn accept_multiple_items(count in 2usize..=5usize) {
        let items: Vec<_> = (0..count).map(|i| format!("text/type{}", i)).collect();
        let header = items.join(", ");
        let accept = Accept::parse(&header).unwrap();
        prop_assert_eq!(accept.items().len(), count);
    }
}

// Accept エラーケース
#[test]
fn accept_parse_errors() {
    // 空
    assert!(matches!(Accept::parse(""), Err(AcceptError::Empty)));
    assert!(matches!(Accept::parse("   "), Err(AcceptError::Empty)));

    // 不正なメディアレンジ
    assert!(Accept::parse("text").is_err());
    assert!(Accept::parse("*/html").is_err());

    // 重複 q 値
    assert!(Accept::parse("text/html; q=0.5; q=0.8").is_err());
}

// Accept アクセサ
proptest! {
    #[test]
    fn accept_item_accessors(
        media_type in "[a-z]{1,8}",
        subtype in "[a-z]{1,8}",
        q in 0u16..=1000u16
    ) {
        let header = if q >= 1000 {
            format!("{}/{}", media_type, subtype)
        } else {
            format!("{}/{}; q={}", media_type, subtype, accept_qvalue_string(q))
        };
        let accept = Accept::parse(&header).unwrap();
        let item = &accept.items()[0];

        prop_assert_eq!(item.media_type(), media_type.as_str());
        prop_assert_eq!(item.subtype(), subtype.as_str());
        prop_assert_eq!(item.qvalue().value(), q);
    }
}

// ========================================
// AcceptCharset のテスト
// ========================================

// AcceptCharset のラウンドトリップ
proptest! {
    #[test]
    fn accept_charset_roundtrip(
        tokens in proptest::collection::vec(accept_token_or_star(8), 1..5),
        qvalues in proptest::collection::vec(0u16..=1000, 1..5)
    ) {
        let mut parts = Vec::new();
        for (idx, token) in tokens.iter().enumerate() {
            let q = qvalues[idx % qvalues.len()];
            let part = if q >= 1000 {
                token.clone()
            } else {
                format!("{}; q={}", token, accept_qvalue_string(q))
            };
            parts.push(part);
        }
        let header = parts.join(", ");
        let parsed = AcceptCharset::parse(&header).unwrap();
        let displayed = parsed.to_string();
        let reparsed = AcceptCharset::parse(&displayed).unwrap();
        prop_assert_eq!(parsed, reparsed);
    }
}

// AcceptCharset 一般的な charset
proptest! {
    #[test]
    fn accept_charset_common(charset in common_charset()) {
        let result = AcceptCharset::parse(charset);
        prop_assert!(result.is_ok());

        let ac = result.unwrap();
        prop_assert_eq!(ac.items().len(), 1);
    }
}

// AcceptCharset アクセサ
proptest! {
    #[test]
    fn accept_charset_accessors(
        charset in "[a-zA-Z0-9-]{1,16}",
        q in 0u16..=1000u16
    ) {
        let header = if q >= 1000 {
            charset.clone()
        } else {
            format!("{}; q={}", charset, accept_qvalue_string(q))
        };
        let ac = AcceptCharset::parse(&header).unwrap();
        let item = &ac.items()[0];

        let expected_charset = charset.to_ascii_lowercase();
        prop_assert_eq!(item.charset(), expected_charset.as_str());
        prop_assert_eq!(item.qvalue().value(), q);
    }
}

// AcceptCharset エラーケース
#[test]
fn accept_charset_errors() {
    assert!(matches!(AcceptCharset::parse(""), Err(AcceptError::Empty)));

    // 不正なパラメータ
    assert!(AcceptCharset::parse("utf-8; invalid").is_err());
}

// ========================================
// AcceptEncoding のテスト
// ========================================

// AcceptEncoding のラウンドトリップ
proptest! {
    #[test]
    fn accept_encoding_roundtrip(
        tokens in proptest::collection::vec(accept_token_string(8), 1..5),
        qvalues in proptest::collection::vec(0u16..=1000, 1..5)
    ) {
        let mut parts = Vec::new();
        for (idx, token) in tokens.iter().enumerate() {
            let q = qvalues[idx % qvalues.len()];
            let part = if q >= 1000 {
                token.clone()
            } else {
                format!("{}; q={}", token, accept_qvalue_string(q))
            };
            parts.push(part);
        }
        let header = parts.join(", ");
        let parsed = AcceptEncoding::parse(&header).unwrap();
        let displayed = parsed.to_string();
        let reparsed = AcceptEncoding::parse(&displayed).unwrap();
        prop_assert_eq!(reparsed.items().len(), parsed.items().len());
    }
}

// AcceptEncoding 一般的なエンコーディング
proptest! {
    #[test]
    fn accept_encoding_common(encoding in common_encoding()) {
        let result = AcceptEncoding::parse(encoding);
        prop_assert!(result.is_ok());

        let ae = result.unwrap();
        prop_assert_eq!(ae.items().len(), 1);
    }
}

// AcceptEncoding アクセサ
proptest! {
    #[test]
    fn accept_encoding_accessors(
        coding in "[a-zA-Z0-9-]{1,16}",
        q in 0u16..=1000u16
    ) {
        let header = if q >= 1000 {
            coding.clone()
        } else {
            format!("{}; q={}", coding, accept_qvalue_string(q))
        };
        let ae = AcceptEncoding::parse(&header).unwrap();
        let item = &ae.items()[0];

        let expected_coding = coding.to_ascii_lowercase();
        prop_assert_eq!(item.coding(), expected_coding.as_str());
        prop_assert_eq!(item.qvalue().value(), q);
    }
}

// AcceptEncoding エラーケース
#[test]
fn accept_encoding_errors() {
    assert!(matches!(AcceptEncoding::parse(""), Err(AcceptError::Empty)));
}

// ========================================
// AcceptLanguage のテスト
// ========================================

// AcceptLanguage のラウンドトリップ
proptest! {
    #[test]
    fn accept_language_roundtrip(
        tags in proptest::collection::vec(accept_language_tag(), 1..4),
        qvalues in proptest::collection::vec(0u16..=1000, 1..4)
    ) {
        let mut parts = Vec::new();
        for (idx, tag) in tags.iter().enumerate() {
            let q = qvalues[idx % qvalues.len()];
            let part = if q >= 1000 {
                tag.clone()
            } else {
                format!("{}; q={}", tag, accept_qvalue_string(q))
            };
            parts.push(part);
        }
        let header = parts.join(", ");
        let parsed = AcceptLanguage::parse(&header).unwrap();
        let displayed = parsed.to_string();
        let reparsed = AcceptLanguage::parse(&displayed).unwrap();
        prop_assert_eq!(reparsed.items().len(), parsed.items().len());
    }
}

// AcceptLanguage 一般的な言語タグ
proptest! {
    #[test]
    fn accept_language_common(language in common_language()) {
        let result = AcceptLanguage::parse(language);
        prop_assert!(result.is_ok());

        let al = result.unwrap();
        prop_assert_eq!(al.items().len(), 1);
    }
}

// AcceptLanguage アクセサ
proptest! {
    #[test]
    fn accept_language_accessors(
        tag in accept_language_tag(),
        q in 0u16..=1000u16
    ) {
        let header = if q >= 1000 {
            tag.clone()
        } else {
            format!("{}; q={}", tag, accept_qvalue_string(q))
        };
        let al = AcceptLanguage::parse(&header).unwrap();
        let item = &al.items()[0];

        prop_assert_eq!(item.language(), tag.as_str());
        prop_assert_eq!(item.qvalue().value(), q);
    }
}

// AcceptLanguage エラーケース
#[test]
fn accept_language_errors() {
    assert!(matches!(AcceptLanguage::parse(""), Err(AcceptError::Empty)));

    // ワイルドカード単独は許可される
    assert!(AcceptLanguage::parse("*").is_ok());
}

// ========================================
// Clone と PartialEq のテスト
// ========================================

proptest! {
    #[test]
    fn accept_clone_eq(media_type in "[a-z]{1,8}", subtype in "[a-z]{1,8}") {
        let header = format!("{}/{}", media_type, subtype);
        let accept = Accept::parse(&header).unwrap();
        let cloned = accept.clone();
        prop_assert_eq!(accept, cloned);
    }
}

proptest! {
    #[test]
    fn accept_charset_clone_eq(charset in "[a-zA-Z0-9-]{1,8}") {
        let ac = AcceptCharset::parse(&charset).unwrap();
        let cloned = ac.clone();
        prop_assert_eq!(ac, cloned);
    }
}

proptest! {
    #[test]
    fn accept_encoding_clone_eq(coding in "[a-zA-Z0-9-]{1,8}") {
        let ae = AcceptEncoding::parse(&coding).unwrap();
        let cloned = ae.clone();
        prop_assert_eq!(ae, cloned);
    }
}

proptest! {
    #[test]
    fn accept_language_clone_eq(tag in accept_language_tag()) {
        let al = AcceptLanguage::parse(&tag).unwrap();
        let cloned = al.clone();
        prop_assert_eq!(al, cloned);
    }
}

// ========================================
// no_panic テスト
// ========================================

proptest! {
    #[test]
    fn accept_parse_no_panic(s in "[ -~]{0,64}") {
        let _ = Accept::parse(&s);
    }
}

proptest! {
    #[test]
    fn accept_charset_parse_no_panic(s in "[ -~]{0,64}") {
        let _ = AcceptCharset::parse(&s);
    }
}

proptest! {
    #[test]
    fn accept_encoding_parse_no_panic(s in "[ -~]{0,64}") {
        let _ = AcceptEncoding::parse(&s);
    }
}

proptest! {
    #[test]
    fn accept_language_parse_no_panic(s in "[ -~]{0,64}") {
        let _ = AcceptLanguage::parse(&s);
    }
}

proptest! {
    #[test]
    fn qvalue_parse_no_panic(s in "[ -~]{0,16}") {
        let _ = QValue::parse(&s);
    }
}

// ========================================
// エッジケースのテスト
// ========================================

#[test]
fn accept_edge_cases() {
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

#[test]
fn accept_quoted_param() {
    // 引用符付きパラメータ
    let accept = Accept::parse("text/html; charset=\"utf-8\"").unwrap();
    let item = &accept.items()[0];
    assert_eq!(item.parameters()[0].1, "utf-8");

    // スペースを含む引用符付きパラメータ
    let accept = Accept::parse("text/html; name=\"hello world\"").unwrap();
    let item = &accept.items()[0];
    assert_eq!(item.parameters()[0].1, "hello world");
}

#[test]
fn accept_language_tag_variants() {
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

#[test]
fn qvalue_edge_cases() {
    // 境界値
    assert_eq!(QValue::parse("0.001").unwrap().value(), 1);
    assert_eq!(QValue::parse("0.999").unwrap().value(), 999);

    // 省略形式
    assert_eq!(QValue::parse("0.1").unwrap().value(), 100);
    assert_eq!(QValue::parse("0.01").unwrap().value(), 10);
}

// ========================================
// MediaRange Display テスト
// ========================================

proptest! {
    #[test]
    fn media_range_display_roundtrip(
        media_type in "[a-z]{1,8}",
        subtype in "[a-z]{1,8}",
        q in 0u16..=1000u16
    ) {
        let header = if q >= 1000 {
            format!("{}/{}", media_type, subtype)
        } else {
            format!("{}/{}; q={}", media_type, subtype, accept_qvalue_string(q))
        };
        let accept = Accept::parse(&header).unwrap();
        let item = &accept.items()[0];
        let displayed = item.to_string();

        // Display からパースできる
        let reparsed = Accept::parse(&displayed).unwrap();
        let reparsed_item = &reparsed.items()[0];

        prop_assert_eq!(item.media_type(), reparsed_item.media_type());
        prop_assert_eq!(item.subtype(), reparsed_item.subtype());
        prop_assert_eq!(item.qvalue().value(), reparsed_item.qvalue().value());
    }
}

// ========================================
// 空白処理テスト
// ========================================

proptest! {
    #[test]
    fn accept_trim_whitespace(media_type in "[a-z]{1,8}", subtype in "[a-z]{1,8}") {
        let header = format!("  {}/{}  ", media_type, subtype);
        let result = Accept::parse(&header);
        prop_assert!(result.is_ok());
    }
}

proptest! {
    #[test]
    fn accept_whitespace_around_params(
        media_type in "[a-z]{1,8}",
        subtype in "[a-z]{1,8}",
        q in 0u16..999u16
    ) {
        let header = format!(
            "{}  /  {}  ;  q  =  {}",
            media_type, subtype, accept_qvalue_string(q)
        );
        let result = Accept::parse(&header);
        prop_assert!(result.is_ok());
    }
}
