//! エンコーダーと Request/Response のプロパティテスト

use proptest::prelude::*;
use shiguredo_http11::{
    EncodeError, Request, Response, encode_chunk, encode_chunks, encode_request,
    encode_request_headers, encode_response, encode_response_headers,
};

// ========================================
// Strategy 定義
// ========================================

// HTTP メソッド
fn http_method() -> impl Strategy<Value = &'static str> {
    prop_oneof![
        Just("GET"),
        Just("POST"),
        Just("PUT"),
        Just("DELETE"),
        Just("HEAD"),
        Just("OPTIONS"),
        Just("PATCH"),
    ]
}

// HTTP バージョン
fn http_version() -> impl Strategy<Value = &'static str> {
    prop_oneof![Just("HTTP/1.0"), Just("HTTP/1.1"),]
}

// URI
fn uri() -> impl Strategy<Value = String> {
    prop_oneof![
        Just("/".to_string()),
        "[a-z]{1,8}".prop_map(|s| format!("/{}", s)),
        "[a-z]{1,4}/[a-z]{1,4}".prop_map(|s| format!("/{}", s)),
        "[a-z]{1,4}\\?[a-z]{1,4}=[a-z]{1,4}".prop_map(|s| format!("/{}", s)),
    ]
}

// ヘッダー名
fn header_name() -> impl Strategy<Value = String> {
    prop_oneof![
        Just("Content-Type".to_string()),
        Just("Accept".to_string()),
        Just("User-Agent".to_string()),
        Just("Cache-Control".to_string()),
        "[A-Za-z]{1,8}(-[A-Za-z]{1,8})?".prop_map(|s| s),
    ]
}

// ヘッダー値
fn header_value() -> impl Strategy<Value = String> {
    "[a-zA-Z0-9 /-]{1,32}".prop_map(|s| s)
}

// ステータスコード
fn status_code() -> impl Strategy<Value = u16> {
    prop_oneof![
        // 1xx
        Just(100u16),
        Just(101u16),
        // 2xx
        Just(200u16),
        Just(201u16),
        Just(204u16),
        // 3xx
        Just(301u16),
        Just(302u16),
        Just(304u16),
        // 4xx
        Just(400u16),
        Just(401u16),
        Just(403u16),
        Just(404u16),
        // 5xx
        Just(500u16),
        Just(502u16),
        Just(503u16),
    ]
}

// Reason phrase
fn reason_phrase() -> impl Strategy<Value = &'static str> {
    prop_oneof![
        Just("OK"),
        Just("Created"),
        Just("No Content"),
        Just("Not Found"),
        Just("Internal Server Error"),
        Just("Bad Gateway"),
    ]
}

// ボディ
fn body() -> impl Strategy<Value = Vec<u8>> {
    prop_oneof![
        Just(Vec::new()),
        "[a-zA-Z0-9]{1,32}".prop_map(|s| s.into_bytes()),
        proptest::collection::vec(prop::num::u8::ANY, 0..64),
    ]
}

// ========================================
// Request のテスト
// ========================================

proptest! {
    #[test]
    fn prop_request_new_with_method_and_uri(method in http_method(), uri in uri()) {
        let req = Request::new(method, &uri);
        prop_assert_eq!(req.method, method);
        prop_assert_eq!(req.uri, uri);
        prop_assert_eq!(req.version, "HTTP/1.1");
    }
}

proptest! {
    #[test]
    fn prop_request_with_version(method in http_method(), uri in uri(), version in http_version()) {
        let req = Request::with_version(method, &uri, version);
        prop_assert_eq!(req.method, method);
        prop_assert_eq!(req.uri, uri);
        prop_assert_eq!(req.version, version);
    }
}

proptest! {
    #[test]
    fn prop_request_header_builder(name in header_name(), value in header_value()) {
        let req = Request::new("GET", "/").header(&name, &value);
        prop_assert_eq!(req.headers.len(), 1);
        prop_assert_eq!(&req.headers[0].0, &name);
        prop_assert_eq!(&req.headers[0].1, &value);
    }
}

