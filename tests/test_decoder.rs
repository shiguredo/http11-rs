//! Decoder のユニットテスト

use shiguredo_http11::{BodyKind, RequestDecoder, ResponseDecoder};

// ========================================
// CONNECT トンネルモードのテスト (RFC 9112 Section 6.3)
// ========================================

/// CONNECT + 2xx レスポンスでトンネルモードになることを確認
#[test]
fn test_connect_2xx_tunnel_mode() {
    for status in [200, 201, 202, 204, 299] {
        let mut decoder = ResponseDecoder::new();
        decoder.set_request_method("CONNECT");

        let response = format!("HTTP/1.1 {} OK\r\nContent-Length: 100\r\n\r\n", status);
        decoder.feed(response.as_bytes()).unwrap();

        let result = decoder.decode_headers().unwrap();
        assert!(
            result.is_some(),
            "expected headers for CONNECT {} response",
            status
        );

        let (head, body_kind) = result.unwrap();
        assert_eq!(head.status_code, status);
        // CONNECT 2xx は Transfer-Encoding/Content-Length を無視してトンネルモードになる
        assert_eq!(
            body_kind,
            BodyKind::Tunnel,
            "expected Tunnel for CONNECT {} response",
            status
        );
        assert!(decoder.is_tunnel());
    }
}

/// CONNECT + 非 2xx レスポンスは通常のボディ判定
#[test]
fn test_connect_non_2xx_normal_body() {
    for status in [100, 101, 301, 400, 401, 403, 404, 500, 502, 503] {
        let mut decoder = ResponseDecoder::new();
        decoder.set_request_method("CONNECT");

        let response = format!(
            "HTTP/1.1 {} Error\r\nContent-Length: 5\r\n\r\nhello",
            status
        );
        decoder.feed(response.as_bytes()).unwrap();

        let result = decoder.decode_headers().unwrap();
        assert!(
            result.is_some(),
            "expected headers for CONNECT {} response",
            status
        );

        let (_head, body_kind) = result.unwrap();
        // 非 2xx はトンネルモードではない
        assert_ne!(
            body_kind,
            BodyKind::Tunnel,
            "expected non-Tunnel for CONNECT {} response",
            status
        );
        assert!(!decoder.is_tunnel());
    }
}

/// 非 CONNECT + 2xx レスポンスは通常のボディ判定
#[test]
fn test_non_connect_2xx_normal_body() {
    for method in ["GET", "POST", "PUT", "DELETE", "HEAD", "OPTIONS"] {
        let mut decoder = ResponseDecoder::new();
        decoder.set_request_method(method);

        let response = "HTTP/1.1 200 OK\r\nContent-Length: 5\r\n\r\nhello";
        decoder.feed(response.as_bytes()).unwrap();

        let result = decoder.decode_headers().unwrap();
        assert!(result.is_some(), "expected headers for {} response", method);

        let (_head, body_kind) = result.unwrap();
        // 非 CONNECT はトンネルモードではない
        assert_ne!(
            body_kind,
            BodyKind::Tunnel,
            "expected non-Tunnel for {} response",
            method
        );
        assert!(!decoder.is_tunnel());
    }
}

/// CONNECT 2xx で Transfer-Encoding/Content-Length は無視される
#[test]
fn test_connect_2xx_ignores_body_headers() {
    // Transfer-Encoding: chunked があってもトンネルモード
    let mut decoder = ResponseDecoder::new();
    decoder.set_request_method("CONNECT");

    let response = "HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n";
    decoder.feed(response.as_bytes()).unwrap();

    let result = decoder.decode_headers().unwrap().unwrap();
    assert_eq!(result.1, BodyKind::Tunnel);

    // Content-Length があってもトンネルモード
    let mut decoder = ResponseDecoder::new();
    decoder.set_request_method("CONNECT");

    let response = "HTTP/1.1 200 OK\r\nContent-Length: 1000\r\n\r\n";
    decoder.feed(response.as_bytes()).unwrap();

    let result = decoder.decode_headers().unwrap().unwrap();
    assert_eq!(result.1, BodyKind::Tunnel);
}

/// take_remaining() でヘッダー後のデータを取得
#[test]
fn test_connect_take_remaining() {
    let mut decoder = ResponseDecoder::new();
    decoder.set_request_method("CONNECT");

    let response = "HTTP/1.1 200 OK\r\n\r\ntunnel data here";
    decoder.feed(response.as_bytes()).unwrap();

    let result = decoder.decode_headers().unwrap().unwrap();
    assert_eq!(result.1, BodyKind::Tunnel);

    // take_remaining でトンネルデータを取得
    let remaining = decoder.take_remaining();
    assert_eq!(remaining, b"tunnel data here");

    // 2 回目は空
    let remaining = decoder.take_remaining();
    assert!(remaining.is_empty());
}

