//! Decoder のユニットテスト

use shiguredo_http11::{BodyKind, BodyProgress, DecoderLimits, RequestDecoder, ResponseDecoder};

// ========================================
// CONNECT トンネルモードのテスト (RFC 9110 Section 9.3.6 / RFC 9112 Section 6.3)
//
// RFC 9112 Section 6.3:
//   "Any 2xx (Successful) response to a CONNECT request implies that
//    the connection will become a tunnel immediately after the empty
//    line that concludes the header fields."
//
// RFC 9110 Section 9.3.6:
//   "A server MUST NOT send any Transfer-Encoding or Content-Length
//    header fields in a 2xx (Successful) response to CONNECT.
//    A client MUST ignore any Content-Length or Transfer-Encoding
//    header fields received in a successful response to CONNECT."
//
// デコーダーはクライアント側 (受信側) なので、CONNECT 2xx で TE/CL が
// 存在していてもエラーにせず無視し、BodyKind::Tunnel を返す。
// ========================================

/// CONNECT + 2xx レスポンスでトンネルモードになることを確認。
/// Content-Length が付いていても無視して Tunnel を返す (MUST ignore)。
///
/// 204 は除外する: RFC 9112 Section 6.3 の "in order of precedence" により
/// item 1 (1xx/204/304 はボディなし) が item 2 (CONNECT 2xx はトンネル) より
/// 優先されるため、CONNECT + 204 は `BodyKind::None` になる。
#[test]
fn test_connect_2xx_tunnel_mode() {
    for status in [200, 201, 202, 299] {
        let mut decoder = ResponseDecoder::new();
        decoder.set_request_method("CONNECT");

        // サーバーが MUST NOT に違反して Content-Length を付けても、
        // クライアントは MUST ignore に従い無視する
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
        assert_eq!(
            body_kind,
            BodyKind::Tunnel,
            "expected Tunnel for CONNECT {} response",
            status
        );
        assert!(decoder.is_tunnel());
    }
}

/// CONNECT + 204 No Content は `BodyKind::None` になることを確認。
///
/// RFC 9112 Section 6.3 の "in order of precedence" により item 1
/// (1xx/204/304 はボディなし) が item 2 (CONNECT 2xx はトンネル) より優先される。
/// このため CONNECT + 204 はトンネルモードに切り替わらず、ヘッダー終了で
/// メッセージが完了する。
#[test]
fn test_connect_204_no_body() {
    let mut decoder = ResponseDecoder::new();
    decoder.set_request_method("CONNECT");

    let response = "HTTP/1.1 204 No Content\r\n\r\n";
    decoder.feed(response.as_bytes()).unwrap();

    let (head, body_kind) = decoder.decode_headers().unwrap().unwrap();
    assert_eq!(head.status_code, 204);
    assert_eq!(body_kind, BodyKind::None);
    assert!(!decoder.is_tunnel());
}

/// CONNECT + 非 2xx レスポンスはトンネルモードにならず、通常のボディ判定に従う。
/// RFC 9112 Section 6.3: "Any response other than a successful response
/// indicates that the tunnel has not yet been formed."
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
        assert_ne!(
            body_kind,
            BodyKind::Tunnel,
            "expected non-Tunnel for CONNECT {} response",
            status
        );
        assert!(!decoder.is_tunnel());
    }
}

/// 非 CONNECT + 2xx レスポンスはトンネルモードにならない。
/// トンネルモードは CONNECT メソッドへの 2xx レスポンス限定。
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
        assert_ne!(
            body_kind,
            BodyKind::Tunnel,
            "expected non-Tunnel for {} response",
            method
        );
        assert!(!decoder.is_tunnel());
    }
}