proptest! {
    #[test]
    fn prop_request_body_builder(data in body()) {
        let req = Request::new("POST", "/").body(data.clone());
        prop_assert_eq!(req.body, data);
    }
}

proptest! {
    #[test]
    fn prop_request_add_header(name in header_name(), value in header_value()) {
        let mut req = Request::new("GET", "/");
        req.add_header(&name, &value);
        prop_assert_eq!(req.headers.len(), 1);
        prop_assert_eq!(&req.headers[0].0, &name);
        prop_assert_eq!(&req.headers[0].1, &value);
    }
}

proptest! {
    #[test]
    fn prop_request_get_header(name in header_name(), value in header_value()) {
        let req = Request::new("GET", "/").header(&name, &value);
        prop_assert_eq!(req.get_header(&name), Some(value.as_str()));
        prop_assert_eq!(req.get_header(&name.to_uppercase()), Some(value.as_str()));
        prop_assert_eq!(req.get_header(&name.to_lowercase()), Some(value.as_str()));
    }
}

proptest! {
    #[test]
    fn prop_request_has_header(name in header_name(), value in header_value()) {
        let req = Request::new("GET", "/").header(&name, &value);
        prop_assert!(req.has_header(&name));
        prop_assert!(req.has_header(&name.to_uppercase()));
        prop_assert!(req.has_header(&name.to_lowercase()));
        prop_assert!(!req.has_header("NonExistent"));
    }
}

proptest! {
    #[test]
    fn prop_request_clone_eq(method in http_method(), uri in uri()) {
        let req = Request::new(method, &uri);
        let cloned = req.clone();
        prop_assert_eq!(req, cloned);
    }
}

// ========================================
// Response のテスト
// ========================================

proptest! {
    #[test]
    fn prop_response_new_with_status(status in status_code(), phrase in reason_phrase()) {
        let res = Response::new(status, phrase);
        prop_assert_eq!(res.status_code, status);
        prop_assert_eq!(res.reason_phrase, phrase);
        prop_assert_eq!(res.version, "HTTP/1.1");
    }
}

proptest! {
    #[test]
    fn prop_response_with_version(version in http_version(), status in status_code(), phrase in reason_phrase()) {
        let res = Response::with_version(version, status, phrase);
        prop_assert_eq!(res.version, version);
        prop_assert_eq!(res.status_code, status);
        prop_assert_eq!(res.reason_phrase, phrase);
    }
}

proptest! {
    #[test]
    fn prop_response_header_builder(name in header_name(), value in header_value()) {
        let res = Response::new(200, "OK").header(&name, &value);
        prop_assert_eq!(res.headers.len(), 1);
        prop_assert_eq!(&res.headers[0].0, &name);
        prop_assert_eq!(&res.headers[0].1, &value);
    }
}

proptest! {
    #[test]
    fn prop_response_body_builder(data in body()) {
        let res = Response::new(200, "OK").body(data.clone());
        prop_assert_eq!(res.body, data);
    }
}

proptest! {
    #[test]
    fn prop_response_add_header(name in header_name(), value in header_value()) {
        let mut res = Response::new(200, "OK");
        res.add_header(&name, &value);
        prop_assert_eq!(res.headers.len(), 1);
        prop_assert_eq!(&res.headers[0].0, &name);
        prop_assert_eq!(&res.headers[0].1, &value);
    }
}

proptest! {
    #[test]
    fn prop_response_get_header(name in header_name(), value in header_value()) {
        let res = Response::new(200, "OK").header(&name, &value);
        prop_assert_eq!(res.get_header(&name), Some(value.as_str()));
        prop_assert_eq!(res.get_header(&name.to_uppercase()), Some(value.as_str()));
        prop_assert_eq!(res.get_header(&name.to_lowercase()), Some(value.as_str()));
    }
}

