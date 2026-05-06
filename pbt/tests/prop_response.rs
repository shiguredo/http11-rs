//! Response 構造体のプロパティテスト

use proptest::prelude::*;
use shiguredo_http11::{EncodeError, HttpHead, Response};

// ========================================
// Strategy 定義
// ========================================

fn http_version() -> impl Strategy<Value = String> {
    prop_oneof![
        Just("HTTP/1.0".to_string()),
        Just("HTTP/1.1".to_string()),
        Just("RTSP/1.0".to_string()),
        Just("RTSP/2.0".to_string()),
    ]
}

fn status_code() -> impl Strategy<Value = u16> {
    prop_oneof![
        100u16..=101, // 1xx
        200u16..=206, // 2xx
        300u16..=308, // 3xx
        400u16..=451, // 4xx
        500u16..=511, // 5xx
    ]
}

fn reason_phrase() -> impl Strategy<Value = String> {
    prop_oneof![
        Just("OK".to_string()),
        Just("Not Found".to_string()),
        Just("Internal Server Error".to_string()),
        Just("Bad Request".to_string()),
        "[A-Za-z ]{1,32}".prop_map(|s| s),
    ]
}

fn header_name() -> impl Strategy<Value = String> {
    "[A-Za-z][A-Za-z0-9-]{0,31}".prop_map(|s| s)
}

// HTTP ヘッダー値 (RFC 9110 Section 5.5)
// field-vchar = VCHAR / obs-text
// VCHAR = %x21-7E, obs-text = %x80-FF, SP = 0x20, HTAB = 0x09
fn header_value_char() -> impl Strategy<Value = char> {
    prop_oneof![
        prop::char::range('!', '~'), // VCHAR: 0x21-0x7E
        Just(' '),                   // SP: 0x20
        Just('\t'),                  // HTAB: 0x09
    ]
}

fn header_value() -> impl Strategy<Value = String> {
    proptest::collection::vec(header_value_char(), 1..=64)
        .prop_map(|chars| chars.into_iter().collect())
}

// ========================================
// コンストラクタのテスト
// ========================================

// new() はデフォルトで HTTP/1.1
proptest! {
    #[test]
    fn prop_response_new_default_version(code in status_code(), phrase in reason_phrase()) {
        let response = Response::new(code, &phrase).unwrap();

        prop_assert_eq!(HttpHead::version(&response), "HTTP/1.1");
        prop_assert_eq!(response.status_code(), code);
        prop_assert_eq!(response.reason_phrase(), &phrase);
        prop_assert!(HttpHead::headers(&response).is_empty());
        prop_assert!(response.body_bytes().is_none());
        prop_assert!(!response.is_body_omitted());
    }
}

// with_version() でカスタムバージョン
proptest! {
    #[test]
    fn prop_response_with_version(version in http_version(), code in status_code(), phrase in reason_phrase()) {
        let response = Response::with_version(&version, code, &phrase).unwrap();

        prop_assert_eq!(HttpHead::version(&response), &version);
        prop_assert_eq!(response.status_code(), code);
        prop_assert_eq!(response.reason_phrase(), &phrase);
        prop_assert!(HttpHead::headers(&response).is_empty());
        prop_assert!(response.body_bytes().is_none());
        prop_assert!(!response.is_body_omitted());
    }
}

// ========================================
// ビルダーパターンのテスト
// ========================================

// header() ビルダー
proptest! {
    #[test]
    fn prop_response_header_builder(code in status_code(), name in header_name(), value in header_value()) {
        let response = Response::new(code, "OK").unwrap().header(&name, &value).unwrap();

        prop_assert_eq!(HttpHead::headers(&response).len(), 1);
        prop_assert_eq!(&HttpHead::headers(&response)[0].0, &name);
        prop_assert_eq!(&HttpHead::headers(&response)[0].1, &value);
    }
}

