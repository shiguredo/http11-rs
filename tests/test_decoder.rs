//! Decoder のユニットテスト

use shiguredo_http11::{BodyKind, BodyProgress, DecoderLimits, RequestDecoder, ResponseDecoder};

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

// ========================================
// consume_body エラーパスのテスト
// ========================================

/// consume_body(0) はエラー (progress() を使うべき)
#[test]
fn test_request_consume_body_zero_error() {
    let mut decoder = RequestDecoder::new();
    let request = "POST / HTTP/1.1\r\nHost: localhost\r\nContent-Length: 5\r\n\r\nhello";
    decoder.feed(request.as_bytes()).unwrap();
    decoder.decode_headers().unwrap().unwrap();

    let result = decoder.consume_body(0);
    assert!(result.is_err());
}

/// consume_body(0) はエラー (レスポンス)
#[test]
fn test_response_consume_body_zero_error() {
    let mut decoder = ResponseDecoder::new();
    let response = "HTTP/1.1 200 OK\r\nContent-Length: 5\r\n\r\nhello";
    decoder.feed(response.as_bytes()).unwrap();
    decoder.decode_headers().unwrap().unwrap();

    let result = decoder.consume_body(0);
    assert!(result.is_err());
}

/// トンネルモードで consume_body() はエラー
#[test]
fn test_response_consume_body_in_tunnel_error() {
    let mut decoder = ResponseDecoder::new();
    decoder.set_request_method("CONNECT");
    let response = "HTTP/1.1 200 OK\r\n\r\ndata";
    decoder.feed(response.as_bytes()).unwrap();
    decoder.decode_headers().unwrap().unwrap();

    let result = decoder.consume_body(4);
    assert!(result.is_err());
}

// ========================================
// ヘッダー値の制御文字エラーのテスト
// ========================================

/// ヘッダー値に制御文字 (NUL) を含むとエラー
#[test]
fn test_header_value_control_char_nul_error() {
    let data = b"GET / HTTP/1.1\r\nHost: localhost\r\nX-Bad: hello\x00world\r\n\r\n";
    let mut decoder = RequestDecoder::new();
    decoder.feed(data).unwrap();

    let result = decoder.decode_headers();
    assert!(result.is_err());
}

/// ヘッダー値に制御文字 (BEL) を含むとエラー
#[test]
fn test_header_value_control_char_bel_error() {
    let data = b"GET / HTTP/1.1\r\nHost: localhost\r\nX-Bad: hello\x07world\r\n\r\n";
    let mut decoder = RequestDecoder::new();
    decoder.feed(data).unwrap();

    let result = decoder.decode_headers();
    assert!(result.is_err());
}

// ========================================
// HTTP/1.0 + Transfer-Encoding のテスト
// ========================================

/// HTTP/1.0 リクエストで Transfer-Encoding: chunked はエラー
#[test]
fn test_request_http10_transfer_encoding_error() {
    let mut decoder = RequestDecoder::new();
    let request = "POST / HTTP/1.0\r\nTransfer-Encoding: chunked\r\n\r\n";
    decoder.feed(request.as_bytes()).unwrap();

    let result = decoder.decode_headers();
    assert!(result.is_err());
}

// ========================================
// トレーラー制限のテスト
// ========================================

/// トレーラーに禁止フィールド (Content-Length) を含むとエラー
#[test]
fn test_chunked_trailer_prohibited_field_error() {
    let mut decoder = ResponseDecoder::new();
    let response =
        "HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n0\r\nContent-Length: 0\r\n\r\n";
    decoder.feed(response.as_bytes()).unwrap();
    decoder.decode_headers().unwrap().unwrap();

    let result = decoder.progress();
    assert!(result.is_err());
}

/// トレーラーに禁止フィールド (Transfer-Encoding) を含むとエラー
#[test]
fn test_chunked_trailer_prohibited_transfer_encoding_error() {
    let mut decoder = ResponseDecoder::new();
    let response = "HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n0\r\nTransfer-Encoding: chunked\r\n\r\n";
    decoder.feed(response.as_bytes()).unwrap();
    decoder.decode_headers().unwrap().unwrap();

    let result = decoder.progress();
    assert!(result.is_err());
}

/// トレーラー数制限超過エラー
#[test]
fn test_chunked_trailer_too_many_error() {
    let limits = DecoderLimits {
        max_headers_count: 2,
        ..DecoderLimits::default()
    };
    let mut decoder = ResponseDecoder::with_limits(limits);
    // 3 つのトレーラーで制限 2 を超える
    let response = "HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n\
                    0\r\nX-A: 1\r\nX-B: 2\r\nX-C: 3\r\n\r\n";
    decoder.feed(response.as_bytes()).unwrap();
    decoder.decode_headers().unwrap().unwrap();

    let result = decoder.progress();
    assert!(result.is_err());
}

