//! その他ヘッダーのプロパティテスト

use proptest::prelude::*;
use shiguredo_http11::content_encoding::ContentEncoding;
use shiguredo_http11::content_language::ContentLanguage;
use shiguredo_http11::content_location::ContentLocation;
use shiguredo_http11::digest_fields::{
    ContentDigest, ReprDigest, WantContentDigest, WantReprDigest,
};
use shiguredo_http11::expect::Expect;
use shiguredo_http11::host::Host;
use shiguredo_http11::trailer::Trailer;
use shiguredo_http11::upgrade::Upgrade;
use shiguredo_http11::vary::Vary;

// ========================================
// Strategy 定義
// ========================================

// HTTP トークン文字 (RFC 7230)
fn header_misc_token_char() -> impl Strategy<Value = char> {
    prop_oneof![
        prop::char::range('a', 'z'),
        prop::char::range('A', 'Z'),
        prop::char::range('0', '9'),
        Just('-'),
        Just('_'),
        Just('.'),
    ]
}

fn header_misc_token_string(max_len: usize) -> impl Strategy<Value = String> {
    proptest::collection::vec(header_misc_token_char(), 1..=max_len)
        .prop_map(|chars| chars.into_iter().collect())
}

fn header_misc_expect_token() -> impl Strategy<Value = String> {
    prop_oneof![
        Just("100-continue".to_string()),
        header_misc_token_string(12),
    ]
}

fn header_misc_digest_algorithm_token() -> impl Strategy<Value = String> {
    "[a-z0-9-]{1,16}".prop_map(|s| s)
}

// 言語タグ生成は pbt クレートを使用
use pbt::language_tag as header_misc_language_tag;

// URI (スペースや CRLF を含まない)
fn header_misc_http_uri() -> impl Strategy<Value = String> {
    prop_oneof![
        Just("/".to_string()),
        "/[a-zA-Z0-9/_.-]{1,64}".prop_map(|s| s),
        "rtsp://[a-z.]{1,32}/[a-z/]{1,32}".prop_map(|s| s),
    ]
}

fn header_misc_content_location_value() -> impl Strategy<Value = String> {
    prop_oneof![
        header_misc_http_uri(),
        (
            "[a-z]{1,8}",
            "[a-z0-9-]{1,8}\\.[a-z]{2,4}",
            "/[a-zA-Z0-9/_-]{0,16}"
        )
            .prop_map(|(scheme, host, path)| format!("{}://{}{}", scheme, host, path)),
    ]
}

fn header_misc_host_value() -> impl Strategy<Value = String> {
    (
        "[a-z0-9-]{1,12}\\.[a-z]{2,4}",
        prop::option::of(1u16..=65535),
    )
        .prop_map(|(host, port)| match port {
            Some(port) => format!("{}:{}", host, port),
            None => host,
        })
}

