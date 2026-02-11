//! エンコーダーのユニットテスト
//!
//! PBT でカバーできないエラーパス・境界値・エッジケースのみ記載する。

use shiguredo_http11::{
    EncodeError, Request, Response, encode_chunk, encode_chunks, encode_request,
    encode_request_headers, encode_response, encode_response_headers,
};

// ========================================
// encode_request のバリデーションエラーテスト
// ========================================

#[test]
fn test_encode_request_invalid_version() {
    // 空文字列はエラー
    let req = Request::with_version("GET", "/", "");
    let result = encode_request(&req);
    assert!(matches!(result, Err(EncodeError::InvalidVersion { .. })));

    // スペースを含むバージョンはエラー (SP は VCHAR ではない)
    let req = Request::with_version("GET", "/", "HTTP /1.1");
    let result = encode_request(&req);
    assert!(matches!(result, Err(EncodeError::InvalidVersion { .. })));

    // 制御文字を含むバージョンはエラー
    let req = Request::with_version("GET", "/", "HTTP\x00/1.1");
    let result = encode_request(&req);
    assert!(matches!(result, Err(EncodeError::InvalidVersion { .. })));
}

#[test]
fn test_encode_response_invalid_version() {
    // 空文字列はエラー
    let res = Response::with_version("", 200, "OK");
    let result = encode_response(&res);
    assert!(matches!(result, Err(EncodeError::InvalidVersion { .. })));

    // スペースを含むバージョンはエラー
    let res = Response::with_version("HTTP /1.1", 200, "OK");
    let result = encode_response(&res);
    assert!(matches!(result, Err(EncodeError::InvalidVersion { .. })));

    // 制御文字を含むバージョンはエラー
    let res = Response::with_version("HTTP\x01/1.1", 200, "OK");
    let result = encode_response(&res);
    assert!(matches!(result, Err(EncodeError::InvalidVersion { .. })));
}

#[test]
fn test_encode_response_invalid_status_code() {
    // 100 未満のステータスコードはエラー
    let res = Response::with_version("HTTP/1.1", 99, "Invalid");
    let result = encode_response(&res);
    assert!(matches!(
        result,
        Err(EncodeError::InvalidStatusCode { code: 99 })
    ));

    // 600 以上のステータスコードはエラー
    let res = Response::with_version("HTTP/1.1", 600, "Invalid");
    let result = encode_response(&res);
    assert!(matches!(
        result,
        Err(EncodeError::InvalidStatusCode { code: 600 })
    ));

    // 0 もエラー
    let res = Response::with_version("HTTP/1.1", 0, "Invalid");
    let result = encode_response(&res);
    assert!(matches!(
        result,
        Err(EncodeError::InvalidStatusCode { code: 0 })
    ));
}

// ========================================
// encode_request のエッジケーステスト
// ========================================

#[test]
fn test_encode_request_with_existing_content_length() {
    // Content-Length が既に設定されている場合は追加しない
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
// encode_response のエッジケーステスト
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
// encode_request_headers / encode_response_headers のテスト
// ========================================

#[test]
fn test_encode_request_headers_ignores_body() {
    let req = Request::with_version("POST", "/", "HTTP/1.0").body(b"hello world".to_vec());
    let encoded = encode_request_headers(&req).unwrap();
    let encoded_str = String::from_utf8_lossy(&encoded);

    // ボディは含まれない
    assert!(!encoded_str.contains("hello world"));
    // Content-Length も自動追加されない
    assert!(!encoded_str.contains("Content-Length"));
}

#[test]
fn test_encode_response_headers_ignores_body() {
    let res = Response::new(200, "OK").body(b"hello world".to_vec());
    let encoded = encode_response_headers(&res).unwrap();
    let encoded_str = String::from_utf8_lossy(&encoded);

    // ボディは含まれない
    assert!(!encoded_str.contains("hello world"));
}

// ========================================
// 正常ケース: Transfer-Encoding のみ、または Content-Length のみは許可
// ========================================

#[test]
fn test_encode_request_te_only_ok() {
    let req = Request::new("POST", "/")
        .header("Host", "example.com")
        .header("Transfer-Encoding", "chunked");
    let result = encode_request(&req);
    assert!(result.is_ok());
}

#[test]
fn test_encode_request_cl_only_ok() {
    let req = Request::new("POST", "/")
        .header("Host", "example.com")
        .header("Content-Length", "100")
        .body(vec![0u8; 100]);
    let result = encode_request(&req);
    assert!(result.is_ok());
}

#[test]
fn test_encode_response_te_only_ok() {
    let res = Response::new(200, "OK").header("Transfer-Encoding", "chunked");
    let result = encode_response(&res);
    assert!(result.is_ok());
}

#[test]
fn test_encode_response_cl_only_ok() {
    let res = Response::new(200, "OK")
        .header("Content-Length", "100")
        .body(vec![0u8; 100]);
    let result = encode_response(&res);
    assert!(result.is_ok());
}

// ========================================
// 205 Reset Content の正常ケーステスト
// ========================================

#[test]
fn test_encode_response_205_empty_body_ok() {
    let res = Response::new(205, "Reset Content");
    let result = encode_response(&res);
    assert!(result.is_ok());
}

#[test]
fn test_encode_response_205_with_cl_zero_ok() {
    // 205 で Content-Length: 0 は許可
    let res = Response::new(205, "Reset Content").header("Content-Length", "0");
    let result = encode_response(&res);
    assert!(result.is_ok());
}

// ========================================
// Host ヘッダーバリデーションテスト
// ========================================

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
    for uri in &["/path\r\nEvil: header", "/\r\n", "/test\nEvil"] {
        let req = Request::new("GET", uri).header("Host", "example.com");
        let result = encode_request(&req);
        assert!(result.is_err(), "CRLF in URI should be rejected: {:?}", uri);
    }
}