/// CONNECT 2xx で Transfer-Encoding / Content-Length は無視される。
/// RFC 9110 Section 9.3.6:
///   "A client MUST ignore any Content-Length or Transfer-Encoding
///    header fields received in a successful response to CONNECT."
/// サーバーが MUST NOT に違反して送ってきても、エラーにせず無視する。
#[test]
fn test_connect_2xx_ignores_body_headers() {
    // Transfer-Encoding: chunked を無視して Tunnel
    let mut decoder = ResponseDecoder::new();
    decoder.set_request_method("CONNECT");
    let response = "HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n";
    decoder.feed(response.as_bytes()).unwrap();
    assert_eq!(
        decoder.decode_headers().unwrap().unwrap().1,
        BodyKind::Tunnel
    );

    // Content-Length: 1000 を無視して Tunnel
    let mut decoder = ResponseDecoder::new();
    decoder.set_request_method("CONNECT");
    let response = "HTTP/1.1 200 OK\r\nContent-Length: 1000\r\n\r\n";
    decoder.feed(response.as_bytes()).unwrap();
    assert_eq!(
        decoder.decode_headers().unwrap().unwrap().1,
        BodyKind::Tunnel
    );

    // Transfer-Encoding + Content-Length の両方があっても無視して Tunnel
    let mut decoder = ResponseDecoder::new();
    decoder.set_request_method("CONNECT");
    let response = "HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\nContent-Length: 100\r\n\r\n";
    decoder.feed(response.as_bytes()).unwrap();
    assert_eq!(
        decoder.decode_headers().unwrap().unwrap().1,
        BodyKind::Tunnel
    );
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
    decoder.set_request_method("HEAD"); // HEAD リクエストへのレスポンス
    let response = "HTTP/1.1 200 OK\r\nTransfer-Encoding: gzip\r\n\r\n";
    decoder.feed(response.as_bytes()).unwrap();

    let result = decoder.decode_headers().unwrap().unwrap();
    assert_eq!(result.1, BodyKind::None);
}

