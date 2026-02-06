//! Digest Fields のプロパティテスト (RFC 9530)

use proptest::prelude::*;
use shiguredo_http11::digest_fields::{
    ContentDigest, ReprDigest, WantContentDigest, WantReprDigest,
};

// ========================================
// Strategy 定義
// ========================================

// 有効なアルゴリズム名
fn valid_algorithm() -> impl Strategy<Value = String> {
    prop_oneof![
        Just("sha-256".to_string()),
        Just("sha-512".to_string()),
        Just("sha-384".to_string()),
        Just("md5".to_string()),
        Just("unixsum".to_string()),
        Just("unixcksum".to_string()),
        Just("adler32".to_string()),
        Just("crc32c".to_string()),
    ]
}

// 有効な優先度 (0-10)
fn valid_weight() -> impl Strategy<Value = u8> {
    0u8..=10
}

// 任意のバイト列 (digest 値)
fn digest_bytes() -> impl Strategy<Value = Vec<u8>> {
    proptest::collection::vec(any::<u8>(), 1..64)
}

// Base64 エンコード用の関数 (テスト用)
fn base64_encode(input: &[u8]) -> String {
    const BASE64_ALPHABET: &[u8; 64] =
        b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

    let mut result = String::new();
    let mut i = 0;

    while i < input.len() {
        let b0 = input[i];
        let b1 = if i + 1 < input.len() { input[i + 1] } else { 0 };
        let b2 = if i + 2 < input.len() { input[i + 2] } else { 0 };

        let n = ((b0 as u32) << 16) | ((b1 as u32) << 8) | (b2 as u32);

        result.push(BASE64_ALPHABET[(n >> 18 & 0x3F) as usize] as char);
        result.push(BASE64_ALPHABET[(n >> 12 & 0x3F) as usize] as char);

        if i + 1 < input.len() {
            result.push(BASE64_ALPHABET[(n >> 6 & 0x3F) as usize] as char);
        } else {
            result.push('=');
        }

        if i + 2 < input.len() {
            result.push(BASE64_ALPHABET[(n & 0x3F) as usize] as char);
        } else {
            result.push('=');
        }

        i += 3;
    }

    result
}

// ========================================
// ContentDigest のテスト
// ========================================

// 単一のダイジェストパース
proptest! {
    #[test]
    fn prop_content_digest_parse_single(
        algorithm in valid_algorithm(),
        data in digest_bytes()
    ) {
        let b64 = base64_encode(&data);
        let input = format!("{}=:{}:", algorithm, b64);

        let digest = ContentDigest::parse(&input).unwrap();
        prop_assert_eq!(digest.items().len(), 1);
        prop_assert_eq!(digest.items()[0].algorithm(), algorithm.as_str());
    }
}

// 複数のダイジェストパース
proptest! {
    #[test]
    fn prop_content_digest_parse_multiple(
        data1 in digest_bytes(),
        data2 in digest_bytes()
    ) {
        let b64_1 = base64_encode(&data1);
        let b64_2 = base64_encode(&data2);
        let input = format!("sha-256=:{}:, sha-512=:{}:", b64_1, b64_2);

        let digest = ContentDigest::parse(&input).unwrap();
        prop_assert_eq!(digest.items().len(), 2);
        prop_assert_eq!(digest.items()[0].algorithm(), "sha-256");
        prop_assert_eq!(digest.items()[1].algorithm(), "sha-512");
    }
}

// get メソッド
proptest! {
    #[test]
    fn prop_content_digest_get(
        algorithm in valid_algorithm(),
        data in digest_bytes()
    ) {
        let b64 = base64_encode(&data);
        let input = format!("{}=:{}:", algorithm, b64);

        let digest = ContentDigest::parse(&input).unwrap();

        // 大文字小文字を無視して取得
        prop_assert!(digest.get(&algorithm).is_some());
        prop_assert!(digest.get(&algorithm.to_uppercase()).is_some());

        // 存在しないアルゴリズム
        prop_assert!(digest.get("nonexistent").is_none());
    }
}

// Display ラウンドトリップ
proptest! {
    #[test]
    fn prop_content_digest_display_roundtrip(data in digest_bytes()) {
        let b64 = base64_encode(&data);
        let input = format!("sha-256=:{}:", b64);

        let digest = ContentDigest::parse(&input).unwrap();
        let display = digest.to_string();

        // 再パース可能
        let reparsed = ContentDigest::parse(&display);
        prop_assert!(reparsed.is_ok());
    }
}

// DigestEntry の Display
proptest! {
    #[test]
    fn prop_digest_entry_display(
        algorithm in valid_algorithm(),
        data in digest_bytes()
    ) {
        let b64 = base64_encode(&data);
        let input = format!("{}=:{}:", algorithm, b64);

        let digest = ContentDigest::parse(&input).unwrap();
        let entry_display = digest.items()[0].to_string();

        prop_assert!(entry_display.contains("=:"));
        prop_assert!(entry_display.contains(":"));
    }
}