proptest! {
    #[test]
    fn prop_response_has_header(name in header_name(), value in header_value()) {
        let res = Response::new(200, "OK").header(&name, &value);
        prop_assert!(res.has_header(&name));
        prop_assert!(res.has_header(&name.to_uppercase()));
        prop_assert!(res.has_header(&name.to_lowercase()));
        prop_assert!(!res.has_header("NonExistent"));
    }
}

proptest! {
    #[test]
    fn prop_response_clone_eq(status in status_code(), phrase in reason_phrase()) {
        let res = Response::new(status, phrase);
        let cloned = res.clone();
        prop_assert_eq!(res, cloned);
    }
}

// ========================================
// encode_request のテスト
// ========================================

proptest! {
    #[test]
    fn prop_encode_request_basic(method in http_method(), uri in uri()) {
        // HTTP/1.1 には Host ヘッダーが必須
        let req = Request::new(method, &uri).header("Host", "example.com");
        let encoded = encode_request(&req).unwrap();

        // リクエストラインを含む
        let request_line = format!("{} {} HTTP/1.1\r\n", method, uri);
        let encoded_str = String::from_utf8_lossy(&encoded);
        prop_assert!(encoded_str.starts_with(&request_line));
        prop_assert!(encoded_str.contains("\r\n\r\n"));
    }
}

proptest! {
    #[test]
    fn prop_encode_request_with_headers(
        method in http_method(),
        uri in uri(),
        header_name in header_name(),
        header_value in header_value()
    ) {
        // HTTP/1.1 には Host ヘッダーが必須
        let req = Request::new(method, &uri)
            .header("Host", "example.com")
            .header(&header_name, &header_value);
        let encoded = encode_request(&req).unwrap();
        let encoded_str = String::from_utf8_lossy(&encoded);

        let header_line = format!("{}: {}\r\n", header_name, header_value);
        prop_assert!(encoded_str.contains(&header_line));
    }
}

proptest! {
    #[test]
    fn prop_encode_request_with_body(method in http_method(), uri in uri(), data in body()) {
        // HTTP/1.1 には Host ヘッダーが必須
        let req = Request::new(method, &uri)
            .header("Host", "example.com")
            .body(data.clone());
        let encoded = encode_request(&req).unwrap();

        if !data.is_empty() {
            // Content-Length が自動追加される
            let encoded_str = String::from_utf8_lossy(&encoded);
            let cl_header = format!("Content-Length: {}\r\n", data.len());
            prop_assert!(encoded_str.contains(&cl_header));
            // ボディが末尾にある
            prop_assert!(encoded.ends_with(&data));
        }
    }
}

// ========================================
// encode_response のテスト
// ========================================

proptest! {
    #[test]
    fn prop_encode_response_basic(status in status_code(), phrase in reason_phrase()) {
        let res = Response::new(status, phrase);
        let encoded = encode_response(&res).unwrap();

        // ステータスラインを含む
        let status_line = format!("HTTP/1.1 {} {}\r\n", status, phrase);
        let encoded_str = String::from_utf8_lossy(&encoded);
        prop_assert!(encoded_str.starts_with(&status_line));
        prop_assert!(encoded_str.contains("\r\n\r\n"));
    }
}

proptest! {
    #[test]
    fn prop_encode_response_with_headers(
        status in status_code(),
        phrase in reason_phrase(),
        header_name in header_name(),
        header_value in header_value()
    ) {
        let res = Response::new(status, phrase).header(&header_name, &header_value);
        let encoded = encode_response(&res).unwrap();
        let encoded_str = String::from_utf8_lossy(&encoded);

        let header_line = format!("{}: {}\r\n", header_name, header_value);
        prop_assert!(encoded_str.contains(&header_line));
    }
}