/// トンネルモードで decode_headers() を再度呼ぶとエラー
#[test]
fn test_connect_tunnel_decode_headers_error() {
    let mut decoder = ResponseDecoder::new();
    decoder.set_request_method("CONNECT");

    let response = "HTTP/1.1 200 OK\r\n\r\n";
    decoder.feed(response.as_bytes()).unwrap();

    // 最初の decode_headers は成功
    let result = decoder.decode_headers().unwrap().unwrap();
    assert_eq!(result.1, BodyKind::Tunnel);

    // トンネルモードで再度 decode_headers を呼ぶとエラー
    let result = decoder.decode_headers();
    assert!(result.is_err());
}

/// トンネルモードで decode() を呼ぶとエラー
#[test]
fn test_connect_tunnel_decode_error() {
    let mut decoder = ResponseDecoder::new();
    decoder.set_request_method("CONNECT");

    let response = "HTTP/1.1 200 OK\r\n\r\n";
    decoder.feed(response.as_bytes()).unwrap();

    // decode() はトンネルモードではエラー
    let result = decoder.decode();
    assert!(result.is_err());
}

// ========================================
// RFC 9112 Section 6.3 準拠テスト
// ========================================

// --- 修正1: 1xx/204/304/HEAD で TE/CL を無視 ---

/// 204 No Content で不正な Transfer-Encoding があってもエラーにならない
#[test]
fn test_204_ignores_invalid_te() {
    let mut decoder = ResponseDecoder::new();
    let response = "HTTP/1.1 204 No Content\r\nTransfer-Encoding: gzip\r\n\r\n";
    decoder.feed(response.as_bytes()).unwrap();

    let result = decoder.decode_headers().unwrap().unwrap();
    assert_eq!(result.1, BodyKind::None);
}

/// 304 Not Modified で不正な Transfer-Encoding があってもエラーにならない
#[test]
fn test_304_ignores_invalid_te() {
    let mut decoder = ResponseDecoder::new();
    let response = "HTTP/1.1 304 Not Modified\r\nTransfer-Encoding: gzip\r\n\r\n";
    decoder.feed(response.as_bytes()).unwrap();

    let result = decoder.decode_headers().unwrap().unwrap();
    assert_eq!(result.1, BodyKind::None);
}

/// HEAD レスポンスで不正な Transfer-Encoding があってもエラーにならない
#[test]
fn test_head_ignores_invalid_te() {
    let mut decoder = ResponseDecoder::new();
    decoder.set_expect_no_body(true); // HEAD リクエストへのレスポンス
    let response = "HTTP/1.1 200 OK\r\nTransfer-Encoding: gzip\r\n\r\n";
    decoder.feed(response.as_bytes()).unwrap();

    let result = decoder.decode_headers().unwrap().unwrap();
    assert_eq!(result.1, BodyKind::None);
}

/// HEAD レスポンスで不正な Content-Length があってもエラーにならない
#[test]
fn test_head_ignores_invalid_cl() {
    let mut decoder = ResponseDecoder::new();
    decoder.set_expect_no_body(true);
    // 通常は異なる値はエラーだが、HEAD では無視
    let response = "HTTP/1.1 200 OK\r\nContent-Length: 100\r\nContent-Length: 200\r\n\r\n";
    decoder.feed(response.as_bytes()).unwrap();

    let result = decoder.decode_headers().unwrap().unwrap();
    assert_eq!(result.1, BodyKind::None);
}

/// 204 で TE と CL の両方があってもエラーにならない
#[test]
fn test_204_ignores_te_and_cl() {
    let mut decoder = ResponseDecoder::new();
    let response =
        "HTTP/1.1 204 No Content\r\nTransfer-Encoding: chunked\r\nContent-Length: 100\r\n\r\n";
    decoder.feed(response.as_bytes()).unwrap();

    let result = decoder.decode_headers().unwrap().unwrap();
    assert_eq!(result.1, BodyKind::None);
}

// --- 修正2: Transfer-Encoding の RFC 準拠処理 ---

/// レスポンス Transfer-Encoding: gzip, chunked → Chunked
#[test]
fn test_response_te_gzip_chunked_is_chunked() {
    let mut decoder = ResponseDecoder::new();
    let response = "HTTP/1.1 200 OK\r\nTransfer-Encoding: gzip, chunked\r\n\r\n";
    decoder.feed(response.as_bytes()).unwrap();

    let result = decoder.decode_headers().unwrap().unwrap();
    assert_eq!(result.1, BodyKind::Chunked);
}

/// レスポンス Transfer-Encoding: gzip → CloseDelimited
#[test]
fn test_response_te_gzip_is_close_delimited() {
    let mut decoder = ResponseDecoder::new();
    let response = "HTTP/1.1 200 OK\r\nTransfer-Encoding: gzip\r\n\r\n";
    decoder.feed(response.as_bytes()).unwrap();

    let result = decoder.decode_headers().unwrap().unwrap();
    assert_eq!(result.1, BodyKind::CloseDelimited);
}