// DigestValue::bytes
proptest! {
    #[test]
    fn prop_digest_value_bytes(data in digest_bytes()) {
        let b64 = base64_encode(&data);
        let input = format!("sha-256=:{}:", b64);

        let digest = ContentDigest::parse(&input).unwrap();
        let value = digest.items()[0].value();

        prop_assert_eq!(value.bytes(), data.as_slice());
    }
}

// DigestValue の Display
proptest! {
    #[test]
    fn prop_digest_value_display(data in digest_bytes()) {
        let b64 = base64_encode(&data);
        let input = format!("sha-256=:{}:", b64);

        let digest = ContentDigest::parse(&input).unwrap();
        let value_display = digest.items()[0].value().to_string();

        prop_assert!(value_display.starts_with(':'));
        prop_assert!(value_display.ends_with(':'));
    }
}

// ========================================
// ReprDigest のテスト
// ========================================

// 単一のダイジェストパース
proptest! {
    #[test]
    fn prop_repr_digest_parse_single(
        algorithm in valid_algorithm(),
        data in digest_bytes()
    ) {
        let b64 = base64_encode(&data);
        let input = format!("{}=:{}:", algorithm, b64);

        let digest = ReprDigest::parse(&input).unwrap();
        prop_assert_eq!(digest.items().len(), 1);
        prop_assert_eq!(digest.items()[0].algorithm(), algorithm.as_str());
    }
}

// 複数のダイジェストパース
proptest! {
    #[test]
    fn prop_repr_digest_parse_multiple(
        data1 in digest_bytes(),
        data2 in digest_bytes()
    ) {
        let b64_1 = base64_encode(&data1);
        let b64_2 = base64_encode(&data2);
        let input = format!("sha-256=:{}:, sha-512=:{}:", b64_1, b64_2);

        let digest = ReprDigest::parse(&input).unwrap();
        prop_assert_eq!(digest.items().len(), 2);
    }
}

// get メソッド
proptest! {
    #[test]
    fn prop_repr_digest_get(
        algorithm in valid_algorithm(),
        data in digest_bytes()
    ) {
        let b64 = base64_encode(&data);
        let input = format!("{}=:{}:", algorithm, b64);

        let digest = ReprDigest::parse(&input).unwrap();
        prop_assert!(digest.get(&algorithm).is_some());
        prop_assert!(digest.get("nonexistent").is_none());
    }
}

// Display ラウンドトリップ
proptest! {
    #[test]
    fn prop_repr_digest_display_roundtrip(data in digest_bytes()) {
        let b64 = base64_encode(&data);
        let input = format!("sha-256=:{}:", b64);

        let digest = ReprDigest::parse(&input).unwrap();
        let display = digest.to_string();

        let reparsed = ReprDigest::parse(&display);
        prop_assert!(reparsed.is_ok());
    }
}

// ========================================
// WantContentDigest のテスト
// ========================================

// 単一の優先度パース
proptest! {
    #[test]
    fn prop_want_content_digest_parse_single(
        algorithm in valid_algorithm(),
        weight in valid_weight()
    ) {
        let input = format!("{}={}", algorithm, weight);

        let want = WantContentDigest::parse(&input).unwrap();
        prop_assert_eq!(want.items().len(), 1);
        prop_assert_eq!(want.items()[0].algorithm(), algorithm.as_str());
        prop_assert_eq!(want.items()[0].weight(), weight);
    }
}

// 複数の優先度パース
proptest! {
    #[test]
    fn prop_want_content_digest_parse_multiple(
        weight1 in valid_weight(),
        weight2 in valid_weight()
    ) {
        let input = format!("sha-256={}, sha-512={}", weight1, weight2);

        let want = WantContentDigest::parse(&input).unwrap();
        prop_assert_eq!(want.items().len(), 2);
        prop_assert_eq!(want.items()[0].weight(), weight1);
        prop_assert_eq!(want.items()[1].weight(), weight2);
    }
}

// get メソッド
proptest! {
    #[test]
    fn prop_want_content_digest_get(
        algorithm in valid_algorithm(),
        weight in valid_weight()
    ) {
        let input = format!("{}={}", algorithm, weight);

        let want = WantContentDigest::parse(&input).unwrap();
        prop_assert_eq!(want.get(&algorithm), Some(weight));
        prop_assert_eq!(want.get(&algorithm.to_uppercase()), Some(weight));
        prop_assert!(want.get("nonexistent").is_none());
    }
}