/// HEAD レスポンスで不正な Content-Length があってもエラーにならない
#[test]
fn test_head_ignores_invalid_cl() {
    let mut decoder = ResponseDecoder::new();
    decoder.set_request_method("HEAD");
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
    assert_eq!(result, BodyProgress::Advanced);

    // チャンクデータを消費 → CRLF 不足のため BodyChunkedDataCrlf で停止
    let peeked = decoder.peek_body().unwrap();
    assert_eq!(peeked, b"hello");
    let result = decoder.consume_body(5).unwrap();
    assert_eq!(result, BodyProgress::NeedData);

    // CRLF + 終端チャンクを別フィードで送る
    decoder.feed(b"\r\n0\r\n\r\n").unwrap();
    // 1 回目: BodyChunkedDataCrlf → BodyChunkedSize に遷移 (Advanced)
    let result = decoder.progress().unwrap();
    assert_eq!(result, BodyProgress::Advanced);
    // 2 回目: BodyChunkedSize で 0-size 行を処理し、トレーラ終端まで読みきって Complete
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
    assert_eq!(result, BodyProgress::Advanced);

    // チャンクデータを消費 → CRLF 不足のため BodyChunkedDataCrlf で停止
    let peeked = decoder.peek_body().unwrap();
    assert_eq!(peeked, b"hello");
    let result = decoder.consume_body(5).unwrap();
    assert_eq!(result, BodyProgress::NeedData);

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

/// 不正なプロトコルバージョン (token "/" DIGIT+ "." DIGIT+ でない形式)
#[test]
fn test_request_invalid_protocol_version_error() {
    // "/" がない
    let mut decoder = RequestDecoder::new();
    let request = "GET / INVALID\r\nHost: localhost\r\n\r\n";
    decoder.feed(request.as_bytes()).unwrap();
    assert!(decoder.decode_headers().is_err());

    // "/" の後にドットがない
    let mut decoder = RequestDecoder::new();
    let request = "GET / HTTP/11\r\nHost: localhost\r\n\r\n";
    decoder.feed(request.as_bytes()).unwrap();
    assert!(decoder.decode_headers().is_err());

    // ドットの後に数字がない
    let mut decoder = RequestDecoder::new();
    let request = "GET / HTTP/1.\r\nHost: localhost\r\n\r\n";
    decoder.feed(request.as_bytes()).unwrap();
    assert!(decoder.decode_headers().is_err());

    // バージョン部分に 3 つのドット区切り
    let mut decoder = RequestDecoder::new();
    let request = "GET / HTTP/1.1.1\r\nHost: localhost\r\n\r\n";
    decoder.feed(request.as_bytes()).unwrap();
    assert!(decoder.decode_headers().is_err());
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

/// 不正なプロトコルバージョン (レスポンス)
#[test]
fn test_response_invalid_protocol_version_error() {
    // "/" がない
    let mut decoder = ResponseDecoder::new();
    let response = "INVALID 200 OK\r\n\r\n";
    decoder.feed(response.as_bytes()).unwrap();
    assert!(decoder.decode_headers().is_err());

    // "/" の後にドットがない
    let mut decoder = ResponseDecoder::new();
    let response = "HTTP/11 200 OK\r\n\r\n";
    decoder.feed(response.as_bytes()).unwrap();
    assert!(decoder.decode_headers().is_err());

    // ドットの後に数字がない
    let mut decoder = ResponseDecoder::new();
    let response = "HTTP/1. 200 OK\r\n\r\n";
    decoder.feed(response.as_bytes()).unwrap();
    assert!(decoder.decode_headers().is_err());
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
// CONNECT リクエストのテスト (RFC 9110 Section 9.3.6)
//
// RFC 9110 Section 9.3.6:
//   "A CONNECT request message does not have content."
//
// CONNECT リクエストは content を持たないため、Content-Length / Transfer-Encoding が
// 付いていても body として読まず、BodyKind::None として扱う。
// ヘッダーの存在自体では reject しない (RFC は MUST NOT としていない)。
//
// Content-Length については RFC 9110 Section 8.6 で:
//   "A user agent SHOULD NOT send a Content-Length header field when
//    the request message does not contain content and the method
//    semantics do not anticipate such data."
// と SHOULD NOT に留まる。
// ========================================

/// CONNECT リクエストは Content-Length / Transfer-Encoding が付いていても
/// body として読まず、常に BodyKind::None を返す。
/// ヘッダーの存在だけでは reject しない。
#[test]
fn test_connect_request_no_body() {
    // Content-Length: N > 0 が付いていても BodyKind::None
    let mut decoder = RequestDecoder::new();
    let request =
        "CONNECT example.com:443 HTTP/1.1\r\nHost: example.com:443\r\nContent-Length: 3\r\n\r\nabc";
    decoder.feed(request.as_bytes()).unwrap();
    let (head, body_kind) = decoder.decode_headers().unwrap().unwrap();
    assert_eq!(head.method, "CONNECT");
    assert_eq!(body_kind, BodyKind::None);

    // Content-Length: 0 でも BodyKind::None
    let mut decoder = RequestDecoder::new();
    let request =
        "CONNECT example.com:443 HTTP/1.1\r\nHost: example.com:443\r\nContent-Length: 0\r\n\r\n";
    decoder.feed(request.as_bytes()).unwrap();
    let (head, body_kind) = decoder.decode_headers().unwrap().unwrap();
    assert_eq!(head.method, "CONNECT");
    assert_eq!(body_kind, BodyKind::None);

    // Transfer-Encoding: chunked でも BodyKind::None
    let mut decoder = RequestDecoder::new();
    let request = "CONNECT example.com:443 HTTP/1.1\r\nHost: example.com:443\r\nTransfer-Encoding: chunked\r\n\r\n";
    decoder.feed(request.as_bytes()).unwrap();
    let (head, body_kind) = decoder.decode_headers().unwrap().unwrap();
    assert_eq!(head.method, "CONNECT");
    assert_eq!(body_kind, BodyKind::None);

    // ヘッダーなし (最も一般的なケース) → BodyKind::None
    let mut decoder = RequestDecoder::new();
    let request = "CONNECT example.com:443 HTTP/1.1\r\nHost: example.com:443\r\n\r\n";
    decoder.feed(request.as_bytes()).unwrap();
    let (head, body_kind) = decoder.decode_headers().unwrap().unwrap();
    assert_eq!(head.method, "CONNECT");
    assert_eq!(body_kind, BodyKind::None);
}

// ========================================
// HTTP/1.0 + Transfer-Encoding 拒否テスト (RFC 9112 Section 6.1)
// ========================================

#[test]
fn test_response_http10_transfer_encoding_error() {
    // HTTP/1.0 + Transfer-Encoding は framing fault
    let mut decoder = ResponseDecoder::new();
    let response = "HTTP/1.0 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n";
    decoder.feed(response.as_bytes()).unwrap();

    let result = decoder.decode_headers();
    assert!(result.is_err());
}

#[test]
fn test_response_http11_transfer_encoding_ok() {
    // HTTP/1.1 + Transfer-Encoding は正常
    let mut decoder = ResponseDecoder::new();
    let response = "HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n";
    decoder.feed(response.as_bytes()).unwrap();

    let result = decoder.decode_headers();
    assert!(result.is_ok());
}

// ========================================
// http/https 空 host 拒否テスト (RFC 9110 Section 4.2)
// ========================================

#[test]
fn test_request_http_empty_host_error() {
    // http:///path は空 host で不正
    let mut decoder = RequestDecoder::new();
    let request = "GET http:///path HTTP/1.1\r\nHost: \r\n\r\n";
    decoder.feed(request.as_bytes()).unwrap();

    let result = decoder.decode_headers();
    assert!(result.is_err());
}

#[test]
fn test_request_https_port_only_host_error() {
    // https://:443/path は空 host で不正
    let mut decoder = RequestDecoder::new();
    let request = "GET https://:443/path HTTP/1.1\r\nHost: \r\n\r\n";
    decoder.feed(request.as_bytes()).unwrap();

    let result = decoder.decode_headers();
    assert!(result.is_err());
}

// ========================================
// chunked パラメータ拒否テスト (RFC 9112 Section 7.1)
// ========================================

#[test]
fn test_request_chunked_with_params_error() {
    // chunked; param=value はエラー
    let mut decoder = RequestDecoder::new();
    let request =
        "POST / HTTP/1.1\r\nHost: example.com\r\nTransfer-Encoding: chunked; q=1.0\r\n\r\n";
    decoder.feed(request.as_bytes()).unwrap();

    let result = decoder.decode_headers();
    assert!(result.is_err());
}

#[test]
fn test_response_chunked_with_params_error() {
    // chunked; param=value はエラー
    let mut decoder = ResponseDecoder::new();
    let response = "HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked; q=1.0\r\n\r\n";
    decoder.feed(response.as_bytes()).unwrap();

    let result = decoder.decode_headers();
    assert!(result.is_err());
}

// ========================================
// http/https URI の :// 必須検証 (RFC 9110 Section 4.2)
// ========================================

#[test]
fn test_request_http_without_double_slash_error() {
    // http:foo は "://" がないので不正
    let mut decoder = RequestDecoder::new();
    let request = "GET http:foo HTTP/1.1\r\nHost: \r\n\r\n";
    decoder.feed(request.as_bytes()).unwrap();

    let result = decoder.decode_headers();
    assert!(result.is_err());
}

#[test]
fn test_request_https_without_double_slash_error() {
    // https:path は "://" がないので不正
    let mut decoder = RequestDecoder::new();
    let request = "GET https:path HTTP/1.1\r\nHost: \r\n\r\n";
    decoder.feed(request.as_bytes()).unwrap();

    let result = decoder.decode_headers();
    assert!(result.is_err());
}

#[test]
fn test_request_http_with_double_slash_ok() {
    // http://example.com/path は正常
    let mut decoder = RequestDecoder::new();
    let request = "GET http://example.com/path HTTP/1.1\r\nHost: example.com\r\n\r\n";
    decoder.feed(request.as_bytes()).unwrap();

    let result = decoder.decode_headers();
    assert!(result.is_ok());
}

// ========================================
// 直接書き込み API (mut_buf / advance_buf / available_buf) のテスト
// ========================================

mod direct_buffer_write {
    use super::*;
    use shiguredo_http11::Error;

    #[test]
    fn response_mut_buf_zero_returns_empty_slice() {
        let mut decoder = ResponseDecoder::new();
        let buf = decoder.mut_buf(0).unwrap();
        assert!(buf.is_empty());
        decoder.advance_buf(0);
        assert_eq!(decoder.remaining().len(), 0);
    }

    #[test]
    fn request_mut_buf_zero_returns_empty_slice() {
        let mut decoder = RequestDecoder::new();
        let buf = decoder.mut_buf(0).unwrap();
        assert!(buf.is_empty());
        decoder.advance_buf(0);
        assert_eq!(decoder.remaining().len(), 0);
    }

    #[test]
    fn response_advance_zero_drops_pending() {
        let mut decoder = ResponseDecoder::new();
        let _ = decoder.mut_buf(64).unwrap();
        decoder.advance_buf(0);
        // pending が破棄されてバッファは空のまま
        assert_eq!(decoder.remaining().len(), 0);
    }

    #[test]
    fn request_advance_zero_drops_pending() {
        let mut decoder = RequestDecoder::new();
        let _ = decoder.mut_buf(64).unwrap();
        decoder.advance_buf(0);
        assert_eq!(decoder.remaining().len(), 0);
    }

    #[test]
    fn response_advance_partial_drops_remainder() {
        let mut decoder = ResponseDecoder::new();
        let buf = decoder.mut_buf(16).unwrap();
        buf[..4].copy_from_slice(b"abcd");
        decoder.advance_buf(4);
        assert_eq!(decoder.remaining(), b"abcd");
    }

    #[test]
    fn request_advance_partial_drops_remainder() {
        let mut decoder = RequestDecoder::new();
        let buf = decoder.mut_buf(16).unwrap();
        buf[..4].copy_from_slice(b"abcd");
        decoder.advance_buf(4);
        assert_eq!(decoder.remaining(), b"abcd");
    }

    #[test]
    fn response_mut_buf_overflow_preserves_state() {
        let limits = DecoderLimits {
            max_buffer_size: 16,
            ..Default::default()
        };
        let mut decoder = ResponseDecoder::with_limits(limits);
        decoder.feed(b"hello").unwrap();
        let prev = decoder.remaining().to_vec();

        match decoder.mut_buf(100) {
            Err(Error::BufferOverflow { size, limit }) => {
                assert_eq!(size, 105);
                assert_eq!(limit, 16);
            }
            other => panic!("expected BufferOverflow, got {:?}", other.is_ok()),
        }
        assert_eq!(decoder.remaining(), prev.as_slice());
    }

    #[test]
    fn request_mut_buf_overflow_preserves_state() {
        let limits = DecoderLimits {
            max_buffer_size: 16,
            ..Default::default()
        };
        let mut decoder = RequestDecoder::with_limits(limits);
        decoder.feed(b"hello").unwrap();
        let prev = decoder.remaining().to_vec();

        match decoder.mut_buf(100) {
            Err(Error::BufferOverflow { size, limit }) => {
                assert_eq!(size, 105);
                assert_eq!(limit, 16);
            }
            other => panic!("expected BufferOverflow, got {:?}", other.is_ok()),
        }
        assert_eq!(decoder.remaining(), prev.as_slice());
    }

    #[test]
    fn response_consecutive_mut_buf_drops_previous_pending() {
        let mut decoder = ResponseDecoder::new();
        let buf = decoder.mut_buf(32).unwrap();
        buf[..3].copy_from_slice(b"foo");
        // advance_buf を呼ばずに 2 回目の mut_buf
        let buf2 = decoder.mut_buf(8).unwrap();
        assert_eq!(buf2.len(), 8);
        decoder.advance_buf(3);
        // 1 回目の pending は破棄される
        assert_eq!(decoder.remaining().len(), 3);
    }

    #[test]
    fn request_consecutive_mut_buf_drops_previous_pending() {
        let mut decoder = RequestDecoder::new();
        let buf = decoder.mut_buf(32).unwrap();
        buf[..3].copy_from_slice(b"foo");
        let buf2 = decoder.mut_buf(8).unwrap();
        assert_eq!(buf2.len(), 8);
        decoder.advance_buf(3);
        assert_eq!(decoder.remaining().len(), 3);
    }

    #[test]
    #[cfg(debug_assertions)]
    #[should_panic(expected = "feed called with pending mut_buf")]
    fn response_feed_with_pending_panics_in_debug() {
        let mut decoder = ResponseDecoder::new();
        let _ = decoder.mut_buf(8).unwrap();
        let _ = decoder.feed(b"x");
    }

    #[test]
    #[cfg(debug_assertions)]
    #[should_panic(expected = "feed called with pending mut_buf")]
    fn request_feed_with_pending_panics_in_debug() {
        let mut decoder = RequestDecoder::new();
        let _ = decoder.mut_buf(8).unwrap();
        let _ = decoder.feed(b"x");
    }

    #[test]
    fn response_reset_clears_pending() {
        let mut decoder = ResponseDecoder::new();
        let _ = decoder.mut_buf(64).unwrap();
        decoder.reset();
        // 再度 mut_buf を呼んでも問題なし
        let buf = decoder.mut_buf(8).unwrap();
        assert_eq!(buf.len(), 8);
        decoder.advance_buf(0);
    }

    #[test]
    fn request_reset_clears_pending() {
        let mut decoder = RequestDecoder::new();
        let _ = decoder.mut_buf(64).unwrap();
        decoder.reset();
        let buf = decoder.mut_buf(8).unwrap();
        assert_eq!(buf.len(), 8);
        decoder.advance_buf(0);
    }

    #[test]
    fn response_available_buf_default_is_max() {
        let decoder = ResponseDecoder::new();
        let max = decoder.limits().max_buffer_size;
        assert_eq!(decoder.available_buf(), max);
    }

    #[test]
    fn request_available_buf_default_is_max() {
        let decoder = RequestDecoder::new();
        let max = decoder.limits().max_buffer_size;
        assert_eq!(decoder.available_buf(), max);
    }

    #[test]
    fn response_available_buf_after_feed() {
        let mut decoder = ResponseDecoder::new();
        let max = decoder.limits().max_buffer_size;
        decoder.feed(b"hello").unwrap();
        assert_eq!(decoder.available_buf(), max - 5);
    }

    #[test]
    fn request_available_buf_after_feed() {
        let mut decoder = RequestDecoder::new();
        let max = decoder.limits().max_buffer_size;
        decoder.feed(b"hello").unwrap();
        assert_eq!(decoder.available_buf(), max - 5);
    }

    #[test]
    fn response_available_buf_after_mut_buf() {
        let mut decoder = ResponseDecoder::new();
        let max = decoder.limits().max_buffer_size;
        decoder.feed(b"abc").unwrap();
        let _ = decoder.mut_buf(7).unwrap();
        // pending を含めて差し引かれる (3 + 7 = 10)
        assert_eq!(decoder.available_buf(), max - 10);
        decoder.advance_buf(2);
    }

    #[test]
    fn request_available_buf_after_mut_buf() {
        let mut decoder = RequestDecoder::new();
        let max = decoder.limits().max_buffer_size;
        decoder.feed(b"abc").unwrap();
        let _ = decoder.mut_buf(7).unwrap();
        assert_eq!(decoder.available_buf(), max - 10);
        decoder.advance_buf(2);
    }

    #[test]
    fn response_available_buf_after_advance_partial() {
        let mut decoder = ResponseDecoder::new();
        let max = decoder.limits().max_buffer_size;
        decoder.feed(b"abc").unwrap();
        let _ = decoder.mut_buf(7).unwrap();
        decoder.advance_buf(4);
        // 3 + 4 = 7 が確定
        assert_eq!(decoder.available_buf(), max - 7);
    }

    #[test]
    fn request_available_buf_after_advance_partial() {
        let mut decoder = RequestDecoder::new();
        let max = decoder.limits().max_buffer_size;
        decoder.feed(b"abc").unwrap();
        let _ = decoder.mut_buf(7).unwrap();
        decoder.advance_buf(4);
        assert_eq!(decoder.available_buf(), max - 7);
    }

    #[test]
    fn response_available_buf_after_reset() {
        let mut decoder = ResponseDecoder::new();
        let max = decoder.limits().max_buffer_size;
        decoder.feed(b"abc").unwrap();
        decoder.reset();
        assert_eq!(decoder.available_buf(), max);
    }

    #[test]
    fn request_available_buf_after_reset() {
        let mut decoder = RequestDecoder::new();
        let max = decoder.limits().max_buffer_size;
        decoder.feed(b"abc").unwrap();
        decoder.reset();
        assert_eq!(decoder.available_buf(), max);
    }

    /// `mut_buf` + `advance_buf` で送り込んだデータを `decode_headers` で
    /// 正常にパースできることを確認する。
    #[test]
    fn response_mut_buf_decodes_headers() {
        let mut decoder = ResponseDecoder::new();
        let data = b"HTTP/1.1 200 OK\r\nContent-Length: 5\r\n\r\nhello";
        let buf = decoder.mut_buf(data.len()).unwrap();
        buf.copy_from_slice(data);
        decoder.advance_buf(data.len());
        let response = decoder.decode().unwrap().expect("response decoded");
        assert_eq!(response.status_code(), 200);
        assert_eq!(response.body_bytes(), Some(b"hello".as_slice()));
    }

    #[test]
    fn request_mut_buf_decodes_headers() {
        let mut decoder = RequestDecoder::new();
        let data = b"POST / HTTP/1.1\r\nHost: example.com\r\nContent-Length: 5\r\n\r\nhello";
        let buf = decoder.mut_buf(data.len()).unwrap();
        buf.copy_from_slice(data);
        decoder.advance_buf(data.len());
        let request = decoder.decode().unwrap().expect("request decoded");
        assert_eq!(request.method(), "POST");
        assert_eq!(request.body_bytes(), Some(b"hello".as_slice()));
    }
}

// ========================================
// peek_body_decompressed のテスト
// ========================================

mod peek_body_decompressed {
    use super::*;
    use shiguredo_http11::compression::{
        CompressionError, CompressionStatus, Decompressor, NoCompression,
    };

    /// `NoCompression` 経由でボディ受信中: ボディデータがある間は `Some` を返す
    #[test]
    fn no_compression_returns_some_during_body() {
        let mut decoder = ResponseDecoder::with_decompressor(NoCompression::new());
        let data = b"HTTP/1.1 200 OK\r\nContent-Length: 5\r\n\r\nhello";
        let buf = decoder.mut_buf(data.len()).unwrap();
        buf.copy_from_slice(data);
        decoder.advance_buf(data.len());

        decoder.decode_headers().unwrap().expect("headers decoded");

        let mut output = vec![0u8; 32];
        let status = decoder
            .peek_body_decompressed(&mut output)
            .unwrap()
            .expect("body data available");
        assert_eq!(status.consumed(), 5);
        assert_eq!(status.produced(), 5);
        assert_eq!(&output[..5], b"hello");
        decoder.consume_body(status.consumed()).unwrap();
    }

    /// `NoCompression` 経由でボディ完了後: `None` に収束する
    /// (`Complete { 0, 0 }` が返るので新しい peek_body_decompressed の判定で None になる)
    #[test]
    fn no_compression_returns_none_after_body_complete() {
        let mut decoder = ResponseDecoder::with_decompressor(NoCompression::new());
        let data = b"HTTP/1.1 200 OK\r\nContent-Length: 5\r\n\r\nhello";
        let buf = decoder.mut_buf(data.len()).unwrap();
        buf.copy_from_slice(data);
        decoder.advance_buf(data.len());

        decoder.decode_headers().unwrap().expect("headers decoded");

        let mut output = vec![0u8; 32];
        let status = decoder
            .peek_body_decompressed(&mut output)
            .unwrap()
            .expect("body data available");
        decoder.consume_body(status.consumed()).unwrap();

        // ボディ完了後の呼び出しは None
        let next = decoder.peek_body_decompressed(&mut output).unwrap();
        assert!(next.is_none());
    }

    /// 内部 buffer 蓄積型の Decompressor 実装でも、ボディ枯渇後に drain できる
    /// (peek_body_decompressed が `decompress(&[], output)` を呼ぶ振る舞いの検証)
    #[test]
    fn drain_internal_buffer_after_body_exhausted() {
        /// テスト用 stub: feed 時に `produce_per_byte` 倍の出力を内部 buffer に蓄積する
        struct BufferingDecompressor {
            buffered: Vec<u8>,
            produce_per_byte: usize,
            finished: bool,
        }

        impl Decompressor for BufferingDecompressor {
            fn decompress(
                &mut self,
                input: &[u8],
                output: &mut [u8],
            ) -> Result<CompressionStatus, CompressionError> {
                // 内部 buffer に蓄積されたバイトを優先的に drain
                if !self.buffered.is_empty() {
                    let n = self.buffered.len().min(output.len());
                    output[..n].copy_from_slice(&self.buffered[..n]);
                    self.buffered.drain(..n);

                    if !self.buffered.is_empty() {
                        return Ok(CompressionStatus::OutputFull {
                            consumed: 0,
                            produced: n,
                        });
                    }
                    if self.finished {
                        return Ok(CompressionStatus::Complete {
                            consumed: 0,
                            produced: n,
                        });
                    }
                    return Ok(CompressionStatus::Continue {
                        consumed: 0,
                        produced: n,
                    });
                }

                // 入力を全消費して内部 buffer に蓄積 (noflate 風の振る舞い)
                if !input.is_empty() {
                    for &b in input {
                        for _ in 0..self.produce_per_byte {
                            self.buffered.push(b);
                        }
                    }
                    // ストリーム終端を 'X' バイトで表現する単純な擬似プロトコル
                    if input.contains(&b'X') {
                        self.finished = true;
                    }

                    let n = self.buffered.len().min(output.len());
                    output[..n].copy_from_slice(&self.buffered[..n]);
                    self.buffered.drain(..n);

                    if !self.buffered.is_empty() {
                        return Ok(CompressionStatus::OutputFull {
                            consumed: input.len(),
                            produced: n,
                        });
                    }
                    if self.finished {
                        return Ok(CompressionStatus::Complete {
                            consumed: input.len(),
                            produced: n,
                        });
                    }
                    return Ok(CompressionStatus::Continue {
                        consumed: input.len(),
                        produced: n,
                    });
                }

                // empty input かつ buffered 空: 進展なし
                if self.finished {
                    Ok(CompressionStatus::Complete {
                        consumed: 0,
                        produced: 0,
                    })
                } else {
                    Ok(CompressionStatus::Continue {
                        consumed: 0,
                        produced: 0,
                    })
                }
            }

            fn reset(&mut self) {
                self.buffered.clear();
                self.finished = false;
            }
        }

        let mut decoder = ResponseDecoder::with_decompressor(BufferingDecompressor {
            buffered: Vec::new(),
            produce_per_byte: 4, // 1 byte 入力 → 4 bytes 出力
            finished: false,
        });

        // body は 2 bytes ('A', 'X')。X でストリーム終端。
        // 期待される展開後出力: AAAAXXXX (8 bytes)
        let data = b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\nAX";
        let buf = decoder.mut_buf(data.len()).unwrap();
        buf.copy_from_slice(data);
        decoder.advance_buf(data.len());

        decoder.decode_headers().unwrap().expect("headers decoded");

        // 出力 buffer は 3 bytes (内部 buffer が複数回に分かれて drain される設定)
        let mut output = vec![0u8; 3];
        let mut decompressed = Vec::new();
        let mut total_consumed = 0;

        for _ in 0..16 {
            // 安全上限
            match decoder.peek_body_decompressed(&mut output).unwrap() {
                Some(status) => {
                    decompressed.extend_from_slice(&output[..status.produced()]);
                    if status.consumed() > 0 {
                        decoder.consume_body(status.consumed()).unwrap();
                        total_consumed += status.consumed();
                    }
                }
                None => break,
            }
        }

        assert_eq!(total_consumed, 2, "all body bytes consumed");
        assert_eq!(
            decompressed, b"AAAAXXXX",
            "all 8 bytes of decompressed output collected"
        );
    }

    /// 進展なしのときに None を返す (`Continue { 0, 0 }` のケース)
    #[test]
    fn returns_none_when_no_progress() {
        let mut decoder = ResponseDecoder::with_decompressor(NoCompression::new());
        let data = b"HTTP/1.1 200 OK\r\nContent-Length: 5\r\n\r\n";
        let buf = decoder.mut_buf(data.len()).unwrap();
        buf.copy_from_slice(data);
        decoder.advance_buf(data.len());

        decoder.decode_headers().unwrap().expect("headers decoded");

        // ボディがまだ届いていない → peek_body は None → decompress(&[], output) 経由で
        // NoCompression は Complete { 0, 0 } を返す → peek_body_decompressed は None を返す
        let mut output = vec![0u8; 32];
        let result = decoder.peek_body_decompressed(&mut output).unwrap();
        assert!(result.is_none(), "no progress should yield None");
    }
}