// 複数の header() チェーン
proptest! {
    #[test]
    fn prop_response_header_builder_chain(code in status_code(), headers in proptest::collection::vec((header_name(), header_value()), 1..5)) {
        let mut response = Response::new(code, "OK").unwrap();
        for (name, value) in &headers {
            response = response.header(name, value).unwrap();
        }

        prop_assert_eq!(HttpHead::headers(&response).len(), headers.len());
        for (i, (name, value)) in headers.iter().enumerate() {
            prop_assert_eq!(&HttpHead::headers(&response)[i].0, name);
            prop_assert_eq!(&HttpHead::headers(&response)[i].1, value);
        }
    }
}

// body() ビルダー
proptest! {
    #[test]
    fn prop_response_body_builder(code in status_code(), body_data in proptest::collection::vec(any::<u8>(), 0..256)) {
        let response = Response::new(code, "OK").unwrap().body(body_data.clone());

        prop_assert_eq!(response.body_bytes(), Some(body_data.as_slice()));
    }
}

// omit_body() ビルダー
proptest! {
    #[test]
    fn prop_response_omit_body_builder(code in status_code(), omit in any::<bool>()) {
        let response = Response::new(code, "OK").unwrap().omit_body(omit);
        prop_assert_eq!(response.is_body_omitted(), omit);
    }
}

// ========================================
// ヘッダー操作のテスト
// ========================================

// get_header() は大文字小文字を区別しない
proptest! {
    #[test]
    fn prop_response_get_header_case_insensitive(code in status_code(), value in header_value()) {
        let response = Response::new(code, "OK").unwrap()
            .header("Content-Type", &value).unwrap();

        prop_assert_eq!(response.get_header("Content-Type"), Some(value.as_str()));
        prop_assert_eq!(response.get_header("content-type"), Some(value.as_str()));
        prop_assert_eq!(response.get_header("CONTENT-TYPE"), Some(value.as_str()));
    }
}

// get_headers() は複数の同名ヘッダーをすべて取得
proptest! {
    #[test]
    fn prop_response_get_headers_multiple(code in status_code(), values in proptest::collection::vec(header_value(), 1..5)) {
        let mut response = Response::new(code, "OK").unwrap();
        for value in &values {
            response = response.header("Set-Cookie", value).unwrap();
        }

        let headers = response.get_headers("Set-Cookie");
        prop_assert_eq!(headers.len(), values.len());
        for (i, value) in values.iter().enumerate() {
            prop_assert_eq!(headers[i], value.as_str());
        }
    }
}

// get_headers() は大文字小文字を区別しない
proptest! {
    #[test]
    fn prop_response_get_headers_case_insensitive(code in status_code(), value in header_value()) {
        let response = Response::new(code, "OK").unwrap()
            .header("Set-Cookie", &value).unwrap();

        prop_assert_eq!(response.get_headers("set-cookie").len(), 1);
        prop_assert_eq!(response.get_headers("SET-COOKIE").len(), 1);
    }
}

// has_header() の動作確認
proptest! {
    #[test]
    fn prop_response_has_header(code in status_code(), name in header_name(), value in header_value()) {
        let response = Response::new(code, "OK").unwrap().header(&name, &value).unwrap();

        prop_assert!(response.has_header(&name));
        prop_assert!(response.has_header(&name.to_lowercase()));
        prop_assert!(response.has_header(&name.to_uppercase()));
        prop_assert!(!response.has_header("X-Not-Exists"));
    }
}

// ========================================
// Connection と Keep-Alive のテスト
// ========================================

// connection() はヘッダー値を返す
proptest! {
    #[test]
    fn prop_response_connection_header(code in status_code(), conn_value in prop_oneof![Just("keep-alive"), Just("close"), Just("Keep-Alive"), Just("Close")]) {
        let response = Response::new(code, "OK").unwrap().header("Connection", conn_value).unwrap();

        prop_assert_eq!(response.connection(), Some(conn_value));
    }
}

// ========================================
// Content-Length と Transfer-Encoding のテスト
// ========================================

// content_length() は数値を返す
proptest! {
    #[test]
    fn prop_response_content_length(code in status_code(), len in 0usize..1_000_000) {
        let response = Response::new(code, "OK").unwrap()
            .header("Content-Length", &len.to_string()).unwrap();

        prop_assert_eq!(response.content_length(), Some(len as u64));
    }
}

