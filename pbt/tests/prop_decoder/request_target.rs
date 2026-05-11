//! request-target 形式のプロパティテスト (RFC 9112 Section 3.2)

use proptest::prelude::*;
use shiguredo_http11::RequestDecoder;

// ========================================
// Strategy 定義
// ========================================

// パス用文字 (RFC 3986 pchar + "/")
const PATH_CHARS: &[char] = &[
    'a', 'b', 'c', 'd', 'e', 'f', 'g', 'h', 'i', 'j', 'k', 'l', 'm', 'n', 'o', 'p', 'q', 'r', 's',
    't', 'u', 'v', 'w', 'x', 'y', 'z', 'A', 'B', 'C', 'D', 'E', 'F', 'G', 'H', 'I', 'J', 'K', 'L',
    'M', 'N', 'O', 'P', 'Q', 'R', 'S', 'T', 'U', 'V', 'W', 'X', 'Y', 'Z', '0', '1', '2', '3', '4',
    '5', '6', '7', '8', '9', '-', '.', '_', '~', '/', ':', '@', '!', '$', '&', '\'', '(', ')', '*',
    '+', ',', ';', '=',
];

fn path_char() -> impl Strategy<Value = char> {
    prop::sample::select(PATH_CHARS)
}

fn path_segment() -> impl Strategy<Value = String> {
    proptest::collection::vec(path_char(), 1..32).prop_map(|chars| chars.into_iter().collect())
}

fn origin_form_uri() -> impl Strategy<Value = String> {
    prop_oneof![
        Just("/".to_string()),
        path_segment().prop_map(|s| format!("/{}", s)),
        (path_segment(), path_segment()).prop_map(|(a, b)| format!("/{}/{}", a, b)),
    ]
}

// クエリ付き origin-form
fn origin_form_with_query() -> impl Strategy<Value = String> {
    (origin_form_uri(), path_segment()).prop_map(|(path, query)| format!("{}?{}", path, query))
}

// ホスト名
fn hostname() -> impl Strategy<Value = String> {
    proptest::collection::vec(prop::char::range('a', 'z'), 1..16)
        .prop_map(|chars| chars.into_iter().collect::<String>())
        .prop_map(|s| format!("{}.com", s))
}

// absolute-form URI
fn absolute_form_uri() -> impl Strategy<Value = String> {
    (hostname(), origin_form_uri()).prop_map(|(host, path)| format!("http://{}{}", host, path))
}

// authority-form (host:port)
fn authority_form_uri() -> impl Strategy<Value = String> {
    (hostname(), 1..=65535u16).prop_map(|(host, port)| format!("{}:{}", host, port))
}

// ========================================
// origin-form テスト
// ========================================

proptest! {
    #[test]
    fn prop_origin_form_with_get_succeeds(uri in origin_form_uri()) {
        let request_line = format!("GET {} HTTP/1.1\r\nHost: example.com\r\n\r\n", uri);
        let mut decoder = RequestDecoder::new();
        decoder.feed(request_line.as_bytes()).unwrap();
        let result = decoder.decode_headers();
        prop_assert!(result.is_ok(), "GET with origin-form should succeed: {}", uri);
        prop_assert!(result.unwrap().is_some());
    }

    #[test]
    fn prop_origin_form_with_query_succeeds(uri in origin_form_with_query()) {
        let request_line = format!("GET {} HTTP/1.1\r\nHost: example.com\r\n\r\n", uri);
        let mut decoder = RequestDecoder::new();
        decoder.feed(request_line.as_bytes()).unwrap();
        let result = decoder.decode_headers();
        prop_assert!(result.is_ok(), "GET with origin-form?query should succeed: {}", uri);
        prop_assert!(result.unwrap().is_some());
    }

    #[test]
    fn prop_origin_form_with_post_succeeds(uri in origin_form_uri()) {
        let request_line = format!("POST {} HTTP/1.1\r\nHost: example.com\r\n\r\n", uri);
        let mut decoder = RequestDecoder::new();
        decoder.feed(request_line.as_bytes()).unwrap();
        let result = decoder.decode_headers();
        prop_assert!(result.is_ok(), "POST with origin-form should succeed: {}", uri);
        prop_assert!(result.unwrap().is_some());
    }
}

