//! エンコーダーと Request/Response のプロパティテスト

use proptest::prelude::*;
use shiguredo_http11::{
    Request, Response, encode_chunk, encode_chunks, encode_request, encode_request_headers,
    encode_response, encode_response_headers,
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
        Just("Host".to_string()),
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

#[test]
fn request_new() {
    let req = Request::new("GET", "/");
    assert_eq!(req.method, "GET");
    assert_eq!(req.uri, "/");
    assert_eq!(req.version, "HTTP/1.1");
    assert!(req.headers.is_empty());
    assert!(req.body.is_empty());
}

proptest! {
    #[test]
    fn request_new_with_method_and_uri(method in http_method(), uri in uri()) {
        let req = Request::new(method, &uri);
        prop_assert_eq!(req.method, method);
        prop_assert_eq!(req.uri, uri);
        prop_assert_eq!(req.version, "HTTP/1.1");
    }
}

proptest! {
    #[test]
    fn request_with_version(method in http_method(), uri in uri(), version in http_version()) {
        let req = Request::with_version(method, &uri, version);
        prop_assert_eq!(req.method, method);
        prop_assert_eq!(req.uri, uri);
        prop_assert_eq!(req.version, version);
    }
}

proptest! {
    #[test]
    fn request_header_builder(name in header_name(), value in header_value()) {
        let req = Request::new("GET", "/").header(&name, &value);
        prop_assert_eq!(req.headers.len(), 1);
        prop_assert_eq!(&req.headers[0].0, &name);
        prop_assert_eq!(&req.headers[0].1, &value);
    }
}

proptest! {
    #[test]
    fn request_body_builder(data in body()) {
        let req = Request::new("POST", "/").body(data.clone());
        prop_assert_eq!(req.body, data);
    }
}

proptest! {
    #[test]
    fn request_add_header(name in header_name(), value in header_value()) {
        let mut req = Request::new("GET", "/");
        req.add_header(&name, &value);
        prop_assert_eq!(req.headers.len(), 1);
        prop_assert_eq!(&req.headers[0].0, &name);
        prop_assert_eq!(&req.headers[0].1, &value);
    }
}

proptest! {
    #[test]
    fn request_get_header(name in header_name(), value in header_value()) {
        let req = Request::new("GET", "/").header(&name, &value);
        prop_assert_eq!(req.get_header(&name), Some(value.as_str()));
        prop_assert_eq!(req.get_header(&name.to_uppercase()), Some(value.as_str()));
        prop_assert_eq!(req.get_header(&name.to_lowercase()), Some(value.as_str()));
    }
}

#[test]
fn request_get_headers() {
    let req = Request::new("GET", "/")
        .header("Accept", "text/html")
        .header("Accept", "application/json");
    let accepts = req.get_headers("Accept");
    assert_eq!(accepts.len(), 2);
    assert_eq!(accepts[0], "text/html");
    assert_eq!(accepts[1], "application/json");
}

proptest! {
    #[test]
    fn request_has_header(name in header_name(), value in header_value()) {
        let req = Request::new("GET", "/").header(&name, &value);
        prop_assert!(req.has_header(&name));
        prop_assert!(req.has_header(&name.to_uppercase()));
        prop_assert!(req.has_header(&name.to_lowercase()));
        prop_assert!(!req.has_header("NonExistent"));
    }
}

#[test]
fn request_is_keep_alive() {
    // HTTP/1.1 はデフォルトで keep-alive
    let req = Request::new("GET", "/");
    assert!(req.is_keep_alive());

    // Connection: close
    let req = Request::new("GET", "/").header("Connection", "close");
    assert!(!req.is_keep_alive());

    // Connection: keep-alive
    let req = Request::new("GET", "/").header("Connection", "keep-alive");
    assert!(req.is_keep_alive());

    // HTTP/1.0 はデフォルトで非 keep-alive
    let req = Request::with_version("GET", "/", "HTTP/1.0");
    assert!(!req.is_keep_alive());

    // HTTP/1.0 + Connection: keep-alive
    let req = Request::with_version("GET", "/", "HTTP/1.0").header("Connection", "keep-alive");
    assert!(req.is_keep_alive());
}

#[test]
fn request_content_length() {
    let req = Request::new("POST", "/").header("Content-Length", "100");
    assert_eq!(req.content_length(), Some(100));

    let req = Request::new("GET", "/");
    assert_eq!(req.content_length(), None);
}

#[test]
fn request_is_chunked() {
    let req = Request::new("POST", "/").header("Transfer-Encoding", "chunked");
    assert!(req.is_chunked());

    let req = Request::new("POST", "/").header("Transfer-Encoding", "CHUNKED");
    assert!(req.is_chunked());

    let req = Request::new("GET", "/");
    assert!(!req.is_chunked());
}

#[test]
fn request_connection() {
    let req = Request::new("GET", "/").header("Connection", "keep-alive");
    assert_eq!(req.connection(), Some("keep-alive"));

    let req = Request::new("GET", "/");
    assert_eq!(req.connection(), None);
}

proptest! {
    #[test]
    fn request_clone_eq(method in http_method(), uri in uri()) {
        let req = Request::new(method, &uri);
        let cloned = req.clone();
        prop_assert_eq!(req, cloned);
    }
}

// ========================================
// Response のテスト
// ========================================

#[test]
fn response_new() {
    let res = Response::new(200, "OK");
    assert_eq!(res.version, "HTTP/1.1");
    assert_eq!(res.status_code, 200);
    assert_eq!(res.reason_phrase, "OK");
    assert!(res.headers.is_empty());
    assert!(res.body.is_empty());
}

proptest! {
    #[test]
    fn response_new_with_status(status in status_code(), phrase in reason_phrase()) {
        let res = Response::new(status, phrase);
        prop_assert_eq!(res.status_code, status);
        prop_assert_eq!(res.reason_phrase, phrase);
        prop_assert_eq!(res.version, "HTTP/1.1");
    }
}

proptest! {
    #[test]
    fn response_with_version(version in http_version(), status in status_code(), phrase in reason_phrase()) {
        let res = Response::with_version(version, status, phrase);
        prop_assert_eq!(res.version, version);
        prop_assert_eq!(res.status_code, status);
        prop_assert_eq!(res.reason_phrase, phrase);
    }
}

proptest! {
    #[test]
    fn response_header_builder(name in header_name(), value in header_value()) {
        let res = Response::new(200, "OK").header(&name, &value);
        prop_assert_eq!(res.headers.len(), 1);
        prop_assert_eq!(&res.headers[0].0, &name);
        prop_assert_eq!(&res.headers[0].1, &value);
    }
}

proptest! {
    #[test]
    fn response_body_builder(data in body()) {
        let res = Response::new(200, "OK").body(data.clone());
        prop_assert_eq!(res.body, data);
    }
}

proptest! {
    #[test]
    fn response_add_header(name in header_name(), value in header_value()) {
        let mut res = Response::new(200, "OK");
        res.add_header(&name, &value);
        prop_assert_eq!(res.headers.len(), 1);
        prop_assert_eq!(&res.headers[0].0, &name);
        prop_assert_eq!(&res.headers[0].1, &value);
    }
}

proptest! {
    #[test]
    fn response_get_header(name in header_name(), value in header_value()) {
        let res = Response::new(200, "OK").header(&name, &value);
        prop_assert_eq!(res.get_header(&name), Some(value.as_str()));
        prop_assert_eq!(res.get_header(&name.to_uppercase()), Some(value.as_str()));
        prop_assert_eq!(res.get_header(&name.to_lowercase()), Some(value.as_str()));
    }
}

#[test]
fn response_get_headers() {
    let res = Response::new(200, "OK")
        .header("Set-Cookie", "a=1")
        .header("Set-Cookie", "b=2");
    let cookies = res.get_headers("Set-Cookie");
    assert_eq!(cookies.len(), 2);
    assert_eq!(cookies[0], "a=1");
    assert_eq!(cookies[1], "b=2");
}

proptest! {
    #[test]
    fn response_has_header(name in header_name(), value in header_value()) {
        let res = Response::new(200, "OK").header(&name, &value);
        prop_assert!(res.has_header(&name));
        prop_assert!(res.has_header(&name.to_uppercase()));
        prop_assert!(res.has_header(&name.to_lowercase()));
        prop_assert!(!res.has_header("NonExistent"));
    }
}

#[test]
fn response_status_categories() {
    // 1xx
    assert!(Response::new(100, "Continue").is_informational());
    assert!(Response::new(101, "Switching Protocols").is_informational());

    // 2xx
    assert!(Response::new(200, "OK").is_success());
    assert!(Response::new(201, "Created").is_success());
    assert!(Response::new(204, "No Content").is_success());

    // 3xx
    assert!(Response::new(301, "Moved Permanently").is_redirect());
    assert!(Response::new(302, "Found").is_redirect());
    assert!(Response::new(304, "Not Modified").is_redirect());

    // 4xx
    assert!(Response::new(400, "Bad Request").is_client_error());
    assert!(Response::new(401, "Unauthorized").is_client_error());
    assert!(Response::new(404, "Not Found").is_client_error());

    // 5xx
    assert!(Response::new(500, "Internal Server Error").is_server_error());
    assert!(Response::new(502, "Bad Gateway").is_server_error());
    assert!(Response::new(503, "Service Unavailable").is_server_error());
}

#[test]
fn response_is_keep_alive() {
    // HTTP/1.1 はデフォルトで keep-alive
    let res = Response::new(200, "OK");
    assert!(res.is_keep_alive());

    // Connection: close
    let res = Response::new(200, "OK").header("Connection", "close");
    assert!(!res.is_keep_alive());

    // Connection: keep-alive
    let res = Response::new(200, "OK").header("Connection", "keep-alive");
    assert!(res.is_keep_alive());

    // HTTP/1.0 はデフォルトで非 keep-alive
    let res = Response::with_version("HTTP/1.0", 200, "OK");
    assert!(!res.is_keep_alive());
}

#[test]
fn response_content_length() {
    let res = Response::new(200, "OK").header("Content-Length", "100");
    assert_eq!(res.content_length(), Some(100));

    let res = Response::new(200, "OK");
    assert_eq!(res.content_length(), None);
}

#[test]
fn response_is_chunked() {
    let res = Response::new(200, "OK").header("Transfer-Encoding", "chunked");
    assert!(res.is_chunked());

    let res = Response::new(200, "OK").header("Transfer-Encoding", "CHUNKED");
    assert!(res.is_chunked());

    let res = Response::new(200, "OK");
    assert!(!res.is_chunked());
}

proptest! {
    #[test]
    fn response_clone_eq(status in status_code(), phrase in reason_phrase()) {
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
    fn encode_request_basic(method in http_method(), uri in uri()) {
        let req = Request::new(method, &uri);
        let encoded = encode_request(&req);

        // リクエストラインを含む
        let request_line = format!("{} {} HTTP/1.1\r\n", method, uri);
        let encoded_str = String::from_utf8_lossy(&encoded);
        prop_assert!(encoded_str.starts_with(&request_line));
        prop_assert!(encoded_str.contains("\r\n\r\n"));
    }
}

proptest! {
    #[test]
    fn encode_request_with_headers(
        method in http_method(),
        uri in uri(),
        header_name in header_name(),
        header_value in header_value()
    ) {
        let req = Request::new(method, &uri).header(&header_name, &header_value);
        let encoded = encode_request(&req);
        let encoded_str = String::from_utf8_lossy(&encoded);

        let header_line = format!("{}: {}\r\n", header_name, header_value);
        prop_assert!(encoded_str.contains(&header_line));
    }
}

proptest! {
    #[test]
    fn encode_request_with_body(method in http_method(), uri in uri(), data in body()) {
        let req = Request::new(method, &uri).body(data.clone());
        let encoded = encode_request(&req);

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

#[test]
fn encode_request_with_existing_content_length() {
    // Content-Length が既に設定されている場合は追加しない
    let req = Request::new("POST", "/")
        .header("Content-Length", "5")
        .body(b"hello".to_vec());
    let encoded = encode_request(&req);
    let encoded_str = String::from_utf8_lossy(&encoded);

    // Content-Length が 1 回だけ出現
    let count = encoded_str.matches("Content-Length").count();
    assert_eq!(count, 1);
}

// ========================================
// encode_response のテスト
// ========================================

proptest! {
    #[test]
    fn encode_response_basic(status in status_code(), phrase in reason_phrase()) {
        let res = Response::new(status, phrase);
        let encoded = encode_response(&res);

        // ステータスラインを含む
        let status_line = format!("HTTP/1.1 {} {}\r\n", status, phrase);
        let encoded_str = String::from_utf8_lossy(&encoded);
        prop_assert!(encoded_str.starts_with(&status_line));
        prop_assert!(encoded_str.contains("\r\n\r\n"));
    }
}

proptest! {
    #[test]
    fn encode_response_with_headers(
        status in status_code(),
        phrase in reason_phrase(),
        header_name in header_name(),
        header_value in header_value()
    ) {
        let res = Response::new(status, phrase).header(&header_name, &header_value);
        let encoded = encode_response(&res);
        let encoded_str = String::from_utf8_lossy(&encoded);

        let header_line = format!("{}: {}\r\n", header_name, header_value);
        prop_assert!(encoded_str.contains(&header_line));
    }
}

proptest! {
    #[test]
    fn encode_response_with_body(status in status_code(), phrase in reason_phrase(), data in body()) {
        let res = Response::new(status, phrase).body(data.clone());
        let encoded = encode_response(&res);

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
    fn encode_response_omit_content_length(
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
        let encoded = encode_response(&res);
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
    fn encode_response_omit_content_length_no_header(
        status in 200..204u16
    ) {
        // omit_content_length=true で Content-Length ヘッダーも設定しない場合
        // Content-Length は自動付与されない (close-delimited になる)
        let res = Response::new(status, "OK")
            .omit_content_length(true);
        let encoded = encode_response(&res);
        let encoded_str = String::from_utf8_lossy(&encoded);

        // Content-Length は含まれない
        prop_assert!(!encoded_str.contains("Content-Length"));
    }
}

#[test]
fn encode_response_no_content_length_with_transfer_encoding() {
    // Transfer-Encoding がある場合は Content-Length を追加しない
    let res = Response::new(200, "OK")
        .header("Transfer-Encoding", "chunked")
        .body(b"hello".to_vec());
    let encoded = encode_response(&res);
    let encoded_str = String::from_utf8_lossy(&encoded);

    assert!(!encoded_str.contains("Content-Length"));
}

// ========================================
// encode_chunk のテスト
// ========================================

proptest! {
    #[test]
    fn encode_chunk_non_empty(data in proptest::collection::vec(prop::num::u8::ANY, 1..64)) {
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

#[test]
fn encode_chunk_empty() {
    // 空データは終端チャンク
    let encoded = encode_chunk(&[]);
    assert_eq!(encoded, b"0\r\n\r\n");
}

// ========================================
// encode_chunks のテスト
// ========================================

proptest! {
    #[test]
    fn encode_chunks_basic(count in 1usize..=5usize) {
        let chunks: Vec<&[u8]> = (0..count).map(|_| b"test".as_ref()).collect();
        let encoded = encode_chunks(&chunks);

        // 終端チャンクで終わる
        prop_assert!(encoded.ends_with(b"0\r\n\r\n"));

        // 各チャンクのサイズ行を含む
        let encoded_str = String::from_utf8_lossy(&encoded);
        prop_assert_eq!(encoded_str.matches("4\r\n").count(), count);
    }
}

#[test]
fn encode_chunks_empty_list() {
    // 空リストでも終端チャンクは出力
    let encoded = encode_chunks(&[]);
    assert_eq!(encoded, b"0\r\n\r\n");
}

#[test]
fn encode_chunks_various_sizes() {
    let chunks: Vec<&[u8]> = vec![b"a", b"bb", b"ccc", b"dddd"];
    let encoded = encode_chunks(&chunks);
    let encoded_str = String::from_utf8_lossy(&encoded);

    assert!(encoded_str.contains("1\r\na\r\n"));
    assert!(encoded_str.contains("2\r\nbb\r\n"));
    assert!(encoded_str.contains("3\r\nccc\r\n"));
    assert!(encoded_str.contains("4\r\ndddd\r\n"));
    assert!(encoded_str.ends_with("0\r\n\r\n"));
}

// ========================================
// encode_request_headers のテスト
// ========================================

proptest! {
    #[test]
    fn encode_request_headers_basic(method in http_method(), uri in uri()) {
        let req = Request::new(method, &uri)
            .header("Host", "example.com");
        let encoded = encode_request_headers(&req);
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

#[test]
fn encode_request_headers_ignores_body() {
    let req = Request::new("POST", "/").body(b"hello world".to_vec());
    let encoded = encode_request_headers(&req);
    let encoded_str = String::from_utf8_lossy(&encoded);

    // ボディは含まれない
    assert!(!encoded_str.contains("hello world"));
    // Content-Length も自動追加されない
    assert!(!encoded_str.contains("Content-Length"));
}

// ========================================
// encode_response_headers のテスト
// ========================================

proptest! {
    #[test]
    fn encode_response_headers_basic(status in status_code(), phrase in reason_phrase()) {
        let res = Response::new(status, phrase)
            .header("Content-Type", "text/html");
        let encoded = encode_response_headers(&res);
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

#[test]
fn encode_response_headers_ignores_body() {
    let res = Response::new(200, "OK").body(b"hello world".to_vec());
    let encoded = encode_response_headers(&res);
    let encoded_str = String::from_utf8_lossy(&encoded);

    // ボディは含まれない
    assert!(!encoded_str.contains("hello world"));
}

// ========================================
// Request::encode / Response::encode のテスト
// ========================================

proptest! {
    #[test]
    fn request_encode_method(method in http_method(), uri in uri()) {
        let req = Request::new(method, &uri);
        let encoded = req.encode();
        prop_assert_eq!(encoded, encode_request(&req));
    }
}

proptest! {
    #[test]
    fn response_encode_method(status in status_code(), phrase in reason_phrase()) {
        let res = Response::new(status, phrase);
        let encoded = res.encode();
        prop_assert_eq!(encoded, encode_response(&res));
    }
}

// ========================================
// Request::encode_headers / Response::encode_headers のテスト
// ========================================

proptest! {
    #[test]
    fn request_encode_headers_method(method in http_method(), uri in uri()) {
        let req = Request::new(method, &uri);
        let encoded = req.encode_headers();
        prop_assert_eq!(encoded, encode_request_headers(&req));
    }
}

proptest! {
    #[test]
    fn response_encode_headers_method(status in status_code(), phrase in reason_phrase()) {
        let res = Response::new(status, phrase);
        let encoded = res.encode_headers();
        prop_assert_eq!(encoded, encode_response_headers(&res));
    }
}