fn header_misc_base64_encode(input: &[u8]) -> String {
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

// Content-Encoding のラウンドトリップ
proptest! {
    #[test]
    fn prop_content_encoding_roundtrip(tokens in proptest::collection::vec(header_misc_token_string(8), 1..5)) {
        let header = tokens.join(", ");
        let parsed = ContentEncoding::parse(&header).unwrap();
        let displayed = parsed.to_string();
        let reparsed = ContentEncoding::parse(&displayed).unwrap();
        prop_assert_eq!(reparsed.encodings().len(), parsed.encodings().len());
    }
}

// Content-Language のラウンドトリップ
proptest! {
    #[test]
    fn prop_content_language_roundtrip(tags in proptest::collection::vec(header_misc_language_tag(), 1..4)) {
        let header = tags.join(", ");
        let parsed = ContentLanguage::parse(&header).unwrap();
        let displayed = parsed.to_string();
        let reparsed = ContentLanguage::parse(&displayed).unwrap();
        prop_assert_eq!(parsed, reparsed);
    }
}

// Content-Location のラウンドトリップ
proptest! {
    #[test]
    fn prop_content_location_roundtrip(value in header_misc_content_location_value()) {
        let parsed = ContentLocation::parse(&value).unwrap();
        let displayed = parsed.to_string();
        let reparsed = ContentLocation::parse(&displayed).unwrap();
        prop_assert_eq!(parsed, reparsed);
    }
}

// Host のラウンドトリップ
proptest! {
    #[test]
    fn prop_host_roundtrip(value in header_misc_host_value()) {
        let parsed = Host::parse(&value).unwrap();
        let displayed = parsed.to_string();
        let reparsed = Host::parse(&displayed).unwrap();
        prop_assert_eq!(parsed, reparsed);
    }
}

// Trailer のラウンドトリップ
proptest! {
    #[test]
    fn prop_trailer_roundtrip(tokens in proptest::collection::vec(header_misc_token_string(8), 1..5)) {
        let header = tokens.join(", ");
        let parsed = Trailer::parse(&header).unwrap();
        let displayed = parsed.to_string();
        let reparsed = Trailer::parse(&displayed).unwrap();
        prop_assert_eq!(parsed, reparsed);
    }
}

// Vary のラウンドトリップ
proptest! {
    #[test]
    fn prop_vary_roundtrip(value in prop_oneof![Just("*".to_string()), proptest::collection::vec(header_misc_token_string(8), 1..5).prop_map(|tokens| tokens.join(", "))]) {
        let parsed = Vary::parse(&value).unwrap();
        let displayed = parsed.to_string();
        let reparsed = Vary::parse(&displayed).unwrap();
        prop_assert_eq!(parsed, reparsed);
    }
}

// Expect のラウンドトリップ
proptest! {
    #[test]
    fn prop_expect_roundtrip(items in proptest::collection::vec((header_misc_expect_token(), prop::option::of(header_misc_token_string(8))), 1..4)) {
        let mut parts = Vec::new();
        let mut expected_has_100 = false;

        for (token, value) in items {
            if token.eq_ignore_ascii_case("100-continue") {
                expected_has_100 = true;
            }
            let part = match value {
                Some(value) => format!("{}={}", token, value),
                None => token,
            };
            parts.push(part);
        }

        let header = parts.join(", ");
        let parsed = Expect::parse(&header).unwrap();
        let displayed = parsed.to_string();
        let reparsed = Expect::parse(&displayed).unwrap();
        prop_assert_eq!(&parsed, &reparsed);
        prop_assert_eq!(parsed.has_100_continue(), expected_has_100);
    }
}

// Upgrade のラウンドトリップ
proptest! {
    #[test]
    fn prop_upgrade_roundtrip(items in proptest::collection::vec((header_misc_token_string(8), prop::option::of(header_misc_token_string(8))), 1..4)) {
        let first_protocol = items[0].0.clone();
        let mut parts = Vec::new();

        for (protocol, version) in items {
            let part = match version {
                Some(version) => format!("{}/{}", protocol, version),
                None => protocol,
            };
            parts.push(part);
        }

        let header = parts.join(", ");
        let parsed = Upgrade::parse(&header).unwrap();
        let displayed = parsed.to_string();
        let reparsed = Upgrade::parse(&displayed).unwrap();
        prop_assert_eq!(&parsed, &reparsed);
        prop_assert!(parsed.has_protocol(&first_protocol));
    }
}

// Digest Fields のラウンドトリップ
proptest! {
    #[test]
    fn prop_content_digest_roundtrip(entries in proptest::collection::vec((header_misc_digest_algorithm_token(), proptest::collection::vec(any::<u8>(), 0..32)), 1..4)) {
        let mut parts = Vec::new();
        for (algorithm, bytes) in entries {
            let encoded = header_misc_base64_encode(&bytes);
            parts.push(format!("{}=:{}:", algorithm, encoded));
        }
        let header = parts.join(", ");
        let parsed = ContentDigest::parse(&header).unwrap();
        let displayed = parsed.to_string();
        let reparsed = ContentDigest::parse(&displayed).unwrap();
        prop_assert_eq!(parsed, reparsed);
    }
}

proptest! {
    #[test]
    fn prop_repr_digest_roundtrip(entries in proptest::collection::vec((header_misc_digest_algorithm_token(), proptest::collection::vec(any::<u8>(), 0..32)), 1..4)) {
        let mut parts = Vec::new();
        for (algorithm, bytes) in entries {
            let encoded = header_misc_base64_encode(&bytes);
            parts.push(format!("{}=:{}:", algorithm, encoded));
        }
        let header = parts.join(", ");
        let parsed = ReprDigest::parse(&header).unwrap();
        let displayed = parsed.to_string();
        let reparsed = ReprDigest::parse(&displayed).unwrap();
        prop_assert_eq!(parsed, reparsed);
    }
}

proptest! {
    #[test]
    fn prop_want_content_digest_roundtrip(entries in proptest::collection::vec((header_misc_digest_algorithm_token(), 0u8..=10), 1..4)) {
        let header = entries
            .iter()
            .map(|(algorithm, weight)| format!("{}={}", algorithm, weight))
            .collect::<Vec<_>>()
            .join(", ");
        let parsed = WantContentDigest::parse(&header).unwrap();
        let displayed = parsed.to_string();
        let reparsed = WantContentDigest::parse(&displayed).unwrap();
        prop_assert_eq!(parsed, reparsed);
    }
}

proptest! {
    #[test]
    fn prop_want_repr_digest_roundtrip(entries in proptest::collection::vec((header_misc_digest_algorithm_token(), 0u8..=10), 1..4)) {
        let header = entries
            .iter()
            .map(|(algorithm, weight)| format!("{}={}", algorithm, weight))
            .collect::<Vec<_>>()
            .join(", ");
        let parsed = WantReprDigest::parse(&header).unwrap();
        let displayed = parsed.to_string();
        let reparsed = WantReprDigest::parse(&displayed).unwrap();
        prop_assert_eq!(parsed, reparsed);
    }
}

// 追加ヘッダーのパースがパニックしない
proptest! {
    #[test]
    fn prop_extra_header_parse_no_panic(s in "[ -~]{0,64}") {
        let _ = ContentLanguage::parse(&s);
        let _ = ContentLocation::parse(&s);
        let _ = Host::parse(&s);
        let _ = Trailer::parse(&s);
        let _ = Vary::parse(&s);
        let _ = Expect::parse(&s);
        let _ = Upgrade::parse(&s);
        let _ = ContentDigest::parse(&s);
        let _ = ReprDigest::parse(&s);
        let _ = WantContentDigest::parse(&s);
        let _ = WantReprDigest::parse(&s);
    }
}