proptest! {
    #[test]
    fn prop_encode_response_with_body(status in status_code(), phrase in reason_phrase(), data in body()) {
        let res = Response::new(status, phrase).body(data.clone());
        let encoded = encode_response(&res).unwrap();

        // 1xx/204/304 はボディがないため Content-Length を追加しない
        let status_has_body = !((100..200).contains(&status) || status == 204 || status == 304);

        if !data.is_empty() && status_has_body {
            // Content-Length が自動追加される
            let encoded_str = String::from_utf8_lossy(&encoded);
            let cl_header = format!("Content-Length: {}\r\n", data.len());
            prop_assert!(encoded_str.contains(&cl_header));
            // ボディが末尾にある
            prop_assert!(encoded.ends_with(&data));
        }
    }
}

proptest! {
    #[test]
    fn prop_encode_response_omit_content_length(
        // 204 No Content には Content-Length を送ってはならない (RFC 9110 Section 8.6)
        // 205 Reset Content もボディを持ってはならない (RFC 9110 Section 15.4.6)
        status in (200u16..204).prop_union(206..300),
        content_length in 1usize..10000
    ) {
        // omit_content_length=true の場合、Content-Length は自動付与されない
        // HEAD レスポンス用: 明示的に Content-Length を設定する
        let res = Response::new(status, "OK")
            .header("Content-Length", &content_length.to_string())
            .omit_content_length(true);
        let encoded = encode_response(&res).unwrap();
        let encoded_str = String::from_utf8_lossy(&encoded);

        // 明示的に設定した Content-Length は維持される
        let cl_header = format!("Content-Length: {}\r\n", content_length);
        prop_assert!(encoded_str.contains(&cl_header));
        // ボディは空 (HEAD レスポンスなので)
        prop_assert!(encoded_str.ends_with("\r\n\r\n"));
    }
}

proptest! {
    #[test]
    fn prop_encode_response_omit_content_length_no_header(
        status in 200..204u16
    ) {
        // omit_content_length=true で Content-Length ヘッダーも設定しない場合
        // Content-Length は自動付与されない (close-delimited になる)
        let res = Response::new(status, "OK")
            .omit_content_length(true);
        let encoded = encode_response(&res).unwrap();
        let encoded_str = String::from_utf8_lossy(&encoded);

        // Content-Length は含まれない
        prop_assert!(!encoded_str.contains("Content-Length"));
    }
}

// ========================================
// encode_chunk のテスト
// ========================================

proptest! {
    #[test]
    fn prop_encode_chunk_non_empty(data in proptest::collection::vec(prop::num::u8::ANY, 1..64)) {
        let encoded = encode_chunk(&data);
        let encoded_str = String::from_utf8_lossy(&encoded);

        // サイズ行を含む
        let size_line = format!("{:x}\r\n", data.len());
        prop_assert!(encoded_str.starts_with(&size_line));
        // CRLF で終わる
        prop_assert!(encoded.ends_with(b"\r\n"));
        // データを含む
        let data_start = size_line.len();
        let data_end = encoded.len() - 2;
        prop_assert_eq!(&encoded[data_start..data_end], &data[..]);
    }
}

// ========================================
// encode_chunks のテスト
// ========================================

proptest! {
    #[test]
    fn prop_encode_chunks_basic(count in 1usize..=5usize) {
        let chunks: Vec<&[u8]> = (0..count).map(|_| b"test".as_ref()).collect();
        let encoded = encode_chunks(&chunks);

        // 終端チャンクで終わる
        prop_assert!(encoded.ends_with(b"0\r\n\r\n"));

        // 各チャンクのサイズ行を含む
        let encoded_str = String::from_utf8_lossy(&encoded);
        prop_assert_eq!(encoded_str.matches("4\r\n").count(), count);
    }
}

// ========================================
// encode_request_headers のテスト
// ========================================