// ========================================
// absolute-form テスト
// ========================================

proptest! {
    #[test]
    fn prop_absolute_form_with_get_succeeds(uri in absolute_form_uri()) {
        let request_line = format!("GET {} HTTP/1.1\r\nHost: example.com\r\n\r\n", uri);
        let mut decoder = RequestDecoder::new();
        decoder.feed(request_line.as_bytes()).unwrap();
        let result = decoder.decode_headers();
        prop_assert!(result.is_ok(), "GET with absolute-form should succeed: {}", uri);
        prop_assert!(result.unwrap().is_some());
    }
}

// ========================================
// authority-form テスト
// ========================================

proptest! {
    #[test]
    fn prop_authority_form_with_connect_succeeds(uri in authority_form_uri()) {
        let request_line = format!("CONNECT {} HTTP/1.1\r\nHost: {}\r\n\r\n", uri, uri);
        let mut decoder = RequestDecoder::new();
        decoder.feed(request_line.as_bytes()).unwrap();
        let result = decoder.decode_headers();
        prop_assert!(result.is_ok(), "CONNECT with authority-form should succeed: {}", uri);
        prop_assert!(result.unwrap().is_some());
    }

    #[test]
    fn prop_authority_form_with_get_fails(uri in authority_form_uri()) {
        let request_line = format!("GET {} HTTP/1.1\r\nHost: example.com\r\n\r\n", uri);
        let mut decoder = RequestDecoder::new();
        decoder.feed(request_line.as_bytes()).unwrap();
        let result = decoder.decode_headers();
        prop_assert!(result.is_err(), "GET with authority-form should fail: {}", uri);
    }

    #[test]
    fn prop_authority_form_with_post_fails(uri in authority_form_uri()) {
        let request_line = format!("POST {} HTTP/1.1\r\nHost: example.com\r\n\r\n", uri);
        let mut decoder = RequestDecoder::new();
        decoder.feed(request_line.as_bytes()).unwrap();
        let result = decoder.decode_headers();
        prop_assert!(result.is_err(), "POST with authority-form should fail: {}", uri);
    }
}

// ========================================
// フラグメント禁止テスト (RFC 9112)
// ========================================

proptest! {
    #[test]
    fn prop_fragment_in_request_target_fails(
        path in origin_form_uri(),
        fragment in path_segment()
    ) {
        let uri = format!("{}#{}", path, fragment);
        let request_line = format!("GET {} HTTP/1.1\r\nHost: example.com\r\n\r\n", uri);
        let mut decoder = RequestDecoder::new();
        decoder.feed(request_line.as_bytes()).unwrap();
        let result = decoder.decode_headers();
        prop_assert!(result.is_err(), "request-target with fragment should fail: {}", uri);
    }

    #[test]
    fn prop_fragment_in_absolute_form_fails(
        uri in absolute_form_uri(),
        fragment in path_segment()
    ) {
        let uri_with_fragment = format!("{}#{}", uri, fragment);
        let request_line = format!("GET {} HTTP/1.1\r\nHost: example.com\r\n\r\n", uri_with_fragment);
        let mut decoder = RequestDecoder::new();
        decoder.feed(request_line.as_bytes()).unwrap();
        let result = decoder.decode_headers();
        prop_assert!(result.is_err(), "absolute-form with fragment should fail: {}", uri_with_fragment);
    }
}

// ========================================
// CONNECT メソッド制限テスト (RFC 9112 Section 3.2.3)
// ========================================