/// トレーラー行長制限超過エラー
#[test]
fn test_chunked_trailer_line_too_long_error() {
    // ヘッダー行 ("Transfer-Encoding: chunked" = 26 文字) は通過するが
    // トレーラー行が超過するサイズに設定
    let limits = DecoderLimits {
        max_header_line_size: 30,
        ..DecoderLimits::default()
    };
    let mut decoder = ResponseDecoder::with_limits(limits);
    // トレーラー行 "X-Trailer: " + 30 文字 = 41 文字 > 30
    let long_value = "a".repeat(30);
    let response = format!(
        "HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n0\r\nX-Trailer: {}\r\n\r\n",
        long_value
    );
    decoder.feed(response.as_bytes()).unwrap();
    decoder.decode_headers().unwrap().unwrap();

    let result = decoder.progress();
    assert!(result.is_err());
}

// ========================================
// BodyChunkedDataCrlf 分割到着のテスト
// ========================================

/// チャンクデータ後の CRLF が別フィードで到着する場合
#[test]
fn test_chunked_crlf_arrives_in_separate_feed() {
    let mut decoder = ResponseDecoder::new();
    // ヘッダーとチャンクサイズ + データを送る (CRLF なし)
    let response = "HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n5\r\nhello";
    decoder.feed(response.as_bytes()).unwrap();
    decoder.decode_headers().unwrap().unwrap();

    // progress でチャンクサイズを処理し、チャンクデータを利用可能にする
    let result = decoder.progress().unwrap();
    assert_eq!(result, BodyProgress::Continue);

    // チャンクデータを消費
    let peeked = decoder.peek_body().unwrap();
    assert_eq!(peeked, b"hello");
    let result = decoder.consume_body(5).unwrap();
    assert_eq!(result, BodyProgress::Continue);

    // CRLF + 終端チャンクを別フィードで送る
    decoder.feed(b"\r\n0\r\n\r\n").unwrap();
    let result = decoder.progress().unwrap();
    // BodyChunkedDataCrlf → BodyChunkedSize → 終端チャンク処理
    assert_eq!(result, BodyProgress::Continue);
    let result = decoder.progress().unwrap();
    assert_eq!(
        result,
        BodyProgress::Complete {
            trailers: Vec::new()
        }
    );
}

/// チャンクデータ後に不正な CRLF が到着
#[test]
fn test_chunked_invalid_crlf_in_separate_feed() {
    let mut decoder = ResponseDecoder::new();
    let response = "HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n5\r\nhello";
    decoder.feed(response.as_bytes()).unwrap();
    decoder.decode_headers().unwrap().unwrap();

    // progress でチャンクサイズを処理
    let result = decoder.progress().unwrap();
    assert_eq!(result, BodyProgress::Continue);

    // チャンクデータを消費
    let peeked = decoder.peek_body().unwrap();
    assert_eq!(peeked, b"hello");
    let result = decoder.consume_body(5).unwrap();
    assert_eq!(result, BodyProgress::Continue);

    // 不正な CRLF (LF LF)
    decoder.feed(b"\n\n").unwrap();
    let result = decoder.progress();
    assert!(result.is_err());
}

// ========================================
// Host ヘッダー検証のテスト (RFC 9112 Section 3.2)
// ========================================

/// HTTP/1.1 リクエストで Host ヘッダーがないとエラー
#[test]
fn test_request_http11_missing_host_error() {
    let mut decoder = RequestDecoder::new();
    let request = "GET / HTTP/1.1\r\n\r\n";
    decoder.feed(request.as_bytes()).unwrap();

    let result = decoder.decode_headers();
    assert!(result.is_err());
}

/// HTTP/1.1 リクエストで Host ヘッダーが複数あるとエラー
#[test]
fn test_request_http11_multiple_host_error() {
    let mut decoder = RequestDecoder::new();
    let request = "GET / HTTP/1.1\r\nHost: a.com\r\nHost: b.com\r\n\r\n";
    decoder.feed(request.as_bytes()).unwrap();

    let result = decoder.decode_headers();
    assert!(result.is_err());
}

/// HTTP/1.1 リクエストで空の Host ヘッダーは許可
#[test]
fn test_request_http11_empty_host_ok() {
    let mut decoder = RequestDecoder::new();
    let request = "GET / HTTP/1.1\r\nHost: \r\n\r\n";
    decoder.feed(request.as_bytes()).unwrap();

    let result = decoder.decode_headers().unwrap();
    assert!(result.is_some());
}