proptest! {
    #[test]
    fn prop_encode_request_headers_basic(method in http_method(), uri in uri()) {
        let req = Request::new(method, &uri)
            .header("Host", "example.com");
        let encoded = encode_request_headers(&req).unwrap();
        let encoded_str = String::from_utf8_lossy(&encoded);

        // リクエストラインを含む
        let request_line = format!("{} {} HTTP/1.1\r\n", method, uri);
        prop_assert!(encoded_str.starts_with(&request_line));
        // ヘッダーを含む
        prop_assert!(encoded_str.contains("Host: example.com\r\n"));
        // 空行で終わる
        prop_assert!(encoded_str.ends_with("\r\n\r\n"));
        // ボディは含まない (ヘッダーのみ)
    }
}

// ========================================
// encode_response_headers のテスト
// ========================================

proptest! {
    #[test]
    fn prop_encode_response_headers_basic(status in status_code(), phrase in reason_phrase()) {
        let res = Response::new(status, phrase)
            .header("Content-Type", "text/html");
        let encoded = encode_response_headers(&res).unwrap();
        let encoded_str = String::from_utf8_lossy(&encoded);

        // ステータスラインを含む
        let status_line = format!("HTTP/1.1 {} {}\r\n", status, phrase);
        prop_assert!(encoded_str.starts_with(&status_line));
        // ヘッダーを含む
        prop_assert!(encoded_str.contains("Content-Type: text/html\r\n"));
        // 空行で終わる
        prop_assert!(encoded_str.ends_with("\r\n\r\n"));
    }
}

// ========================================
// Request::encode / Response::encode のテスト
// ========================================

proptest! {
    #[test]
    fn prop_request_encode_method(method in http_method(), uri in uri()) {
        // HTTP/1.1 には Host ヘッダーが必須
        let req = Request::new(method, &uri).header("Host", "example.com");
        let encoded = req.encode();
        prop_assert_eq!(encoded, encode_request(&req).unwrap());
    }
}

proptest! {
    #[test]
    fn prop_response_encode_method(status in status_code(), phrase in reason_phrase()) {
        let res = Response::new(status, phrase);
        let encoded = res.encode();
        prop_assert_eq!(encoded, encode_response(&res).unwrap());
    }
}

// ========================================
// Request::encode_headers / Response::encode_headers のテスト
// ========================================

proptest! {
    #[test]
    fn prop_request_encode_headers_method(method in http_method(), uri in uri()) {
        // HTTP/1.1 には Host ヘッダーが必須
        let req = Request::new(method, &uri).header("Host", "example.com");
        let encoded = req.encode_headers();
        prop_assert_eq!(encoded, encode_request_headers(&req).unwrap());
    }
}

// ========================================
// Host 必須チェックのテスト (RFC 9112 Section 3.2)
// ========================================

proptest! {
    #[test]
    fn prop_encode_request_host_required_for_http11(method in http_method(), uri in uri()) {
        // HTTP/1.1 で Host ヘッダーがない場合は常にエラー
        let req = Request::new(method, &uri);
        let result = encode_request(&req);
        prop_assert!(matches!(result, Err(EncodeError::MissingHostHeader)));
    }
}

proptest! {
    #[test]
    fn prop_encode_request_host_optional_for_http10(method in http_method(), uri in uri()) {
        // HTTP/1.0 で Host ヘッダーがない場合は成功
        let req = Request::with_version(method, &uri, "HTTP/1.0");
        let result = encode_request(&req);
        prop_assert!(result.is_ok());
    }
}

proptest! {
    #[test]
    fn prop_response_encode_headers_method(status in status_code(), phrase in reason_phrase()) {
        let res = Response::new(status, phrase);
        let encoded = res.encode_headers();
        prop_assert_eq!(encoded, encode_response_headers(&res).unwrap());
    }
}

// ========================================
// RFC 9112 Section 6.2: Transfer-Encoding と Content-Length の同時送信禁止
// ========================================

