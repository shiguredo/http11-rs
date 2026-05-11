//! エンコーダーのプロパティテスト

use proptest::prelude::*;
use shiguredo_http11::{
    EncodeError, Request, RequestEncoder, Response, ResponseEncoder, StatusCode, encode_chunk,
    encode_chunks, encode_request, encode_request_headers, encode_response,
    encode_response_headers,
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
// encode_request のテスト
// ========================================

proptest! {
    #[test]
    fn prop_encode_request_basic(method in http_method(), uri in uri()) {
        let req = Request::new(method, &uri)
            .unwrap()
            .header("Host", "example.com")
            .unwrap();
        let encoded = encode_request(&req).unwrap();

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
        let req = Request::new(method, &uri)
            .unwrap()
            .header("Host", "example.com")
            .unwrap()
            .header(&header_name, &header_value)
            .unwrap();
        let encoded = encode_request(&req).unwrap();
        let encoded_str = String::from_utf8_lossy(&encoded);

        let header_line = format!("{}: {}\r\n", header_name, header_value);
        prop_assert!(encoded_str.contains(&header_line));
    }
}

proptest! {
    #[test]
    fn prop_encode_request_with_body(method in http_method(), uri in uri(), data in body()) {
        let req = Request::new(method, &uri)
            .unwrap()
            .header("Host", "example.com")
            .unwrap()
            .body(data.clone());
        let encoded = encode_request(&req).unwrap();

        if !data.is_empty() {
            let encoded_str = String::from_utf8_lossy(&encoded);
            let cl_header = format!("Content-Length: {}\r\n", data.len());
            prop_assert!(encoded_str.contains(&cl_header));
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
        let res = Response::new(status, phrase).unwrap();
        let encoded = encode_response(&res).unwrap();

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
        let res = Response::new(status, phrase)
            .unwrap()
            .header(&header_name, &header_value)
            .unwrap();
        let encoded = encode_response(&res).unwrap();
        let encoded_str = String::from_utf8_lossy(&encoded);

        let header_line = format!("{}: {}\r\n", header_name, header_value);
        prop_assert!(encoded_str.contains(&header_line));
    }
}

proptest! {
    #[test]
    fn prop_encode_response_with_body(status in status_code(), phrase in reason_phrase(), data in body()) {
        let res = Response::new(status, phrase).unwrap().body(data.clone());
        let encoded = encode_response(&res).unwrap();

        let status_has_body = !((100..200).contains(&status) || status == 204 || status == 304);

        if !data.is_empty() && status_has_body {
            let encoded_str = String::from_utf8_lossy(&encoded);
            let cl_header = format!("Content-Length: {}\r\n", data.len());
            prop_assert!(encoded_str.contains(&cl_header));
            prop_assert!(encoded.ends_with(&data));
        }
    }
}

proptest! {
    #[test]
    fn prop_encode_response_omit_body_with_content_length(
        status in (200u16..204).prop_union(206..300),
        content_length in 1usize..10000
    ) {
        let res = Response::new(status, "OK")
            .unwrap()
            .header("Content-Length", content_length.to_string())
            .unwrap()
            .omit_body(true);
        let encoded = encode_response(&res).unwrap();
        let encoded_str = String::from_utf8_lossy(&encoded);

        let cl_header = format!("Content-Length: {}\r\n", content_length);
        prop_assert!(encoded_str.contains(&cl_header));
        prop_assert!(encoded_str.ends_with("\r\n\r\n"));
    }
}

proptest! {
    #[test]
    fn prop_encode_response_omit_body_empty_no_header(
        status in 200..204u16
    ) {
        let res = Response::new(status, "OK")
            .unwrap()
            .omit_body(true);
        let encoded = encode_response(&res).unwrap();
        let encoded_str = String::from_utf8_lossy(&encoded);

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

        let size_line = format!("{:x}\r\n", data.len());
        prop_assert!(encoded_str.starts_with(&size_line));
        prop_assert!(encoded.ends_with(b"\r\n"));
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

        prop_assert!(encoded.ends_with(b"0\r\n\r\n"));

        let encoded_str = String::from_utf8_lossy(&encoded);
        prop_assert_eq!(encoded_str.matches("4\r\n").count(), count);
    }
}

// `write_hex_usize` ヘルパーは encoder.rs のプライベート関数のため、
// 公開 API `encode_chunk` / `encode_chunks` の出力が
// `alloc::format!("{:x}\r\n", n)` ベースの参照実装とバイト単位で完全一致することで
// ヘルパーの正しさを間接検証する

proptest! {
    #[test]
    fn prop_encode_chunk_equals_format_reference(
        data in proptest::collection::vec(prop::num::u8::ANY, 0..256)
    ) {
        let encoded = encode_chunk(&data);

        let mut expected: Vec<u8> = Vec::new();
        if data.is_empty() {
            expected.extend_from_slice(b"0\r\n\r\n");
        } else {
            expected.extend_from_slice(format!("{:x}\r\n", data.len()).as_bytes());
            expected.extend_from_slice(&data);
            expected.extend_from_slice(b"\r\n");
        }
        prop_assert_eq!(encoded, expected);
    }
}

proptest! {
    #[test]
    fn prop_encode_chunks_equals_format_reference(
        chunks in proptest::collection::vec(
            proptest::collection::vec(prop::num::u8::ANY, 0..32),
            0..6,
        )
    ) {
        let chunk_refs: Vec<&[u8]> = chunks.iter().map(|c| c.as_slice()).collect();
        let encoded = encode_chunks(&chunk_refs);

        let mut expected: Vec<u8> = Vec::new();
        for chunk in &chunks {
            expected.extend_from_slice(format!("{:x}\r\n", chunk.len()).as_bytes());
            expected.extend_from_slice(chunk);
            expected.extend_from_slice(b"\r\n");
        }
        expected.extend_from_slice(b"0\r\n\r\n");
        prop_assert_eq!(encoded, expected);
    }
}

// ========================================
// encode_request_headers のテスト
// ========================================

proptest! {
    #[test]
    fn prop_encode_request_headers_basic(method in http_method(), uri in uri()) {
        let req = Request::new(method, &uri)
            .unwrap()
            .header("Host", "example.com")
            .unwrap();
        let encoded = encode_request_headers(&req).unwrap();
        let encoded_str = String::from_utf8_lossy(&encoded);

        let request_line = format!("{} {} HTTP/1.1\r\n", method, uri);
        prop_assert!(encoded_str.starts_with(&request_line));
        prop_assert!(encoded_str.contains("Host: example.com\r\n"));
        prop_assert!(encoded_str.ends_with("\r\n\r\n"));
    }
}

// ========================================
// encode_response_headers のテスト
// ========================================

proptest! {
    #[test]
    fn prop_encode_response_headers_basic(status in status_code(), phrase in reason_phrase()) {
        let res = Response::new(status, phrase)
            .unwrap()
            .header("Content-Type", "text/html")
            .unwrap();
        let encoded = encode_response_headers(&res).unwrap();
        let encoded_str = String::from_utf8_lossy(&encoded);

        let status_line = format!("HTTP/1.1 {} {}\r\n", status, phrase);
        prop_assert!(encoded_str.starts_with(&status_line));
        prop_assert!(encoded_str.contains("Content-Type: text/html\r\n"));
        prop_assert!(encoded_str.ends_with("\r\n\r\n"));
    }
}

// ========================================
// Host 必須チェックのテスト (RFC 9112 Section 3.2)
// ========================================

proptest! {
    #[test]
    fn prop_encode_request_host_required_for_http11(method in http_method(), uri in uri()) {
        let req = Request::new(method, &uri).unwrap();
        let result = encode_request(&req);
        prop_assert!(matches!(result, Err(EncodeError::MissingHostHeader)));
    }
}

proptest! {
    #[test]
    fn prop_encode_request_host_optional_for_http10(method in http_method(), uri in uri()) {
        let req = Request::with_version(method, &uri, "HTTP/1.0").unwrap();
        let result = encode_request(&req);
        prop_assert!(result.is_ok());
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
        let req = Request::new(method, &uri)
            .unwrap()
            .header("Host", "example.com")
            .unwrap()
            .header("Transfer-Encoding", "chunked")
            .unwrap()
            .header("Content-Length", cl.to_string())
            .unwrap();
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
        status in (200u16..204).prop_union(205..600),
        cl in 1usize..10000
    ) {
        let res = Response::new(status, "OK")
            .unwrap()
            .header("Transfer-Encoding", "chunked")
            .unwrap()
            .header("Content-Length", cl.to_string())
            .unwrap();
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
    fn prop_encode_response_1xx_or_204_with_te_always_error(
        status in prop_oneof![100u16..200, Just(204u16)]
    ) {
        let res = Response::new(status, "Info")
            .unwrap()
            .header("Transfer-Encoding", "chunked")
            .unwrap();
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
// RFC 9110 Section 8.6: 1xx / 204 レスポンスで Content-Length 禁止
// ========================================

proptest! {
    #[test]
    fn prop_encode_response_1xx_or_204_with_cl_always_error(
        status in prop_oneof![100u16..200, Just(204u16)]
    ) {
        let res = Response::new(status, "Info")
            .unwrap()
            .header("Content-Length", "0")
            .unwrap();
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
// 205 Reset Content テスト
// ========================================

proptest! {
    #[test]
    fn prop_encode_response_205_with_body_always_error(
        data in proptest::collection::vec(any::<u8>(), 1..128)
    ) {
        let res = Response::with_status(StatusCode::RESET_CONTENT).body(data);
        let result = encode_response(&res);
        prop_assert!(matches!(result, Err(EncodeError::ForbiddenBodyFor205)));
    }
}

proptest! {
    /// 205 レスポンスで Transfer-Encoding は常にエラー
    #[test]
    fn prop_encode_response_205_with_te_always_error(
        te_value in prop_oneof![
            Just("chunked".to_string()),
            Just("gzip".to_string()),
            Just("deflate".to_string()),
        ]
    ) {
        let res = Response::with_status(StatusCode::RESET_CONTENT)
            .header("Transfer-Encoding", &te_value)
            .unwrap();
        let result = encode_response(&res);
        match result {
            Err(EncodeError::ForbiddenTransferEncoding { status_code: 205 }) => {}
            other => {
                prop_assert!(false, "Expected ForbiddenTransferEncoding for 205, got {:?}", other);
            }
        }
    }
}

proptest! {
    /// 205 レスポンスで Content-Length が非 0 は常にエラー
    #[test]
    fn prop_encode_response_205_with_cl_nonzero_always_error(cl in 1usize..10000) {
        let res = Response::with_status(StatusCode::RESET_CONTENT)
            .header("Content-Length", cl.to_string())
            .unwrap();
        let result = encode_response(&res);
        match result {
            Err(EncodeError::ForbiddenContentLength { status_code: 205 }) => {}
            other => {
                prop_assert!(false, "Expected ForbiddenContentLength for 205, got {:?}", other);
            }
        }
    }
}

// ========================================
// 304 レスポンスのボディ除外 PBT
// ========================================

proptest! {
    /// 304 レスポンスはボディを含めない
    #[test]
    fn prop_encode_response_304_no_body(
        data in proptest::collection::vec(any::<u8>(), 1..128)
    ) {
        let res = Response::with_status(StatusCode::NOT_MODIFIED).body(data);
        let encoded = encode_response(&res).unwrap();
        let encoded_str = String::from_utf8_lossy(&encoded);
        let header_end = encoded_str.find("\r\n\r\n").unwrap();
        prop_assert_eq!(encoded.len(), header_end + 4);
    }
}

// ========================================
// Host ヘッダーバリデーション PBT
// ========================================

proptest! {
    #[test]
    fn prop_encode_request_duplicate_host_always_error(
        method in http_method(),
        uri in uri(),
        host1 in "[a-z]{3,8}\\.com",
        host2 in "[a-z]{3,8}\\.org"
    ) {
        let req = Request::new(method, &uri)
            .unwrap()
            .header("Host", &host1)
            .unwrap()
            .header("Host", &host2)
            .unwrap();
        let result = encode_request(&req);
        prop_assert!(matches!(result, Err(EncodeError::DuplicateHostHeader)));
    }
}

// ========================================
// encode_request_headers のエラーパス PBT
// ========================================

proptest! {
    /// encode_request_headers でも Host ヘッダー必須
    #[test]
    fn prop_encode_request_headers_host_required_for_http11(
        method in http_method(),
        uri in uri()
    ) {
        let req = Request::new(method, &uri).unwrap();
        let result = encode_request_headers(&req);
        prop_assert!(matches!(result, Err(EncodeError::MissingHostHeader)));
    }
}

proptest! {
    /// encode_request_headers でも TE+CL 同時禁止
    #[test]
    fn prop_encode_request_headers_te_and_cl_error(
        method in http_method(),
        uri in uri(),
        cl in 1usize..10000
    ) {
        let req = Request::new(method, &uri)
            .unwrap()
            .header("Host", "example.com")
            .unwrap()
            .header("Transfer-Encoding", "chunked")
            .unwrap()
            .header("Content-Length", cl.to_string())
            .unwrap();
        let result = encode_request_headers(&req);
        prop_assert!(matches!(
            result,
            Err(EncodeError::ConflictingTransferEncodingAndContentLength)
        ));
    }
}

// ========================================
// encode_response_headers のエラーパス PBT
// ========================================

proptest! {
    /// encode_response_headers でも TE+CL 同時禁止
    #[test]
    fn prop_encode_response_headers_te_and_cl_error(
        status in (200u16..204).prop_union(206..600),
        cl in 1usize..10000
    ) {
        let res = Response::new(status, "OK")
            .unwrap()
            .header("Transfer-Encoding", "chunked")
            .unwrap()
            .header("Content-Length", cl.to_string())
            .unwrap();
        let result = encode_response_headers(&res);
        prop_assert!(matches!(
            result,
            Err(EncodeError::ConflictingTransferEncodingAndContentLength)
        ));
    }
}

proptest! {
    /// encode_response_headers で 1xx/204+TE 禁止
    #[test]
    fn prop_encode_response_headers_1xx_or_204_with_te_error(
        status in prop_oneof![100u16..200, Just(204u16)]
    ) {
        let res = Response::new(status, "Info")
            .unwrap()
            .header("Transfer-Encoding", "chunked")
            .unwrap();
        let result = encode_response_headers(&res);
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

proptest! {
    /// encode_response_headers で 1xx/204+CL 禁止
    #[test]
    fn prop_encode_response_headers_1xx_or_204_with_cl_error(
        status in prop_oneof![100u16..200, Just(204u16)]
    ) {
        let res = Response::new(status, "Info")
            .unwrap()
            .header("Content-Length", "0")
            .unwrap();
        let result = encode_response_headers(&res);
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

proptest! {
    /// encode_response_headers で 205+TE 禁止
    #[test]
    fn prop_encode_response_headers_205_with_te_error(
        te_value in prop_oneof![
            Just("chunked".to_string()),
            Just("gzip".to_string()),
        ]
    ) {
        let res = Response::with_status(StatusCode::RESET_CONTENT)
            .header("Transfer-Encoding", &te_value)
            .unwrap();
        let result = encode_response_headers(&res);
        match result {
            Err(EncodeError::ForbiddenTransferEncoding { status_code: 205 }) => {}
            other => {
                prop_assert!(false, "Expected ForbiddenTransferEncoding for 205, got {:?}", other);
            }
        }
    }
}

proptest! {
    /// encode_response_headers で 205+CL(非 0) 禁止
    #[test]
    fn prop_encode_response_headers_205_with_cl_nonzero_error(cl in 1usize..10000) {
        let res = Response::with_status(StatusCode::RESET_CONTENT)
            .header("Content-Length", cl.to_string())
            .unwrap();
        let result = encode_response_headers(&res);
        match result {
            Err(EncodeError::ForbiddenContentLength { status_code: 205 }) => {}
            other => {
                prop_assert!(false, "Expected ForbiddenContentLength for 205, got {:?}", other);
            }
        }
    }
}

// ========================================
// メソッドラッパーのテスト
// ========================================

proptest! {
    /// Request::encode() は encode_request() と同じ結果を返す
    #[test]
    fn prop_request_encode_equals_free_function(method in http_method(), uri in uri()) {
        let req = Request::new(method, &uri)
            .unwrap()
            .header("Host", "example.com")
            .unwrap();
        let via_method = req.encode();
        let via_free = encode_request(&req).unwrap();
        prop_assert_eq!(via_method, via_free);
    }
}

proptest! {
    /// Response::encode() は encode_response() と同じ結果を返す
    #[test]
    fn prop_response_encode_equals_free_function(status in status_code(), phrase in reason_phrase()) {
        let res = Response::new(status, phrase).unwrap();
        let via_method = res.encode();
        let via_free = encode_response(&res).unwrap();
        prop_assert_eq!(via_method, via_free);
    }
}

proptest! {
    /// Request::encode_headers() は encode_request_headers() と同じ結果を返す
    #[test]
    fn prop_request_encode_headers_equals_free_function(method in http_method(), uri in uri()) {
        let req = Request::new(method, &uri)
            .unwrap()
            .header("Host", "example.com")
            .unwrap();
        let via_method = req.encode_headers();
        let via_free = encode_request_headers(&req).unwrap();
        prop_assert_eq!(via_method, via_free);
    }
}

proptest! {
    /// Response::encode_headers() は encode_response_headers() と同じ結果を返す
    #[test]
    fn prop_response_encode_headers_equals_free_function(
        status in status_code(),
        phrase in reason_phrase()
    ) {
        let res = Response::new(status, phrase)
            .unwrap()
            .header("Content-Type", "text/html")
            .unwrap();
        let via_method = res.encode_headers();
        let via_free = encode_response_headers(&res).unwrap();
        prop_assert_eq!(via_method, via_free);
    }
}

proptest! {
    /// Request::try_encode() は encode_request() と同じ結果を返す
    #[test]
    fn prop_request_try_encode_equals_free_function(method in http_method(), uri in uri()) {
        let req = Request::new(method, &uri)
            .unwrap()
            .header("Host", "example.com")
            .unwrap();
        let via_method = req.try_encode();
        let via_free = encode_request(&req);
        prop_assert_eq!(via_method, via_free);
    }
}

proptest! {
    /// Response::try_encode() は encode_response() と同じ結果を返す
    #[test]
    fn prop_response_try_encode_equals_free_function(status in status_code(), phrase in reason_phrase()) {
        let res = Response::new(status, phrase).unwrap();
        let via_method = res.try_encode();
        let via_free = encode_response(&res);
        prop_assert_eq!(via_method, via_free);
    }
}

proptest! {
    /// Request::try_encode_headers() は encode_request_headers() と同じ結果を返す
    #[test]
    fn prop_request_try_encode_headers_equals_free_function(method in http_method(), uri in uri()) {
        let req = Request::new(method, &uri)
            .unwrap()
            .header("Host", "example.com")
            .unwrap();
        let via_method = req.try_encode_headers();
        let via_free = encode_request_headers(&req);
        prop_assert_eq!(via_method, via_free);
    }
}

proptest! {
    /// Response::try_encode_headers() は encode_response_headers() と同じ結果を返す
    #[test]
    fn prop_response_try_encode_headers_equals_free_function(
        status in status_code(),
        phrase in reason_phrase()
    ) {
        let res = Response::new(status, phrase).unwrap();
        let via_method = res.try_encode_headers();
        let via_free = encode_response_headers(&res);
        prop_assert_eq!(via_method, via_free);
    }
}

// ========================================
// ResponseEncoder / RequestEncoder の PBT
// ========================================

proptest! {
    /// ResponseEncoder::new() で compress_body / finish / reset
    #[test]
    fn prop_response_encoder_compress_body(
        data in proptest::collection::vec(any::<u8>(), 1..256)
    ) {
        let mut encoder = ResponseEncoder::new();
        let mut output = vec![0u8; 512];

        let status = encoder.compress_body(&data, &mut output).unwrap();
        prop_assert_eq!(status.consumed(), data.len());
        prop_assert_eq!(status.produced(), data.len());
        prop_assert_eq!(&output[..data.len()], &data[..]);

        let finish_status = encoder.finish(&mut output).unwrap();
        prop_assert!(finish_status.is_complete());

        encoder.reset();
        let status2 = encoder.compress_body(&data, &mut output).unwrap();
        prop_assert_eq!(status2.consumed(), data.len());
    }
}

proptest! {
    /// RequestEncoder::new() で compress_body / finish / reset
    #[test]
    fn prop_request_encoder_compress_body(
        data in proptest::collection::vec(any::<u8>(), 1..256)
    ) {
        let mut encoder = RequestEncoder::new();
        let mut output = vec![0u8; 512];

        let status = encoder.compress_body(&data, &mut output).unwrap();
        prop_assert_eq!(status.consumed(), data.len());
        prop_assert_eq!(status.produced(), data.len());
        prop_assert_eq!(&output[..data.len()], &data[..]);

        let finish_status = encoder.finish(&mut output).unwrap();
        prop_assert!(finish_status.is_complete());

        encoder.reset();
        let status2 = encoder.compress_body(&data, &mut output).unwrap();
        prop_assert_eq!(status2.consumed(), data.len());
    }
}

proptest! {
    /// ResponseEncoder::default() は new() と同じ動作をする
    #[test]
    fn prop_response_encoder_default(
        data in proptest::collection::vec(any::<u8>(), 1..256)
    ) {
        let mut encoder: ResponseEncoder = ResponseEncoder::default();
        let mut output = vec![0u8; 512];

        let status = encoder.compress_body(&data, &mut output).unwrap();
        prop_assert_eq!(status.consumed(), data.len());
        prop_assert_eq!(status.produced(), data.len());
        prop_assert_eq!(&output[..data.len()], &data[..]);
    }
}

proptest! {
    /// RequestEncoder::default() は new() と同じ動作をする
    #[test]
    fn prop_request_encoder_default(
        data in proptest::collection::vec(any::<u8>(), 1..256)
    ) {
        let mut encoder: RequestEncoder = RequestEncoder::default();
        let mut output = vec![0u8; 512];

        let status = encoder.compress_body(&data, &mut output).unwrap();
        prop_assert_eq!(status.consumed(), data.len());
        prop_assert_eq!(status.produced(), data.len());
        prop_assert_eq!(&output[..data.len()], &data[..]);
    }
}

proptest! {
    /// ResponseEncoder::with_compressor(NoCompression) は new() と同じ動作をする
    #[test]
    fn prop_response_encoder_with_compressor(
        data in proptest::collection::vec(any::<u8>(), 1..256)
    ) {
        let compressor = shiguredo_http11::compression::NoCompression::new();
        let mut encoder = ResponseEncoder::with_compressor(compressor);
        let mut output = vec![0u8; 512];

        let status = encoder.compress_body(&data, &mut output).unwrap();
        prop_assert_eq!(status.consumed(), data.len());
        prop_assert_eq!(status.produced(), data.len());
        prop_assert_eq!(&output[..data.len()], &data[..]);
    }
}

proptest! {
    /// RequestEncoder::with_compressor(NoCompression) は new() と同じ動作をする
    #[test]
    fn prop_request_encoder_with_compressor(
        data in proptest::collection::vec(any::<u8>(), 1..256)
    ) {
        let compressor = shiguredo_http11::compression::NoCompression::new();
        let mut encoder = RequestEncoder::with_compressor(compressor);
        let mut output = vec![0u8; 512];

        let status = encoder.compress_body(&data, &mut output).unwrap();
        prop_assert_eq!(status.consumed(), data.len());
        prop_assert_eq!(status.produced(), data.len());
        prop_assert_eq!(&output[..data.len()], &data[..]);
    }
}
