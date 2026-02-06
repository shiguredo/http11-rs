//! Response 構造体のプロパティテスト

use proptest::prelude::*;
use shiguredo_http11::Response;

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
        let response = Response::new(code, &phrase);

        prop_assert_eq!(&response.version, "HTTP/1.1");
        prop_assert_eq!(response.status_code, code);
        prop_assert_eq!(&response.reason_phrase, &phrase);
        prop_assert!(response.headers.is_empty());
        prop_assert!(response.body.is_empty());
    }
}

// with_version() でカスタムバージョン
proptest! {
    #[test]
    fn prop_response_with_version(version in http_version(), code in status_code(), phrase in reason_phrase()) {
        let response = Response::with_version(&version, code, &phrase);

        prop_assert_eq!(&response.version, &version);
        prop_assert_eq!(response.status_code, code);
        prop_assert_eq!(&response.reason_phrase, &phrase);
        prop_assert!(response.headers.is_empty());
        prop_assert!(response.body.is_empty());
    }
}

// ========================================
// ビルダーパターンのテスト
// ========================================

// header() ビルダー
proptest! {
    #[test]
    fn prop_response_header_builder(code in status_code(), name in header_name(), value in header_value()) {
        let response = Response::new(code, "OK").header(&name, &value);

        prop_assert_eq!(response.headers.len(), 1);
        prop_assert_eq!(&response.headers[0].0, &name);
        prop_assert_eq!(&response.headers[0].1, &value);
    }
}

// 複数の header() チェーン
proptest! {
    #[test]
    fn prop_response_header_builder_chain(code in status_code(), headers in proptest::collection::vec((header_name(), header_value()), 1..5)) {
        let mut response = Response::new(code, "OK");
        for (name, value) in &headers {
            response = response.header(name, value);
        }

        prop_assert_eq!(response.headers.len(), headers.len());
        for (i, (name, value)) in headers.iter().enumerate() {
            prop_assert_eq!(&response.headers[i].0, name);
            prop_assert_eq!(&response.headers[i].1, value);
        }
    }
}

// body() ビルダー
proptest! {
    #[test]
    fn prop_response_body_builder(code in status_code(), body_data in proptest::collection::vec(any::<u8>(), 0..256)) {
        let response = Response::new(code, "OK").body(body_data.clone());

        prop_assert_eq!(&response.body, &body_data);
    }
}

// ========================================
// ヘッダー操作のテスト
// ========================================

// get_header() は大文字小文字を区別しない
proptest! {
    #[test]
    fn prop_response_get_header_case_insensitive(code in status_code(), value in header_value()) {
        let response = Response::new(code, "OK")
            .header("Content-Type", &value);

        prop_assert_eq!(response.get_header("Content-Type"), Some(value.as_str()));
        prop_assert_eq!(response.get_header("content-type"), Some(value.as_str()));
        prop_assert_eq!(response.get_header("CONTENT-TYPE"), Some(value.as_str()));
    }
}

// get_headers() は複数の同名ヘッダーをすべて取得
proptest! {
    #[test]
    fn prop_response_get_headers_multiple(code in status_code(), values in proptest::collection::vec(header_value(), 1..5)) {
        let mut response = Response::new(code, "OK");
        for value in &values {
            response = response.header("Set-Cookie", value);
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
        let response = Response::new(code, "OK")
            .header("Set-Cookie", &value);

        prop_assert_eq!(response.get_headers("set-cookie").len(), 1);
        prop_assert_eq!(response.get_headers("SET-COOKIE").len(), 1);
    }
}

// has_header() の動作確認
proptest! {
    #[test]
    fn prop_response_has_header(code in status_code(), name in header_name(), value in header_value()) {
        let response = Response::new(code, "OK").header(&name, &value);

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
        let response = Response::new(code, "OK").header("Connection", conn_value);

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
        let response = Response::new(code, "OK")
            .header("Content-Length", &len.to_string());

        prop_assert_eq!(response.content_length(), Some(len));
    }
}