// ========================================
// バリデーションのテスト
// ========================================

// 不正な status_code は構築時に拒否される
proptest! {
    #[test]
    fn prop_response_invalid_status_code(
        code in prop_oneof![0u16..100, 600u16..=u16::MAX],
        phrase in reason_phrase(),
    ) {
        let result = Response::new(code, &phrase);
        let is_invalid_status = matches!(result, Err(EncodeError::InvalidStatusCode { .. }));
        prop_assert!(is_invalid_status);
    }
}

// 制御文字を含む reason_phrase は構築時に拒否される
fn invalid_reason_phrase_char() -> impl Strategy<Value = char> {
    prop_oneof![
        // 制御文字 0x00-0x08, 0x0A-0x1F, 0x7F
        prop::char::range('\u{0}', '\u{8}'),
        prop::char::range('\u{A}', '\u{1F}'),
        Just('\u{7F}'),
    ]
}

proptest! {
    #[test]
    fn prop_response_invalid_reason_phrase(
        code in status_code(),
        bad_char in invalid_reason_phrase_char(),
    ) {
        let phrase = format!("OK{bad_char}bad");
        let result = Response::new(code, &phrase);
        let is_invalid = matches!(result, Err(EncodeError::InvalidReasonPhrase { .. }));
        prop_assert!(is_invalid);
    }
}

// 空の reason_phrase も構築時に拒否される
proptest! {
    #[test]
    fn prop_response_empty_reason_phrase_rejected(code in status_code()) {
        let result = Response::new(code, "");
        let is_invalid = matches!(result, Err(EncodeError::InvalidReasonPhrase { .. }));
        prop_assert!(is_invalid);
    }
}

// 不正な version は構築時に拒否される
fn invalid_version() -> impl Strategy<Value = String> {
    prop_oneof![
        Just(String::new()),
        Just("garbage".to_string()),
        Just("HTTP /1.1".to_string()),
        Just("HTTP/1.1\r\nX: y".to_string()),
        Just("HTTP/abc.def".to_string()),
    ]
}

proptest! {
    #[test]
    fn prop_response_invalid_version(
        version in invalid_version(),
        code in status_code(),
        phrase in reason_phrase(),
    ) {
        let result = Response::with_version(&version, code, &phrase);
        let is_invalid = matches!(result, Err(EncodeError::InvalidVersion { .. }));
        prop_assert!(is_invalid);
    }
}

// 不正なヘッダー名は add_header / header で拒否される
fn invalid_header_name() -> impl Strategy<Value = String> {
    prop_oneof![
        Just(String::new()),
        Just("Bad Name".to_string()),
        Just("Bad\r\nName".to_string()),
        Just("Bad\0Name".to_string()),
        Just("Bad:Name".to_string()),
    ]
}

proptest! {
    #[test]
    fn prop_response_invalid_header_name(
        code in status_code(),
        name in invalid_header_name(),
        value in header_value(),
    ) {
        let mut response = Response::new(code, "OK").unwrap();
        let result = response.add_header(&name, &value);
        let is_invalid = matches!(result, Err(EncodeError::InvalidHeaderName { .. }));
        prop_assert!(is_invalid);
    }
}

// 不正なヘッダー値は add_header / header で拒否される
fn invalid_header_value_char() -> impl Strategy<Value = char> {
    prop_oneof![
        prop::char::range('\u{0}', '\u{8}'),
        prop::char::range('\u{A}', '\u{1F}'),
        Just('\u{7F}'),
    ]
}

proptest! {
    #[test]
    fn prop_response_invalid_header_value(
        code in status_code(),
        name in header_name(),
        bad_char in invalid_header_value_char(),
    ) {
        let value = format!("good{bad_char}bad");
        let mut response = Response::new(code, "OK").unwrap();
        let result = response.add_header(&name, &value);
        let is_invalid = matches!(result, Err(EncodeError::InvalidHeaderValue { .. }));
        prop_assert!(is_invalid);
    }
}
