//! エンコーダーのユニットテスト

use shiguredo_http11::{
    EncodeError, Request, Response, encode_chunk, encode_chunks, encode_request,
    encode_request_headers, encode_response, encode_response_headers,
};

// ========================================
// Request のテスト
// ========================================

#[test]
fn test_request_new() {
    let req = Request::new("GET", "/");
    assert_eq!(req.method, "GET");
    assert_eq!(req.uri, "/");
    assert_eq!(req.version, "HTTP/1.1");
    assert!(req.headers.is_empty());
    assert!(req.body.is_empty());
}

#[test]
fn test_request_get_headers() {
    let req = Request::new("GET", "/")
        .header("Accept", "text/html")
        .header("Accept", "application/json");
    let accepts = req.get_headers("Accept");
    assert_eq!(accepts.len(), 2);
    assert_eq!(accepts[0], "text/html");
    assert_eq!(accepts[1], "application/json");
}

#[test]
fn test_request_is_keep_alive() {
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
fn test_request_content_length() {
    let req = Request::new("POST", "/").header("Content-Length", "100");
    assert_eq!(req.content_length(), Some(100));

    let req = Request::new("GET", "/");
    assert_eq!(req.content_length(), None);
}

#[test]
fn test_request_is_chunked() {
    let req = Request::new("POST", "/").header("Transfer-Encoding", "chunked");
    assert!(req.is_chunked());

    let req = Request::new("POST", "/").header("Transfer-Encoding", "CHUNKED");
    assert!(req.is_chunked());

    let req = Request::new("GET", "/");
    assert!(!req.is_chunked());
}

#[test]
fn test_request_connection() {
    let req = Request::new("GET", "/").header("Connection", "keep-alive");
    assert_eq!(req.connection(), Some("keep-alive"));

    let req = Request::new("GET", "/");
    assert_eq!(req.connection(), None);
}

// ========================================
// Response のテスト
// ========================================

#[test]
fn test_response_new() {
    let res = Response::new(200, "OK");
    assert_eq!(res.version, "HTTP/1.1");
    assert_eq!(res.status_code, 200);
    assert_eq!(res.reason_phrase, "OK");
    assert!(res.headers.is_empty());
    assert!(res.body.is_empty());
}

#[test]
fn test_response_get_headers() {
    let res = Response::new(200, "OK")
        .header("Set-Cookie", "a=1")
        .header("Set-Cookie", "b=2");
    let cookies = res.get_headers("Set-Cookie");
    assert_eq!(cookies.len(), 2);
    assert_eq!(cookies[0], "a=1");
    assert_eq!(cookies[1], "b=2");
}

#[test]
fn test_response_status_categories() {
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
fn test_response_is_keep_alive() {
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
fn test_response_content_length() {
    let res = Response::new(200, "OK").header("Content-Length", "100");
    assert_eq!(res.content_length(), Some(100));

    let res = Response::new(200, "OK");
    assert_eq!(res.content_length(), None);
}

#[test]
fn test_response_is_chunked() {
    let res = Response::new(200, "OK").header("Transfer-Encoding", "chunked");
    assert!(res.is_chunked());

    let res = Response::new(200, "OK").header("Transfer-Encoding", "CHUNKED");
    assert!(res.is_chunked());

    let res = Response::new(200, "OK");
    assert!(!res.is_chunked());
}

// ========================================
// encode_request のテスト
// ========================================

#[test]
fn test_encode_request_with_existing_content_length() {
    // Content-Length が既に設定されている場合は追加しない
    // HTTP/1.1 には Host ヘッダーが必須
    let req = Request::new("POST", "/")
        .header("Host", "example.com")
        .header("Content-Length", "5")
        .body(b"hello".to_vec());
    let encoded = encode_request(&req).unwrap();
    let encoded_str = String::from_utf8_lossy(&encoded);

    // Content-Length が 1 回だけ出現
    let count = encoded_str.matches("Content-Length").count();
    assert_eq!(count, 1);
}

// ========================================
// encode_response のテスト
// ========================================

#[test]
fn test_encode_response_no_content_length_with_transfer_encoding() {
    // Transfer-Encoding がある場合は Content-Length を追加しない
    let res = Response::new(200, "OK")
        .header("Transfer-Encoding", "chunked")
        .body(b"hello".to_vec());
    let encoded = encode_response(&res).unwrap();
    let encoded_str = String::from_utf8_lossy(&encoded);

    assert!(!encoded_str.contains("Content-Length"));
}

// ========================================
// encode_chunk のテスト
// ========================================

#[test]
fn test_encode_chunk_empty() {
    // 空データは終端チャンク
    let encoded = encode_chunk(&[]);
    assert_eq!(encoded, b"0\r\n\r\n");
}

// ========================================
// encode_chunks のテスト
// ========================================

#[test]
fn test_encode_chunks_empty_list() {
    // 空リストでも終端チャンクは出力
    let encoded = encode_chunks(&[]);
    assert_eq!(encoded, b"0\r\n\r\n");
}

#[test]
fn test_encode_chunks_various_sizes() {
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

#[test]
fn test_encode_request_headers_ignores_body() {
    // HTTP/1.0 で Host なしを使用 (HTTP/1.1 では Host 必須なのでエラーになる)
    let req = Request::with_version("POST", "/", "HTTP/1.0").body(b"hello world".to_vec());
    let encoded = encode_request_headers(&req).unwrap();
    let encoded_str = String::from_utf8_lossy(&encoded);

    // ボディは含まれない
    assert!(!encoded_str.contains("hello world"));
    // Content-Length も自動追加されない
    assert!(!encoded_str.contains("Content-Length"));
}

// ========================================
// encode_response_headers のテスト
// ========================================

#[test]
fn test_encode_response_headers_ignores_body() {
    let res = Response::new(200, "OK").body(b"hello world".to_vec());
    let encoded = encode_response_headers(&res).unwrap();
    let encoded_str = String::from_utf8_lossy(&encoded);

    // ボディは含まれない
    assert!(!encoded_str.contains("hello world"));
}

// ========================================
// Host 必須チェックのテスト (RFC 9112 Section 3.2)
// ========================================

#[test]
fn test_encode_request_missing_host_http11() {
    // HTTP/1.1 で Host ヘッダーがない場合はエラー
    let req = Request::new("GET", "/");
    let result = encode_request(&req);
    assert!(matches!(result, Err(EncodeError::MissingHostHeader)));
}

#[test]
fn test_encode_request_with_host_http11() {
    // HTTP/1.1 で Host ヘッダーがある場合は成功
    let req = Request::new("GET", "/").header("Host", "example.com");
    let result = encode_request(&req);
    assert!(result.is_ok());
}

#[test]
fn test_encode_request_missing_host_http10() {
    // HTTP/1.0 で Host ヘッダーがない場合は成功 (Host は任意)
    let req = Request::with_version("GET", "/", "HTTP/1.0");
    let result = encode_request(&req);
    assert!(result.is_ok());
}

#[test]
fn test_encode_request_headers_missing_host_http11() {
    // encode_request_headers でも HTTP/1.1 で Host ヘッダーがない場合はエラー
    let req = Request::new("GET", "/");
    let result = encode_request_headers(&req);
    assert!(matches!(result, Err(EncodeError::MissingHostHeader)));
}

#[test]
fn test_request_try_encode_missing_host() {
    // Request::try_encode でも HTTP/1.1 で Host ヘッダーがない場合はエラー
    let req = Request::new("GET", "/");
    let result = req.try_encode();
    assert!(matches!(result, Err(EncodeError::MissingHostHeader)));
}

#[test]
fn test_request_try_encode_headers_missing_host() {
    // Request::try_encode_headers でも HTTP/1.1 で Host ヘッダーがない場合はエラー
    let req = Request::new("GET", "/");
    let result = req.try_encode_headers();
    assert!(matches!(result, Err(EncodeError::MissingHostHeader)));
}

// ========================================
// RFC 9112 Section 6.2: Transfer-Encoding と Content-Length の同時送信禁止
// ========================================

#[test]
fn test_encode_request_conflicting_te_and_cl() {
    // RFC 9112 Section 6.2: Transfer-Encoding と Content-Length を同時に送信してはならない
    let req = Request::new("POST", "/")
        .header("Host", "example.com")
        .header("Transfer-Encoding", "chunked")
        .header("Content-Length", "100");
    let result = encode_request(&req);
    assert!(matches!(
        result,
        Err(EncodeError::ConflictingTransferEncodingAndContentLength)
    ));
}

#[test]
fn test_encode_request_headers_conflicting_te_and_cl() {
    // encode_request_headers でも同様にエラー
    let req = Request::new("POST", "/")
        .header("Host", "example.com")
        .header("Transfer-Encoding", "chunked")
        .header("Content-Length", "100");
    let result = encode_request_headers(&req);
    assert!(matches!(
        result,
        Err(EncodeError::ConflictingTransferEncodingAndContentLength)
    ));
}

#[test]
fn test_request_try_encode_conflicting_te_and_cl() {
    // Request::try_encode でも同様にエラー
    let req = Request::new("POST", "/")
        .header("Host", "example.com")
        .header("Transfer-Encoding", "chunked")
        .header("Content-Length", "100");
    let result = req.try_encode();
    assert!(matches!(
        result,
        Err(EncodeError::ConflictingTransferEncodingAndContentLength)
    ));
}

#[test]
fn test_encode_response_conflicting_te_and_cl() {
    // RFC 9112 Section 6.2: レスポンスでも Transfer-Encoding と Content-Length を同時に送信してはならない
    let res = Response::new(200, "OK")
        .header("Transfer-Encoding", "chunked")
        .header("Content-Length", "100");
    let result = encode_response(&res);
    assert!(matches!(
        result,
        Err(EncodeError::ConflictingTransferEncodingAndContentLength)
    ));
}

#[test]
fn test_encode_response_headers_conflicting_te_and_cl() {
    // encode_response_headers でも同様にエラー
    let res = Response::new(200, "OK")
        .header("Transfer-Encoding", "chunked")
        .header("Content-Length", "100");
    let result = encode_response_headers(&res);
    assert!(matches!(
        result,
        Err(EncodeError::ConflictingTransferEncodingAndContentLength)
    ));
}

#[test]
fn test_response_try_encode_conflicting_te_and_cl() {
    // Response::try_encode でも同様にエラー
    let res = Response::new(200, "OK")
        .header("Transfer-Encoding", "chunked")
        .header("Content-Length", "100");
    let result = res.try_encode();
    assert!(matches!(
        result,
        Err(EncodeError::ConflictingTransferEncodingAndContentLength)
    ));
}

// ========================================
// RFC 9112 Section 6.1: 1xx / 204 レスポンスで Transfer-Encoding 禁止
// ========================================

#[test]
fn test_encode_response_1xx_with_te() {
    // RFC 9112 Section 6.1: 1xx レスポンスに Transfer-Encoding は禁止
    let res = Response::new(100, "Continue").header("Transfer-Encoding", "chunked");
    let result = encode_response(&res);
    assert!(matches!(
        result,
        Err(EncodeError::ForbiddenTransferEncoding { status_code: 100 })
    ));

    let res = Response::new(101, "Switching Protocols").header("Transfer-Encoding", "chunked");
    let result = encode_response(&res);
    assert!(matches!(
        result,
        Err(EncodeError::ForbiddenTransferEncoding { status_code: 101 })
    ));
}

#[test]
fn test_encode_response_204_with_te() {
    // RFC 9112 Section 6.1: 204 No Content レスポンスに Transfer-Encoding は禁止
    let res = Response::new(204, "No Content").header("Transfer-Encoding", "chunked");
    let result = encode_response(&res);
    assert!(matches!(
        result,
        Err(EncodeError::ForbiddenTransferEncoding { status_code: 204 })
    ));
}

#[test]
fn test_encode_response_headers_1xx_with_te() {
    // encode_response_headers でも同様にエラー
    let res = Response::new(100, "Continue").header("Transfer-Encoding", "chunked");
    let result = encode_response_headers(&res);
    assert!(matches!(
        result,
        Err(EncodeError::ForbiddenTransferEncoding { status_code: 100 })
    ));
}

#[test]
fn test_encode_response_headers_204_with_te() {
    // encode_response_headers でも同様にエラー
    let res = Response::new(204, "No Content").header("Transfer-Encoding", "chunked");
    let result = encode_response_headers(&res);
    assert!(matches!(
        result,
        Err(EncodeError::ForbiddenTransferEncoding { status_code: 204 })
    ));
}

#[test]
fn test_response_try_encode_1xx_with_te() {
    // Response::try_encode でも同様にエラー
    let res = Response::new(100, "Continue").header("Transfer-Encoding", "chunked");
    let result = res.try_encode();
    assert!(matches!(
        result,
        Err(EncodeError::ForbiddenTransferEncoding { status_code: 100 })
    ));
}

#[test]
fn test_response_try_encode_204_with_te() {
    // Response::try_encode でも同様にエラー
    let res = Response::new(204, "No Content").header("Transfer-Encoding", "chunked");
    let result = res.try_encode();
    assert!(matches!(
        result,
        Err(EncodeError::ForbiddenTransferEncoding { status_code: 204 })
    ));
}

// ========================================
// 正常ケース: Transfer-Encoding のみ、または Content-Length のみは許可
// ========================================

#[test]
fn test_encode_request_te_only_ok() {
    // Transfer-Encoding のみは OK
    let req = Request::new("POST", "/")
        .header("Host", "example.com")
        .header("Transfer-Encoding", "chunked");
    let result = encode_request(&req);
    assert!(result.is_ok());
}

#[test]
fn test_encode_request_cl_only_ok() {
    // Content-Length のみは OK
    let req = Request::new("POST", "/")
        .header("Host", "example.com")
        .header("Content-Length", "100")
        .body(vec![0u8; 100]);
    let result = encode_request(&req);
    assert!(result.is_ok());
}

#[test]
fn test_encode_response_te_only_ok() {
    // Transfer-Encoding のみは OK (2xx 以上)
    let res = Response::new(200, "OK").header("Transfer-Encoding", "chunked");
    let result = encode_response(&res);
    assert!(result.is_ok());
}

#[test]
fn test_encode_response_cl_only_ok() {
    // Content-Length のみは OK
    let res = Response::new(200, "OK")
        .header("Content-Length", "100")
        .body(vec![0u8; 100]);
    let result = encode_response(&res);
    assert!(result.is_ok());
}

#[test]
fn test_encode_response_1xx_without_te_ok() {
    // 1xx レスポンスで Transfer-Encoding がなければ OK
    let res = Response::new(100, "Continue");
    let result = encode_response(&res);
    assert!(result.is_ok());
}

#[test]
fn test_encode_response_204_without_te_ok() {
    // 204 レスポンスで Transfer-Encoding がなければ OK
    let res = Response::new(204, "No Content");
    let result = encode_response(&res);
    assert!(result.is_ok());
}

// ========================================
// 205 Reset Content ボディ禁止テスト
// ========================================

#[test]
fn test_encode_response_205_empty_body_ok() {
    // 205 で空ボディは OK
    let res = Response::new(205, "Reset Content");
    let result = encode_response(&res);
    assert!(result.is_ok());
}

#[test]
fn test_encode_response_205_with_body_error() {
    // 205 で非空ボディはエラー
    let res = Response::new(205, "Reset Content").body(b"hello".to_vec());
    let result = encode_response(&res);
    assert!(matches!(result, Err(EncodeError::ForbiddenBodyFor205)));
}

#[test]
fn test_encode_response_205_with_te_error() {
    // 205 で Transfer-Encoding はエラー
    let res = Response::new(205, "Reset Content").header("Transfer-Encoding", "chunked");
    let result = encode_response(&res);
    assert!(matches!(
        result,
        Err(EncodeError::ForbiddenTransferEncoding { status_code: 205 })
    ));
}

// ========================================
// RFC 9110 Section 8.6: 1xx / 204 レスポンスで Content-Length 禁止
// ========================================

#[test]
fn test_encode_response_1xx_with_cl() {
    // RFC 9110 Section 8.6: 1xx レスポンスに Content-Length は禁止
    let res = Response::new(100, "Continue").header("Content-Length", "0");
    let result = encode_response(&res);
    assert!(matches!(
        result,
        Err(EncodeError::ForbiddenContentLength { status_code: 100 })
    ));
}

#[test]
fn test_encode_response_204_with_cl() {
    // RFC 9110 Section 8.6: 204 No Content レスポンスに Content-Length は禁止
    let res = Response::new(204, "No Content").header("Content-Length", "0");
    let result = encode_response(&res);
    assert!(matches!(
        result,
        Err(EncodeError::ForbiddenContentLength { status_code: 204 })
    ));
}

#[test]
fn test_encode_response_headers_1xx_with_cl() {
    // encode_response_headers でも同様にエラー
    let res = Response::new(100, "Continue").header("Content-Length", "0");
    let result = encode_response_headers(&res);
    assert!(matches!(
        result,
        Err(EncodeError::ForbiddenContentLength { status_code: 100 })
    ));
}

#[test]
fn test_encode_response_headers_204_with_cl() {
    // encode_response_headers でも同様にエラー
    let res = Response::new(204, "No Content").header("Content-Length", "0");
    let result = encode_response_headers(&res);
    assert!(matches!(
        result,
        Err(EncodeError::ForbiddenContentLength { status_code: 204 })
    ));
}

#[test]
fn test_encode_response_205_with_cl_zero_ok() {
    // 205 で Content-Length: 0 は許可
    let res = Response::new(205, "Reset Content").header("Content-Length", "0");
    let result = encode_response(&res);
    assert!(result.is_ok());
}

#[test]
fn test_encode_response_205_with_cl_nonzero_error() {
    // 205 で Content-Length > 0 はエラー
    let res = Response::new(205, "Reset Content").header("Content-Length", "10");
    let result = encode_response(&res);
    assert!(matches!(
        result,
        Err(EncodeError::ForbiddenContentLength { status_code: 205 })
    ));
}

#[test]
fn test_encode_response_headers_205_with_cl_nonzero_error() {
    // encode_response_headers でも 205 + CL > 0 はエラー
    let res = Response::new(205, "Reset Content").header("Content-Length", "10");
    let result = encode_response_headers(&res);
    assert!(matches!(
        result,
        Err(EncodeError::ForbiddenContentLength { status_code: 205 })
    ));
}

// ========================================
// Host ヘッダーバリデーションテスト
// ========================================

#[test]
fn test_encode_request_duplicate_host_error() {
    // Host ヘッダーの重複はエラー
    let req = Request::new("GET", "/")
        .header("Host", "example.com")
        .header("Host", "other.com");
    let result = encode_request(&req);
    assert!(matches!(result, Err(EncodeError::DuplicateHostHeader)));
}

#[test]
fn test_encode_request_invalid_host_error() {
    // 不正な Host ヘッダー値はエラー
    let req = Request::new("GET", "/").header("Host", "exam ple.com");
    let result = encode_request(&req);
    assert!(matches!(result, Err(EncodeError::InvalidHostHeader { .. })));
}

#[test]
fn test_encode_request_host_authority_mismatch_error() {
    // Host と URI authority の不一致はエラー
    let req = Request::new("GET", "http://example.com/path").header("Host", "other.com");
    let result = encode_request(&req);
    assert!(matches!(
        result,
        Err(EncodeError::HostAuthorityMismatch { .. })
    ));
}

#[test]
fn test_encode_request_host_authority_match_ok() {
    // Host と URI authority の一致は OK
    let req = Request::new("GET", "http://example.com/path").header("Host", "example.com");
    let result = encode_request(&req);
    assert!(result.is_ok());
}

#[test]
fn test_encode_request_empty_host_ok() {
    // 空の Host ヘッダーは許可 (RFC 9112 Section 3.2)
    let req = Request::new("GET", "/").header("Host", "");
    let result = encode_request(&req);
    assert!(result.is_ok());
}

// ========================================
// CRLF/NUL インジェクション拒否テスト
// ========================================

#[test]
fn test_encode_request_crlf_in_method() {
    // メソッドに CRLF を含む場合はエラー
    for method in &["GET\r\nEvil: header", "POST\r\n", "GET\nEvil", "GET\rEvil"] {
        let req = Request::new(method, "/").header("Host", "example.com");
        let result = encode_request(&req);
        assert!(
            result.is_err(),
            "CRLF in method should be rejected: {:?}",
            method
        );
    }
}

#[test]
fn test_encode_request_crlf_in_uri() {
    // URI に CRLF を含む場合はエラー
    for uri in &["/path\r\nEvil: header", "/\r\n", "/test\nEvil"] {
        let req = Request::new("GET", uri).header("Host", "example.com");
        let result = encode_request(&req);
        assert!(result.is_err(), "CRLF in URI should be rejected: {:?}", uri);
    }
}

#[test]
fn test_encode_request_crlf_in_header_name() {
    // ヘッダー名に CRLF を含む場合はエラー
    for name in &["Evil\r\nHeader", "Evil\nHeader", "Evil\rHeader"] {
        let req = Request::new("GET", "/")
            .header("Host", "example.com")
            .header(name, "value");
        let result = encode_request(&req);
        assert!(
            result.is_err(),
            "CRLF in header name should be rejected: {:?}",
            name
        );
    }
}

#[test]
fn test_encode_request_crlf_in_header_value() {
    // ヘッダー値に CRLF を含む場合はエラー
    for value in &["evil\r\nEvil: injected", "evil\ninjected", "evil\rinjected"] {
        let req = Request::new("GET", "/")
            .header("Host", "example.com")
            .header("X-Test", value);
        let result = encode_request(&req);
        assert!(
            result.is_err(),
            "CRLF in header value should be rejected: {:?}",
            value
        );
    }
}

#[test]
fn test_encode_request_nul_in_header_value() {
    // ヘッダー値に NUL を含む場合はエラー
    let req = Request::new("GET", "/")
        .header("Host", "example.com")
        .header("X-Test", "evil\0value");
    let result = encode_request(&req);
    assert!(result.is_err(), "NUL in header value should be rejected");
}

#[test]
fn test_encode_response_crlf_in_reason_phrase() {
    // reason-phrase に CRLF を含む場合はエラー
    for phrase in &["OK\r\nEvil: header", "OK\n", "OK\r"] {
        let res = Response::new(200, phrase);
        let result = encode_response(&res);
        assert!(
            result.is_err(),
            "CRLF in reason-phrase should be rejected: {:?}",
            phrase
        );
    }
}

#[test]
fn test_encode_response_crlf_in_header_name() {
    // レスポンスでもヘッダー名に CRLF を含む場合はエラー
    for name in &["Evil\r\nHeader", "Evil\nHeader"] {
        let res = Response::new(200, "OK").header(name, "value");
        let result = encode_response(&res);
        assert!(
            result.is_err(),
            "CRLF in response header name should be rejected: {:?}",
            name
        );
    }
}

#[test]
fn test_encode_response_crlf_in_header_value() {
    // レスポンスでもヘッダー値に CRLF を含む場合はエラー
    for value in &["evil\r\nEvil: injected", "evil\ninjected"] {
        let res = Response::new(200, "OK").header("X-Test", value);
        let result = encode_response(&res);
        assert!(
            result.is_err(),
            "CRLF in response header value should be rejected: {:?}",
            value
        );
    }
}

// ========================================
// userinfo 除外テスト (RFC 9112 Section 3.2)
// ========================================

#[test]
fn test_encode_request_absolute_form_userinfo_match() {
    // absolute-form で userinfo 付き URI の Host 一致検証
    // userinfo を除外して比較するので一致する
    let req =
        Request::new("GET", "http://user:pass@example.com/path").header("Host", "example.com");
    let result = encode_request(&req);
    assert!(result.is_ok());
}

#[test]
fn test_encode_request_absolute_form_userinfo_mismatch() {
    // absolute-form で userinfo 付き URI の Host 不一致
    let req = Request::new("GET", "http://user:pass@example.com/path").header("Host", "other.com");
    let result = encode_request(&req);
    assert!(matches!(
        result,
        Err(EncodeError::HostAuthorityMismatch { .. })
    ));
}

#[test]
fn test_encode_request_absolute_form_userinfo_only_user() {
    // userinfo がユーザー名のみ (パスワードなし)
    let req = Request::new("GET", "http://user@example.com/path").header("Host", "example.com");
    let result = encode_request(&req);
    assert!(result.is_ok());
}

#[test]
fn test_encode_request_absolute_form_userinfo_with_port() {
    // userinfo + ポート番号
    let req =
        Request::new("GET", "http://user@example.com:8080/path").header("Host", "example.com:8080");
    let result = encode_request(&req);
    assert!(result.is_ok());
}

#[test]
fn test_encode_request_absolute_form_at_in_userinfo() {
    // userinfo に @ を含むケース (RFC 3986 では非推奨だが rfind で正しく処理)
    let req =
        Request::new("GET", "http://user%40name@example.com/path").header("Host", "example.com");
    let result = encode_request(&req);
    assert!(result.is_ok());
}