proptest! {
    #[test]
    fn prop_connect_with_origin_form_fails(uri in origin_form_uri()) {
        let request_line = format!("CONNECT {} HTTP/1.1\r\nHost: example.com\r\n\r\n", uri);
        let mut decoder = RequestDecoder::new();
        decoder.feed(request_line.as_bytes()).unwrap();
        let result = decoder.decode_headers();
        prop_assert!(result.is_err(), "CONNECT with origin-form should fail: {}", uri);
    }

    #[test]
    fn prop_connect_with_absolute_form_fails(uri in absolute_form_uri()) {
        let request_line = format!("CONNECT {} HTTP/1.1\r\nHost: example.com\r\n\r\n", uri);
        let mut decoder = RequestDecoder::new();
        decoder.feed(request_line.as_bytes()).unwrap();
        let result = decoder.decode_headers();
        prop_assert!(result.is_err(), "CONNECT with absolute-form should fail: {}", uri);
    }
}

// ========================================
// OPTIONS メソッド制限テスト (RFC 9112 Section 3.2.4)
// ========================================

proptest! {
    #[test]
    fn prop_options_with_origin_form_succeeds(uri in origin_form_uri()) {
        let request_line = format!("OPTIONS {} HTTP/1.1\r\nHost: example.com\r\n\r\n", uri);
        let mut decoder = RequestDecoder::new();
        decoder.feed(request_line.as_bytes()).unwrap();
        let result = decoder.decode_headers();
        prop_assert!(result.is_ok(), "OPTIONS with origin-form should succeed: {}", uri);
        prop_assert!(result.unwrap().is_some());
    }

    #[test]
    fn prop_options_with_absolute_form_succeeds(uri in absolute_form_uri()) {
        let request_line = format!("OPTIONS {} HTTP/1.1\r\nHost: example.com\r\n\r\n", uri);
        let mut decoder = RequestDecoder::new();
        decoder.feed(request_line.as_bytes()).unwrap();
        let result = decoder.decode_headers();
        prop_assert!(result.is_ok(), "OPTIONS with absolute-form should succeed: {}", uri);
        prop_assert!(result.unwrap().is_some());
    }

    #[test]
    fn prop_options_with_authority_form_fails(uri in authority_form_uri()) {
        let request_line = format!("OPTIONS {} HTTP/1.1\r\nHost: example.com\r\n\r\n", uri);
        let mut decoder = RequestDecoder::new();
        decoder.feed(request_line.as_bytes()).unwrap();
        let result = decoder.decode_headers();
        prop_assert!(result.is_err(), "OPTIONS with authority-form should fail: {}", uri);
    }
}

// ========================================
// "://" なしの absolute-form テスト
// ========================================

// urn: スキームの absolute-form ("://" を含まない)
const URN_NSS_CHARS: &[char] = &[
    'a', 'b', 'c', 'd', 'e', 'f', 'g', 'h', 'i', 'j', 'k', 'l', 'm', 'n', 'o', 'p', 'q', 'r', 's',
    't', 'u', 'v', 'w', 'x', 'y', 'z', 'A', 'B', 'C', 'D', 'E', 'F', 'G', 'H', 'I', 'J', 'K', 'L',
    'M', 'N', 'O', 'P', 'Q', 'R', 'S', 'T', 'U', 'V', 'W', 'X', 'Y', 'Z', '0', '1', '2', '3', '4',
    '5', '6', '7', '8', '9', ':', '.', '-',
];

fn urn_nid() -> impl Strategy<Value = String> {
    proptest::collection::vec(prop::char::range('a', 'z'), 2..=8)
        .prop_map(|chars| chars.into_iter().collect())
}

fn urn_nss() -> impl Strategy<Value = String> {
    proptest::collection::vec(prop::sample::select(URN_NSS_CHARS), 1..=32)
        .prop_map(|chars| chars.into_iter().collect())
}

proptest! {
    #[test]
    fn prop_urn_absolute_form_succeeds(
        nid in urn_nid(),
        nss in urn_nss()
    ) {
        let uri = format!("urn:{}:{}", nid, nss);
        let raw = format!("GET {} HTTP/1.1\r\nHost: \r\n\r\n", uri);
        let mut decoder = RequestDecoder::new();
        decoder.feed(raw.as_bytes()).unwrap();
        let result = decoder.decode_headers();
        prop_assert!(result.is_ok(), "urn: absolute-form should succeed: {}", uri);
        let (head, _) = result.unwrap().unwrap();
        prop_assert_eq!(head.uri(), uri.as_str());
    }
}