/// レスポンス Transfer-Encoding: chunked, gzip → CloseDelimited (chunked が最後でない)
#[test]
fn test_response_te_chunked_gzip_is_close_delimited() {
    let mut decoder = ResponseDecoder::new();
    let response = "HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked, gzip\r\n\r\n";
    decoder.feed(response.as_bytes()).unwrap();

    let result = decoder.decode_headers().unwrap().unwrap();
    assert_eq!(result.1, BodyKind::CloseDelimited);
}

/// レスポンス Transfer-Encoding: deflate, chunked → Chunked
#[test]
fn test_response_te_deflate_chunked_is_chunked() {
    let mut decoder = ResponseDecoder::new();
    let response = "HTTP/1.1 200 OK\r\nTransfer-Encoding: deflate, chunked\r\n\r\n";
    decoder.feed(response.as_bytes()).unwrap();

    let result = decoder.decode_headers().unwrap().unwrap();
    assert_eq!(result.1, BodyKind::Chunked);
}

/// レスポンスで Transfer-Encoding と Content-Length 両方ある場合、TE を優先
#[test]
fn test_response_te_and_cl_prefers_te() {
    let mut decoder = ResponseDecoder::new();
    let response = "HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\nContent-Length: 100\r\n\r\n";
    decoder.feed(response.as_bytes()).unwrap();

    let result = decoder.decode_headers().unwrap().unwrap();
    // TE が優先され、chunked フレーミング
    assert_eq!(result.1, BodyKind::Chunked);
}

/// リクエスト Transfer-Encoding: gzip → エラー
#[test]
fn test_request_te_gzip_error() {
    let mut decoder = RequestDecoder::new();
    let request = "POST / HTTP/1.1\r\nHost: localhost\r\nTransfer-Encoding: gzip\r\n\r\n";
    decoder.feed(request.as_bytes()).unwrap();

    let result = decoder.decode_headers();
    assert!(result.is_err());
}

/// リクエスト Transfer-Encoding: gzip, chunked → エラー
#[test]
fn test_request_te_gzip_chunked_error() {
    let mut decoder = RequestDecoder::new();
    let request = "POST / HTTP/1.1\r\nHost: localhost\r\nTransfer-Encoding: gzip, chunked\r\n\r\n";
    decoder.feed(request.as_bytes()).unwrap();

    let result = decoder.decode_headers();
    assert!(result.is_err());
}

/// リクエスト Transfer-Encoding: chunked → OK
#[test]
fn test_request_te_chunked_ok() {
    let mut decoder = RequestDecoder::new();
    let request = "POST / HTTP/1.1\r\nHost: localhost\r\nTransfer-Encoding: chunked\r\n\r\n";
    decoder.feed(request.as_bytes()).unwrap();

    let result = decoder.decode_headers().unwrap().unwrap();
    assert_eq!(result.1, BodyKind::Chunked);
}

// --- 修正3: Content-Length カンマ区切り対応 ---

/// Content-Length: 42, 42 → 42 として処理
#[test]
fn test_cl_comma_same_values_ok() {
    let mut decoder = ResponseDecoder::new();
    let response = "HTTP/1.1 200 OK\r\nContent-Length: 42, 42\r\n\r\n";
    decoder.feed(response.as_bytes()).unwrap();

    let result = decoder.decode_headers().unwrap().unwrap();
    assert_eq!(result.1, BodyKind::ContentLength(42));
}

/// Content-Length: 42, 42, 42 → 42 として処理
#[test]
fn test_cl_comma_three_same_values_ok() {
    let mut decoder = ResponseDecoder::new();
    let response = "HTTP/1.1 200 OK\r\nContent-Length: 42, 42, 42\r\n\r\n";
    decoder.feed(response.as_bytes()).unwrap();

    let result = decoder.decode_headers().unwrap().unwrap();
    assert_eq!(result.1, BodyKind::ContentLength(42));
}

/// Content-Length: 42, 43 → エラー (値が一致しない)
#[test]
fn test_cl_comma_different_values_error() {
    let mut decoder = ResponseDecoder::new();
    let response = "HTTP/1.1 200 OK\r\nContent-Length: 42, 43\r\n\r\n";
    decoder.feed(response.as_bytes()).unwrap();

    let result = decoder.decode_headers();
    assert!(result.is_err());
}

/// Content-Length: 42, → エラー (空の値)
#[test]
fn test_cl_comma_trailing_comma_error() {
    let mut decoder = ResponseDecoder::new();
    let response = "HTTP/1.1 200 OK\r\nContent-Length: 42,\r\n\r\n";
    decoder.feed(response.as_bytes()).unwrap();

    let result = decoder.decode_headers();
    assert!(result.is_err());
}

/// Content-Length: ,42 → エラー (空の値)
#[test]
fn test_cl_comma_leading_comma_error() {
    let mut decoder = ResponseDecoder::new();
    let response = "HTTP/1.1 200 OK\r\nContent-Length: ,42\r\n\r\n";
    decoder.feed(response.as_bytes()).unwrap();

    let result = decoder.decode_headers();
    assert!(result.is_err());
}