#[test]
fn test_encode_request_crlf_in_header_name() {
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
    let req = Request::new("GET", "/")
        .header("Host", "example.com")
        .header("X-Test", "evil\0value");
    let result = encode_request(&req);
    assert!(result.is_err(), "NUL in header value should be rejected");
}

#[test]
fn test_encode_response_crlf_in_reason_phrase() {
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
// userinfo テスト (RFC 9110 Section 4.2.4)
// ========================================

#[test]
fn test_encode_request_http_userinfo_rejected() {
    // RFC 9110 Section 4.2.4: http URI の userinfo は MUST NOT
    let req =
        Request::new("GET", "http://user:pass@example.com/path").header("Host", "example.com");
    assert!(matches!(
        encode_request(&req),
        Err(EncodeError::UserinfoInHttpUri { .. })
    ));
}

#[test]
fn test_encode_request_https_userinfo_rejected() {
    // RFC 9110 Section 4.2.4: https URI の userinfo は MUST NOT
    let req = Request::new("GET", "https://user@example.com/path").header("Host", "example.com");
    assert!(matches!(
        encode_request(&req),
        Err(EncodeError::UserinfoInHttpUri { .. })
    ));
}

#[test]
fn test_encode_request_http_userinfo_with_port_rejected() {
    let req =
        Request::new("GET", "http://user@example.com:8080/path").header("Host", "example.com:8080");
    assert!(matches!(
        encode_request(&req),
        Err(EncodeError::UserinfoInHttpUri { .. })
    ));
}

#[test]
fn test_encode_request_non_http_scheme_userinfo_allowed() {
    // http/https 以外のスキームでは userinfo は許可
    let req = Request::new("GET", "ftp://user@example.com/path").header("Host", "example.com");
    assert!(encode_request(&req).is_ok());
}

// ========================================
// Content-Length と body.len() の整合性検証テスト
// ========================================

#[test]
fn test_encode_response_content_length_mismatch() {
    // Content-Length と body.len() が不一致 → ContentLengthMismatch エラー
    let res = Response::new(200, "OK")
        .header("Content-Length", "10")
        .body(b"hello".to_vec());
    let result = encode_response(&res);
    assert!(matches!(
        result,
        Err(EncodeError::ContentLengthMismatch {
            header_value: 10,
            body_length: 5,
        })
    ));
}

#[test]
fn test_encode_request_content_length_mismatch() {
    // Content-Length と body.len() が不一致 → ContentLengthMismatch エラー
    let req = Request::new("POST", "/")
        .header("Host", "example.com")
        .header("Content-Length", "10")
        .body(b"hello".to_vec());
    let result = encode_request(&req);
    assert!(matches!(
        result,
        Err(EncodeError::ContentLengthMismatch {
            header_value: 10,
            body_length: 5,
        })
    ));
}

#[test]
fn test_encode_response_omit_content_length_skips_mismatch_check() {
    // omit_content_length: true の場合はミスマッチチェックをスキップ (HEAD レスポンス用)
    let res = Response::new(200, "OK")
        .header("Content-Length", "1000")
        .omit_content_length(true);
    let result = encode_response(&res);
    assert!(result.is_ok());
}

#[test]
fn test_encode_request_absolute_form_at_in_userinfo() {
    // RFC 9110 Section 4.2.4: userinfo の "@" は http URI で禁止
    let req =
        Request::new("GET", "http://user%40name@example.com/path").header("Host", "example.com");
    assert!(matches!(
        encode_request(&req),
        Err(EncodeError::UserinfoInHttpUri { .. })
    ));
}

// ========================================
// CONNECT リクエスト content 禁止テスト (RFC 9110 Section 9.3.6)
// ========================================

#[test]
fn test_encode_request_connect_with_body_error() {
    // CONNECT リクエストに body があるとエラー
    let req = Request::new("CONNECT", "example.com:443")
        .header("Host", "example.com:443")
        .body(b"hello".to_vec());
    let result = encode_request(&req);
    assert!(matches!(
        result,
        Err(EncodeError::ConnectRequestWithContent)
    ));
}

#[test]
fn test_encode_request_connect_with_content_length_error() {
    // CONNECT リクエストに Content-Length があるとエラー
    let req = Request::new("CONNECT", "example.com:443")
        .header("Host", "example.com:443")
        .header("Content-Length", "0");
    let result = encode_request(&req);
    assert!(matches!(
        result,
        Err(EncodeError::ConnectRequestWithContent)
    ));
}

#[test]
fn test_encode_request_connect_with_transfer_encoding_error() {
    // CONNECT リクエストに Transfer-Encoding があるとエラー
    let req = Request::new("CONNECT", "example.com:443")
        .header("Host", "example.com:443")
        .header("Transfer-Encoding", "chunked");
    let result = encode_request(&req);
    assert!(matches!(
        result,
        Err(EncodeError::ConnectRequestWithContent)
    ));
}

#[test]
fn test_encode_request_headers_connect_with_content_length_error() {
    // encode_request_headers でも CONNECT + CL は拒否
    let req = Request::new("CONNECT", "example.com:443")
        .header("Host", "example.com:443")
        .header("Content-Length", "0");
    let result = encode_request_headers(&req);
    assert!(matches!(
        result,
        Err(EncodeError::ConnectRequestWithContent)
    ));
}

#[test]
fn test_encode_request_connect_no_body_ok() {
    // CONNECT リクエストで body なしは OK
    let req = Request::new("CONNECT", "example.com:443").header("Host", "example.com:443");
    let result = encode_request(&req);
    assert!(result.is_ok());
}

// ========================================
// method/request-target form 整合性テスト (RFC 9112 Section 3.2)
// ========================================

#[test]
fn test_encode_request_connect_authority_form_ok() {
    // CONNECT は authority-form (host:port) のみ許可
    let req = Request::new("CONNECT", "example.com:443").header("Host", "example.com:443");
    let result = encode_request(&req);
    assert!(result.is_ok());
}

#[test]
fn test_encode_request_connect_origin_form_error() {
    // CONNECT で origin-form は不正
    let req = Request::new("CONNECT", "/path").header("Host", "example.com");
    let result = encode_request(&req);
    assert!(matches!(
        result,
        Err(EncodeError::InvalidRequestTargetForm { .. })
    ));
}

#[test]
fn test_encode_request_connect_absolute_form_error() {
    // CONNECT で absolute-form は不正
    let req = Request::new("CONNECT", "http://example.com/path").header("Host", "example.com");
    let result = encode_request(&req);
    assert!(matches!(
        result,
        Err(EncodeError::InvalidRequestTargetForm { .. })
    ));
}

#[test]
fn test_encode_request_connect_asterisk_form_error() {
    // CONNECT で asterisk-form は不正
    let req = Request::new("CONNECT", "*").header("Host", "example.com");
    let result = encode_request(&req);
    assert!(matches!(
        result,
        Err(EncodeError::InvalidRequestTargetForm { .. })
    ));
}

#[test]
fn test_encode_request_options_asterisk_form_ok() {
    // OPTIONS * は asterisk-form 許可
    let req = Request::new("OPTIONS", "*").header("Host", "example.com");
    let result = encode_request(&req);
    assert!(result.is_ok());
}

#[test]
fn test_encode_request_get_asterisk_form_error() {
    // GET で asterisk-form は不正
    let req = Request::new("GET", "*").header("Host", "example.com");
    let result = encode_request(&req);
    assert!(matches!(
        result,
        Err(EncodeError::InvalidRequestTargetForm { .. })
    ));
}

#[test]
fn test_encode_request_get_authority_form_error() {
    // GET で authority-form は不正
    let req = Request::new("GET", "example.com:80").header("Host", "example.com");
    let result = encode_request(&req);
    assert!(matches!(
        result,
        Err(EncodeError::InvalidRequestTargetForm { .. })
    ));
}

// ========================================
// absolute-form 判定テスト ("://" なし absolute-URI)
// ========================================

#[test]
fn test_encode_request_absolute_form_without_double_slash_ok() {
    // "://" を含まない absolute-URI (urn:isbn:...) は absolute-form
    let req = Request::with_version("GET", "urn:isbn:0451450523", "HTTP/1.0");
    let result = encode_request(&req);
    assert!(result.is_ok());
}

#[test]
fn test_encode_request_absolute_form_urn_nid_nss() {
    // urn:nid:nss 形式の absolute-URI
    let req = Request::with_version("GET", "urn:example:animal:ferret:nose", "HTTP/1.0");
    let result = encode_request(&req);
    assert!(result.is_ok());
}

#[test]
fn test_encode_request_authority_form_still_works() {
    // 通常の authority-form (host:port) は引き続き authority-form と判定
    let req = Request::new("CONNECT", "example.com:443").header("Host", "example.com:443");
    let result = encode_request(&req);
    assert!(result.is_ok());
}