proptest! {
    #[test]
    fn prop_encode_request_te_and_cl_always_error(
        method in http_method(),
        uri in uri(),
        cl in 1usize..10000
    ) {
        // Transfer-Encoding と Content-Length が両方あれば常にエラー
        let req = Request::new(method, &uri)
            .header("Host", "example.com")
            .header("Transfer-Encoding", "chunked")
            .header("Content-Length", &cl.to_string());
        let result = encode_request(&req);
        prop_assert!(matches!(
            result,
            Err(EncodeError::ConflictingTransferEncodingAndContentLength)
        ));
    }
}

proptest! {
    #[test]
    fn prop_encode_response_te_and_cl_always_error(
        // 1xx / 204 は別のエラーになる可能性があるため除外
        status in (200u16..204).prop_union(205..600),
        cl in 1usize..10000
    ) {
        // Transfer-Encoding と Content-Length が両方あれば常にエラー
        let res = Response::new(status, "OK")
            .header("Transfer-Encoding", "chunked")
            .header("Content-Length", &cl.to_string());
        let result = encode_response(&res);
        prop_assert!(matches!(
            result,
            Err(EncodeError::ConflictingTransferEncodingAndContentLength)
        ));
    }
}

// ========================================
// RFC 9112 Section 6.1: 1xx / 204 レスポンスで Transfer-Encoding 禁止
// ========================================

proptest! {
    #[test]
    fn prop_encode_response_1xx_with_te_always_error(status in 100u16..200) {
        // 1xx レスポンスに Transfer-Encoding があれば常にエラー
        let res = Response::new(status, "Info").header("Transfer-Encoding", "chunked");
        let result = encode_response(&res);
        match result {
            Err(EncodeError::ForbiddenTransferEncoding { status_code }) => {
                prop_assert_eq!(status_code, status);
            }
            other => {
                prop_assert!(false, "Expected ForbiddenTransferEncoding, got {:?}", other);
            }
        }
    }
}

// ========================================
// CRLF/NUL インジェクション拒否テスト
// ========================================

proptest! {
    #[test]
    fn prop_encode_request_crlf_in_method(uri in uri()) {
        // メソッドに CRLF を含む場合はエラー
        for method in &["GET\r\nEvil: header", "POST\r\n", "GET\nEvil", "GET\rEvil"] {
            let req = Request::new(method, &uri).header("Host", "example.com");
            let result = encode_request(&req);
            prop_assert!(result.is_err(), "CRLF in method should be rejected: {:?}", method);
        }
    }
}

proptest! {
    #[test]
    fn prop_encode_request_crlf_in_uri(method in http_method()) {
        // URI に CRLF を含む場合はエラー
        for uri in &["/path\r\nEvil: header", "/\r\n", "/test\nEvil"] {
            let req = Request::new(method, uri).header("Host", "example.com");
            let result = encode_request(&req);
            prop_assert!(result.is_err(), "CRLF in URI should be rejected: {:?}", uri);
        }
    }
}

proptest! {
    #[test]
    fn prop_encode_request_crlf_in_header_name(method in http_method(), uri in uri()) {
        // ヘッダー名に CRLF を含む場合はエラー
        for name in &["Evil\r\nHeader", "Evil\nHeader", "Evil\rHeader"] {
            let req = Request::new(method, &uri)
                .header("Host", "example.com")
                .header(name, "value");
            let result = encode_request(&req);
            prop_assert!(result.is_err(), "CRLF in header name should be rejected: {:?}", name);
        }
    }
}

proptest! {
    #[test]
    fn prop_encode_request_crlf_in_header_value(method in http_method(), uri in uri()) {
        // ヘッダー値に CRLF を含む場合はエラー
        for value in &["evil\r\nEvil: injected", "evil\ninjected", "evil\rinjected"] {
            let req = Request::new(method, &uri)
                .header("Host", "example.com")
                .header("X-Test", value);
            let result = encode_request(&req);
            prop_assert!(result.is_err(), "CRLF in header value should be rejected: {:?}", value);
        }
    }
}