/// HTTP/1.1 リクエストで不正な Host ヘッダー値はエラー
#[test]
fn test_request_http11_invalid_host_value_error() {
    let mut decoder = RequestDecoder::new();
    let request = "GET / HTTP/1.1\r\nHost: :invalid:host:\r\n\r\n";
    decoder.feed(request.as_bytes()).unwrap();

    let result = decoder.decode_headers();
    assert!(result.is_err());
}

// ========================================
// Default トレイト実装のテスト
// ========================================

/// RequestDecoder::default() は new() と同等
#[test]
fn test_request_decoder_default() {
    let mut decoder = RequestDecoder::default();
    let request = "GET / HTTP/1.1\r\nHost: localhost\r\n\r\n";
    decoder.feed(request.as_bytes()).unwrap();
    let result = decoder.decode_headers().unwrap();
    assert!(result.is_some());
}

/// ResponseDecoder::default() は new() と同等
#[test]
fn test_response_decoder_default() {
    let mut decoder = ResponseDecoder::default();
    let response = "HTTP/1.1 200 OK\r\n\r\n";
    decoder.feed(response.as_bytes()).unwrap();
    let result = decoder.decode_headers().unwrap();
    assert!(result.is_some());
}

// ========================================
// decode() と streaming API 混在エラーのテスト
// ========================================

/// decode() をストリーミング API と混在して使うとエラー (リクエスト)
#[test]
fn test_request_decode_mixed_with_streaming_error() {
    let mut decoder = RequestDecoder::new();
    let request = "POST / HTTP/1.1\r\nHost: localhost\r\nContent-Length: 5\r\n\r\nhello";
    decoder.feed(request.as_bytes()).unwrap();

    // streaming API でヘッダーをデコード
    decoder.decode_headers().unwrap().unwrap();

    // その後 decode() を呼ぶとエラー
    let result = decoder.decode();
    assert!(result.is_err());
}

/// decode() をストリーミング API と混在して使うとエラー (レスポンス)
#[test]
fn test_response_decode_mixed_with_streaming_error() {
    let mut decoder = ResponseDecoder::new();
    let response = "HTTP/1.1 200 OK\r\nContent-Length: 5\r\n\r\nhello";
    decoder.feed(response.as_bytes()).unwrap();

    // streaming API でヘッダーをデコード
    decoder.decode_headers().unwrap().unwrap();

    // その後 decode() を呼ぶとエラー
    let result = decoder.decode();
    assert!(result.is_err());
}

// ========================================
// リクエスト行バリデーションのテスト
// ========================================

/// 不正な HTTP バージョン
#[test]
fn test_request_invalid_http_version_error() {
    let mut decoder = RequestDecoder::new();
    let request = "GET / HTTP/2.0\r\nHost: localhost\r\n\r\n";
    decoder.feed(request.as_bytes()).unwrap();

    let result = decoder.decode_headers();
    assert!(result.is_err());
}

/// 不正なメソッド名 (スペースを含む)
#[test]
fn test_request_invalid_method_error() {
    // メソッドに不正な文字を含む (トークン文字でない)
    let data = b"G\x01T / HTTP/1.1\r\nHost: localhost\r\n\r\n";
    let mut decoder = RequestDecoder::new();
    decoder.feed(data).unwrap();

    let result = decoder.decode_headers();
    assert!(result.is_err());
}

/// 不正な request-target (制御文字を含む)
#[test]
fn test_request_invalid_request_target_error() {
    let data = b"GET /path\x01invalid HTTP/1.1\r\nHost: localhost\r\n\r\n";
    let mut decoder = RequestDecoder::new();
    decoder.feed(data).unwrap();

    let result = decoder.decode_headers();
    assert!(result.is_err());
}

/// ボディデコード中に decode_headers() を呼ぶとエラー (リクエスト)
#[test]
fn test_request_decode_headers_during_body_error() {
    let mut decoder = RequestDecoder::new();
    let request = "POST / HTTP/1.1\r\nHost: localhost\r\nContent-Length: 100\r\n\r\nhello";
    decoder.feed(request.as_bytes()).unwrap();
    decoder.decode_headers().unwrap().unwrap();

    // ボディ未消費のまま decode_headers を再度呼ぶ
    // (Complete でないフェーズなのでエラー)
    // 注: ボディが残っているのでフェーズは BodyContentLength
    let result = decoder.decode_headers();
    assert!(result.is_err());
}

