//! request-target 形式のプロパティテスト (RFC 9112 Section 3.2)

use proptest::prelude::*;
use shiguredo_http11::RequestDecoder;

// ========================================
// Strategy 定義
// ========================================

// パス用文字 (RFC 3986 pchar + "/")
fn path_char() -> impl Strategy<Value = char> {
    prop_oneof![
        prop::char::range('a', 'z'),
        prop::char::range('A', 'Z'),
        prop::char::range('0', '9'),
        Just('-'),
        Just('.'),
        Just('_'),
        Just('~'),
        Just('/'),
        Just(':'),
        Just('@'),
        Just('!'),
        Just('$'),
        Just('&'),
        Just('\''),
        Just('('),
        Just(')'),
        Just('*'),
        Just('+'),
        Just(','),
        Just(';'),
        Just('='),
    ]
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
// asterisk-form テスト
// ========================================

proptest! {
    #[test]
    fn prop_asterisk_form_with_options_succeeds(_dummy in Just(())) {
        let request_line = "OPTIONS * HTTP/1.1\r\nHost: example.com\r\n\r\n";
        let mut decoder = RequestDecoder::new();
        decoder.feed(request_line.as_bytes()).unwrap();
        let result = decoder.decode_headers();
        prop_assert!(result.is_ok(), "OPTIONS with asterisk-form should succeed");
        prop_assert!(result.unwrap().is_some());
    }

    #[test]
    fn prop_asterisk_form_with_get_fails(_dummy in Just(())) {
        let request_line = "GET * HTTP/1.1\r\nHost: example.com\r\n\r\n";
        let mut decoder = RequestDecoder::new();
        decoder.feed(request_line.as_bytes()).unwrap();
        let result = decoder.decode_headers();
        prop_assert!(result.is_err(), "GET with asterisk-form should fail");
    }

    #[test]
    fn prop_asterisk_form_with_post_fails(_dummy in Just(())) {
        let request_line = "POST * HTTP/1.1\r\nHost: example.com\r\n\r\n";
        let mut decoder = RequestDecoder::new();
        decoder.feed(request_line.as_bytes()).unwrap();
        let result = decoder.decode_headers();
        prop_assert!(result.is_err(), "POST with asterisk-form should fail");
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
// 不正な文字テスト (RFC 3986)
// ========================================

#[test]
fn test_invalid_path_character_space() {
    let request_line = "GET /path with space HTTP/1.1\r\nHost: example.com\r\n\r\n";
    let mut decoder = RequestDecoder::new();
    decoder.feed(request_line.as_bytes()).unwrap();
    let result = decoder.decode_headers();
    assert!(result.is_err(), "path with space should fail");
}

#[test]
fn test_invalid_path_character_backslash() {
    let request_line = "GET /path\\file HTTP/1.1\r\nHost: example.com\r\n\r\n";
    let mut decoder = RequestDecoder::new();
    decoder.feed(request_line.as_bytes()).unwrap();
    let result = decoder.decode_headers();
    assert!(result.is_err(), "path with backslash should fail");
}

#[test]
fn test_invalid_path_character_angle_bracket() {
    let request_line = "GET /path<file HTTP/1.1\r\nHost: example.com\r\n\r\n";
    let mut decoder = RequestDecoder::new();
    decoder.feed(request_line.as_bytes()).unwrap();
    let result = decoder.decode_headers();
    assert!(result.is_err(), "path with angle bracket should fail");
}

// ========================================
// パーセントエンコーディングテスト
// ========================================

#[test]
fn test_valid_percent_encoding() {
    let request_line = "GET /path%20with%20space HTTP/1.1\r\nHost: example.com\r\n\r\n";
    let mut decoder = RequestDecoder::new();
    decoder.feed(request_line.as_bytes()).unwrap();
    let result = decoder.decode_headers();
    assert!(result.is_ok(), "valid percent-encoding should succeed");
    assert!(result.unwrap().is_some());
}

#[test]
fn test_invalid_percent_encoding_incomplete() {
    let request_line = "GET /path%2 HTTP/1.1\r\nHost: example.com\r\n\r\n";
    let mut decoder = RequestDecoder::new();
    decoder.feed(request_line.as_bytes()).unwrap();
    let result = decoder.decode_headers();
    assert!(result.is_err(), "incomplete percent-encoding should fail");
}

#[test]
fn test_invalid_percent_encoding_non_hex() {
    let request_line = "GET /path%GG HTTP/1.1\r\nHost: example.com\r\n\r\n";
    let mut decoder = RequestDecoder::new();
    decoder.feed(request_line.as_bytes()).unwrap();
    let result = decoder.decode_headers();
    assert!(result.is_err(), "non-hex percent-encoding should fail");
}

// ========================================
// RequestTargetForm API テスト
// ========================================

#[test]
fn test_request_target_form_export() {
    use shiguredo_http11::RequestTargetForm;

    // 型が公開されていることを確認
    let _origin = RequestTargetForm::Origin;
    let _absolute = RequestTargetForm::Absolute;
    let _authority = RequestTargetForm::Authority;
    let _asterisk = RequestTargetForm::Asterisk;

    // Debug トレイト
    assert!(!format!("{:?}", RequestTargetForm::Origin).is_empty());

    // Clone と PartialEq
    let form = RequestTargetForm::Origin;
    let cloned = form;
    assert_eq!(form, cloned);
}