proptest! {
    #[test]
    fn prop_encode_request_nul_in_header_value(method in http_method(), uri in uri()) {
        // ヘッダー値に NUL を含む場合はエラー
        let req = Request::new(method, &uri)
            .header("Host", "example.com")
            .header("X-Test", "evil\0value");
        let result = encode_request(&req);
        prop_assert!(result.is_err(), "NUL in header value should be rejected");
    }
}

proptest! {
    #[test]
    fn prop_encode_response_crlf_in_reason_phrase(status in status_code()) {
        // reason-phrase に CRLF を含む場合はエラー
        for phrase in &["OK\r\nEvil: header", "OK\n", "OK\r"] {
            let res = Response::new(status, phrase);
            let result = encode_response(&res);
            prop_assert!(result.is_err(), "CRLF in reason-phrase should be rejected: {:?}", phrase);
        }
    }
}

proptest! {
    #[test]
    fn prop_encode_response_crlf_in_header_name(status in status_code(), phrase in reason_phrase()) {
        // レスポンスでもヘッダー名に CRLF を含む場合はエラー
        for name in &["Evil\r\nHeader", "Evil\nHeader"] {
            let res = Response::new(status, phrase).header(name, "value");
            let result = encode_response(&res);
            prop_assert!(result.is_err(), "CRLF in response header name should be rejected: {:?}", name);
        }
    }
}

proptest! {
    #[test]
    fn prop_encode_response_crlf_in_header_value(status in status_code(), phrase in reason_phrase()) {
        // レスポンスでもヘッダー値に CRLF を含む場合はエラー
        for value in &["evil\r\nEvil: injected", "evil\ninjected"] {
            let res = Response::new(status, phrase).header("X-Test", value);
            let result = encode_response(&res);
            prop_assert!(result.is_err(), "CRLF in response header value should be rejected: {:?}", value);
        }
    }
}

// ========================================
// 205 Reset Content ボディ禁止テスト
// ========================================

proptest! {
    #[test]
    fn prop_encode_response_205_with_body_always_error(
        data in proptest::collection::vec(any::<u8>(), 1..128)
    ) {
        // 205 で非空ボディは常にエラー
        let res = Response::new(205, "Reset Content").body(data);
        let result = encode_response(&res);
        prop_assert!(matches!(result, Err(EncodeError::ForbiddenBodyFor205)));
    }
}

// ========================================
// RFC 9110 Section 8.6: 1xx / 204 レスポンスで Content-Length 禁止
// ========================================

proptest! {
    #[test]
    fn prop_encode_response_1xx_with_cl_always_error(status in 100u16..200) {
        // 1xx レスポンスに Content-Length があれば常にエラー
        let res = Response::new(status, "Info").header("Content-Length", "0");
        let result = encode_response(&res);
        match result {
            Err(EncodeError::ForbiddenContentLength { status_code }) => {
                prop_assert_eq!(status_code, status);
            }
            other => {
                prop_assert!(false, "Expected ForbiddenContentLength, got {:?}", other);
            }
        }
    }
}

// ========================================
// Host ヘッダーバリデーションテスト
// ========================================

proptest! {
    #[test]
    fn prop_encode_request_duplicate_host_always_error(
        method in http_method(),
        uri in uri(),
        host1 in "[a-z]{3,8}\\.com",
        host2 in "[a-z]{3,8}\\.org"
    ) {
        // 異なるドメインで Host を 2 つ付けると常にエラー
        let req = Request::new(method, &uri)
            .header("Host", &host1)
            .header("Host", &host2);
        let result = encode_request(&req);
        prop_assert!(matches!(result, Err(EncodeError::DuplicateHostHeader)));
    }
}