// ========================================
// レスポンス行バリデーションのテスト
// ========================================

/// 不正な HTTP バージョン (レスポンス)
#[test]
fn test_response_invalid_http_version_error() {
    let mut decoder = ResponseDecoder::new();
    let response = "HTTP/3.0 200 OK\r\n\r\n";
    decoder.feed(response.as_bytes()).unwrap();

    let result = decoder.decode_headers();
    assert!(result.is_err());
}

/// 範囲外ステータスコード (600)
#[test]
fn test_response_status_code_out_of_range_error() {
    let mut decoder = ResponseDecoder::new();
    let response = "HTTP/1.1 600 Error\r\n\r\n";
    decoder.feed(response.as_bytes()).unwrap();

    let result = decoder.decode_headers();
    assert!(result.is_err());
}

/// 範囲外ステータスコード (99)
#[test]
fn test_response_status_code_too_low_error() {
    let mut decoder = ResponseDecoder::new();
    let response = "HTTP/1.1 099 Error\r\n\r\n";
    decoder.feed(response.as_bytes()).unwrap();

    let result = decoder.decode_headers();
    assert!(result.is_err());
}

/// ボディデコード中に decode_headers() を呼ぶとエラー (レスポンス)
#[test]
fn test_response_decode_headers_during_body_error() {
    let mut decoder = ResponseDecoder::new();
    let response = "HTTP/1.1 200 OK\r\nContent-Length: 100\r\n\r\nhello";
    decoder.feed(response.as_bytes()).unwrap();
    decoder.decode_headers().unwrap().unwrap();

    // ボディ未消費のまま decode_headers を再度呼ぶ
    let result = decoder.decode_headers();
    assert!(result.is_err());
}

// ========================================
// consume_body の len 超過エラーのテスト
// ========================================

/// Content-Length で remaining を超える consume_body はエラー
#[test]
fn test_consume_body_exceeds_remaining_error() {
    let mut decoder = ResponseDecoder::new();
    let response = "HTTP/1.1 200 OK\r\nContent-Length: 5\r\n\r\nhello";
    decoder.feed(response.as_bytes()).unwrap();
    decoder.decode_headers().unwrap().unwrap();

    // 5 バイトしかないのに 10 バイト消費しようとする
    let result = decoder.consume_body(10);
    assert!(result.is_err());
}

/// close-delimited で buf を超える consume_body はエラー
#[test]
fn test_consume_body_exceeds_buffer_close_delimited_error() {
    let mut decoder = ResponseDecoder::new();
    let response = "HTTP/1.1 200 OK\r\n\r\nhello";
    decoder.feed(response.as_bytes()).unwrap();
    decoder.decode_headers().unwrap().unwrap();

    // バッファにある以上のバイト数を消費しようとする
    let result = decoder.consume_body(100);
    assert!(result.is_err());
}

// ========================================
// CONNECT リクエスト content 禁止テスト (RFC 9110 Section 9.3.6)
// ========================================

/// CONNECT リクエストに Content-Length があるとエラー
#[test]
fn test_connect_request_with_content_length_error() {
    let mut decoder = RequestDecoder::new();
    let request =
        "CONNECT example.com:443 HTTP/1.1\r\nHost: example.com:443\r\nContent-Length: 3\r\n\r\nabc";
    decoder.feed(request.as_bytes()).unwrap();

    let result = decoder.decode_headers();
    assert!(result.is_err());
    assert!(
        result.unwrap_err().to_string().contains("CONNECT"),
        "error should mention CONNECT"
    );
}

/// CONNECT リクエストに Transfer-Encoding があるとエラー
#[test]
fn test_connect_request_with_transfer_encoding_error() {
    let mut decoder = RequestDecoder::new();
    let request = "CONNECT example.com:443 HTTP/1.1\r\nHost: example.com:443\r\nTransfer-Encoding: chunked\r\n\r\n";
    decoder.feed(request.as_bytes()).unwrap();

    let result = decoder.decode_headers();
    assert!(result.is_err());
}

/// CONNECT リクエストで body なしは正常
#[test]
fn test_connect_request_no_body_ok() {
    let mut decoder = RequestDecoder::new();
    let request = "CONNECT example.com:443 HTTP/1.1\r\nHost: example.com:443\r\n\r\n";
    decoder.feed(request.as_bytes()).unwrap();

    let result = decoder.decode_headers();
    assert!(result.is_ok());
    let (head, body_kind) = result.unwrap().unwrap();
    assert_eq!(head.method, "CONNECT");
    assert_eq!(body_kind, BodyKind::None);
}
