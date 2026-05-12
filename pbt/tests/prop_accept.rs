//! Accept 系ヘッダーのプロパティテスト

use proptest::prelude::*;
use shiguredo_http11::accept::{Accept, AcceptCharset, AcceptEncoding, AcceptLanguage, QValue};

// ========================================
// Strategy 定義
// ========================================

// HTTP トークン文字 (RFC 9110 Section 5.6.2) - 安全な文字のみ使用
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

// 言語タグ生成は pbt クレートを使用
use pbt::language_tag as accept_language_tag;

// ========================================
// QValue のテスト
// ========================================

// QValue パース (小数)
proptest! {
    #[test]
    fn prop_qvalue_parse_decimal(value in 0u16..=1000u16) {
        let q_str = accept_qvalue_string(value);
        let q = QValue::parse(&q_str).unwrap();
        prop_assert_eq!(q.value(), value);
    }
}

// QValue Display のラウンドトリップ
proptest! {
    #[test]
    fn prop_qvalue_display_roundtrip(value in 0u16..=1000u16) {
        let q_str = accept_qvalue_string(value);
        let q = QValue::parse(&q_str).unwrap();
        let displayed = q.to_string();
        let reparsed = QValue::parse(&displayed).unwrap();
        prop_assert_eq!(q.value(), reparsed.value());
    }
}

// ========================================
// Accept のテスト
// ========================================

// Accept のラウンドトリップ
proptest! {
    #[test]
    fn prop_accept_roundtrip(
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

// Accept パラメータ付き
proptest! {
    #[test]
    fn prop_accept_with_params(
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
    fn prop_accept_multiple_items(count in 2usize..=5usize) {
        let items: Vec<_> = (0..count).map(|i| format!("text/type{}", i)).collect();
        let header = items.join(", ");
        let accept = Accept::parse(&header).unwrap();
        prop_assert_eq!(accept.items().len(), count);
    }
}

// Accept アクセサ
proptest! {
    #[test]
    fn prop_accept_item_accessors(
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
    fn prop_accept_charset_roundtrip(
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

// AcceptCharset アクセサ
proptest! {
    #[test]
    fn prop_accept_charset_accessors(
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

// ========================================
// AcceptEncoding のテスト
// ========================================

// AcceptEncoding のラウンドトリップ
proptest! {
    #[test]
    fn prop_accept_encoding_roundtrip(
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

// AcceptEncoding アクセサ
proptest! {
    #[test]
    fn prop_accept_encoding_accessors(
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

// ========================================
// AcceptLanguage のテスト
// ========================================

// AcceptLanguage のラウンドトリップ
proptest! {
    #[test]
    fn prop_accept_language_roundtrip(
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

// AcceptLanguage アクセサ
proptest! {
    #[test]
    fn prop_accept_language_accessors(
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

// ========================================
// MediaRange Display テスト
// ========================================

proptest! {
    #[test]
    fn prop_media_range_display_roundtrip(
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