// Display ラウンドトリップ
proptest! {
    #[test]
    fn prop_want_content_digest_display_roundtrip(
        algorithm in valid_algorithm(),
        weight in valid_weight()
    ) {
        let input = format!("{}={}", algorithm, weight);

        let want = WantContentDigest::parse(&input).unwrap();
        let display = want.to_string();

        let reparsed = WantContentDigest::parse(&display);
        prop_assert!(reparsed.is_ok());
    }
}

// DigestPreference の Display
proptest! {
    #[test]
    fn prop_digest_preference_display(
        algorithm in valid_algorithm(),
        weight in valid_weight()
    ) {
        let input = format!("{}={}", algorithm, weight);

        let want = WantContentDigest::parse(&input).unwrap();
        let pref_display = want.items()[0].to_string();

        prop_assert!(pref_display.contains('='));
    }
}

// ========================================
// WantReprDigest のテスト
// ========================================

// 単一の優先度パース
proptest! {
    #[test]
    fn prop_want_repr_digest_parse_single(
        algorithm in valid_algorithm(),
        weight in valid_weight()
    ) {
        let input = format!("{}={}", algorithm, weight);

        let want = WantReprDigest::parse(&input).unwrap();
        prop_assert_eq!(want.items().len(), 1);
        prop_assert_eq!(want.items()[0].algorithm(), algorithm.as_str());
        prop_assert_eq!(want.items()[0].weight(), weight);
    }
}

// 複数の優先度パース
proptest! {
    #[test]
    fn prop_want_repr_digest_parse_multiple(
        weight1 in valid_weight(),
        weight2 in valid_weight()
    ) {
        let input = format!("sha-256={}, sha-512={}", weight1, weight2);

        let want = WantReprDigest::parse(&input).unwrap();
        prop_assert_eq!(want.items().len(), 2);
    }
}

// get メソッド
proptest! {
    #[test]
    fn prop_want_repr_digest_get(
        algorithm in valid_algorithm(),
        weight in valid_weight()
    ) {
        let input = format!("{}={}", algorithm, weight);

        let want = WantReprDigest::parse(&input).unwrap();
        prop_assert_eq!(want.get(&algorithm), Some(weight));
        prop_assert!(want.get("nonexistent").is_none());
    }
}

// Display ラウンドトリップ
proptest! {
    #[test]
    fn prop_want_repr_digest_display_roundtrip(
        algorithm in valid_algorithm(),
        weight in valid_weight()
    ) {
        let input = format!("{}={}", algorithm, weight);

        let want = WantReprDigest::parse(&input).unwrap();
        let display = want.to_string();

        let reparsed = WantReprDigest::parse(&display);
        prop_assert!(reparsed.is_ok());
    }
}

// ========================================
// Clone と PartialEq のテスト
// ========================================

proptest! {
    #[test]
    fn prop_content_digest_clone_eq(data in digest_bytes()) {
        let b64 = base64_encode(&data);
        let input = format!("sha-256=:{}:", b64);

        let digest = ContentDigest::parse(&input).unwrap();
        let cloned = digest.clone();

        prop_assert_eq!(digest, cloned);
    }
}

proptest! {
    #[test]
    fn prop_repr_digest_clone_eq(data in digest_bytes()) {
        let b64 = base64_encode(&data);
        let input = format!("sha-256=:{}:", b64);

        let digest = ReprDigest::parse(&input).unwrap();
        let cloned = digest.clone();

        prop_assert_eq!(digest, cloned);
    }
}

proptest! {
    #[test]
    fn prop_want_content_digest_clone_eq(weight in valid_weight()) {
        let input = format!("sha-256={}", weight);

        let want = WantContentDigest::parse(&input).unwrap();
        let cloned = want.clone();

        prop_assert_eq!(want, cloned);
    }
}

proptest! {
    #[test]
    fn prop_want_repr_digest_clone_eq(weight in valid_weight()) {
        let input = format!("sha-256={}", weight);

        let want = WantReprDigest::parse(&input).unwrap();
        let cloned = want.clone();

        prop_assert_eq!(want, cloned);
    }
}

// ========================================
// no_panic テスト
// ========================================

proptest! {
    #[test]
    fn prop_content_digest_parse_no_panic(s in "[ -~]{0,128}") {
        let _ = ContentDigest::parse(&s);
    }
}

proptest! {
    #[test]
    fn prop_repr_digest_parse_no_panic(s in "[ -~]{0,128}") {
        let _ = ReprDigest::parse(&s);
    }
}

proptest! {
    #[test]
    fn prop_want_content_digest_parse_no_panic(s in "[ -~]{0,128}") {
        let _ = WantContentDigest::parse(&s);
    }
}

proptest! {
    #[test]
    fn prop_want_repr_digest_parse_no_panic(s in "[ -~]{0,128}") {
        let _ = WantReprDigest::parse(&s);
    }
}
