//! ストリーミング API と CONNECT トンネルモードのテスト
//!
//! - `decode()` と streaming API (`decode_headers` / `peek_body` / `consume_body` /
//!   `progress` / `take_remaining`) の混在エラー
//! - `consume_body(0)` / トンネル中の `consume_body` / 残量超過の `consume_body` エラー
//! - CONNECT メソッドへの 2xx レスポンスでのトンネル化と非トンネル化の判定
//! - CONNECT リクエスト受信時のトンネルモード遷移と reset の挙動

use shiguredo_http11::compression::{
    CompressionError, CompressionStatus, Decompressor, NoCompression,
};
use shiguredo_http11::{BodyKind, HttpHead, RequestDecoder, ResponseDecoder};

// ========================================
// Keep-Alive 接続での Decompressor リセット検証
// ========================================

use std::cell::Cell;
use std::rc::Rc;

/// テスト用 stub: NoCompression をラップし reset() 呼び出し回数をカウントする
struct CountingDecompressor {
    inner: NoCompression,
    reset_count: Rc<Cell<usize>>,
}

impl Decompressor for CountingDecompressor {
    fn decompress(
        &mut self,
        input: &[u8],
        output: &mut [u8],
    ) -> Result<CompressionStatus, CompressionError> {
        self.inner.decompress(input, output)
    }

    fn reset(&mut self) {
        self.inner.reset();
        let count = self.reset_count.get();
        self.reset_count.set(count + 1);
    }
}

/// 2 メッセージ連続 decode で RequestDecoder 側の decompressor.reset() が呼ばれること
#[test]
fn test_decompressor_reset_request_pipelined() {
    let reset_count = Rc::new(Cell::new(0));
    let decomp = CountingDecompressor {
        inner: NoCompression::new(),
        reset_count: reset_count.clone(),
    };
    let mut decoder = RequestDecoder::with_decompressor(decomp);

    // 1 件目のリクエスト (Content-Length: 0)
    let req1 = "GET /1 HTTP/1.1\r\nHost: example.com\r\nContent-Length: 0\r\n\r\n";
    decoder.feed(req1.as_bytes()).unwrap();
    let request = decoder.decode().unwrap().unwrap();
    assert_eq!(request.method(), "GET");

    assert_eq!(
        reset_count.get(),
        1,
        "1 件目のメッセージ完了後に reset() が呼ばれる"
    );

    // 2 件目のリクエスト (Content-Length: 0)
    let req2 = "GET /2 HTTP/1.1\r\nHost: example.com\r\nContent-Length: 0\r\n\r\n";
    decoder.feed(req2.as_bytes()).unwrap();
    let request = decoder.decode().unwrap().unwrap();
    assert_eq!(request.method(), "GET");

    assert_eq!(
        reset_count.get(),
        2,
        "2 件目のメッセージ完了後に reset() が呼ばれる"
    );
}

/// 2 メッセージ連続 decode で ResponseDecoder 側の decompressor.reset() が呼ばれること
#[test]
fn test_decompressor_reset_response_pipelined() {
    let reset_count = Rc::new(Cell::new(0));
    let decomp = CountingDecompressor {
        inner: NoCompression::new(),
        reset_count: reset_count.clone(),
    };
    let mut decoder = ResponseDecoder::with_decompressor(decomp);

    // 1 件目のレスポンス (Content-Length: 0)
    let res1 = "HTTP/1.1 200 OK\r\nContent-Length: 0\r\n\r\n";
    decoder.feed(res1.as_bytes()).unwrap();
    let response = decoder.decode().unwrap().unwrap();
    assert_eq!(response.status_code(), 200);

    assert_eq!(
        reset_count.get(),
        1,
        "1 件目のメッセージ完了後に reset() が呼ばれる"
    );

    // 2 件目のレスポンス (Content-Length: 0)
    let res2 = "HTTP/1.1 201 Created\r\nContent-Length: 0\r\n\r\n";
    decoder.feed(res2.as_bytes()).unwrap();
    let response = decoder.decode().unwrap().unwrap();
    assert_eq!(response.status_code(), 201);

    assert_eq!(
        reset_count.get(),
        2,
        "2 件目のメッセージ完了後に reset() が呼ばれる"
    );
}

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
        assert_eq!(head.status_code(), status);
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
    assert_eq!(head.status_code(), 204);
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
        assert!(result.is_some(), "{} レスポンスでヘッダーを期待", method);

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
    // Transfer-Encoding: chunked を無視して Tunnel + ResponseHead から TE が消える
    let mut decoder = ResponseDecoder::new();
    decoder.set_request_method("CONNECT");
    let response = "HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n";
    decoder.feed(response.as_bytes()).unwrap();
    let (head, body_kind) = decoder.decode_headers().unwrap().unwrap();
    assert_eq!(body_kind, BodyKind::Tunnel);
    assert_eq!(head.get_header("Transfer-Encoding"), None);
    assert!(!head.is_chunked());

    // Content-Length: 1000 を無視して Tunnel + ResponseHead から CL が消える
    let mut decoder = ResponseDecoder::new();
    decoder.set_request_method("CONNECT");
    let response = "HTTP/1.1 200 OK\r\nContent-Length: 1000\r\n\r\n";
    decoder.feed(response.as_bytes()).unwrap();
    let (head, body_kind) = decoder.decode_headers().unwrap().unwrap();
    assert_eq!(body_kind, BodyKind::Tunnel);
    assert_eq!(head.get_header("Content-Length"), None);
    assert_eq!(head.content_length().unwrap(), None);

    // Transfer-Encoding + Content-Length の両方があっても Tunnel、両方とも消える
    let mut decoder = ResponseDecoder::new();
    decoder.set_request_method("CONNECT");
    let response = "HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\nContent-Length: 100\r\n\r\n";
    decoder.feed(response.as_bytes()).unwrap();
    let (head, body_kind) = decoder.decode_headers().unwrap().unwrap();
    assert_eq!(body_kind, BodyKind::Tunnel);
    assert_eq!(head.get_header("Transfer-Encoding"), None);
    assert_eq!(head.get_header("Content-Length"), None);
    assert!(!head.is_chunked());
    assert_eq!(head.content_length().unwrap(), None);

    // CONNECT 非 2xx (例: 502) では従来通り CL が ResponseHead.headers に残る
    let mut decoder = ResponseDecoder::new();
    decoder.set_request_method("CONNECT");
    let response = "HTTP/1.1 502 Bad Gateway\r\nContent-Length: 5\r\n\r\nhello";
    decoder.feed(response.as_bytes()).unwrap();
    let (head, body_kind) = decoder.decode_headers().unwrap().unwrap();
    assert_eq!(body_kind, BodyKind::ContentLength(5));
    assert_eq!(head.get_header("Content-Length"), Some("5"));
    assert_eq!(head.content_length().unwrap(), Some(5));
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
/// body として読まず、常に BodyKind::Tunnel を返してトンネルモードに遷移する。
/// ヘッダーの存在だけでは reject しない。
///
/// RFC 9110 Section 9.3.6:
///   "A CONNECT request message does not have content."
///   "the connection becomes a tunnel immediately after the header section"
#[test]
fn test_connect_request_enters_tunnel_mode() {
    // Content-Length: N > 0 が付いていても BodyKind::Tunnel
    let mut decoder = RequestDecoder::new();
    let request =
        "CONNECT example.com:443 HTTP/1.1\r\nHost: example.com:443\r\nContent-Length: 3\r\n\r\nabc";
    decoder.feed(request.as_bytes()).unwrap();
    let (head, body_kind) = decoder.decode_headers().unwrap().unwrap();
    assert_eq!(head.method(), "CONNECT");
    assert_eq!(body_kind, BodyKind::Tunnel);
    assert!(decoder.is_tunnel(), "CONNECT 受信後はトンネルモード");
    // ヘッダー終端後のバイトはトンネルデータとして取り出せる
    assert_eq!(
        decoder.take_remaining(),
        b"abc",
        "ヘッダー終端後のバイトは take_remaining で取得できる"
    );

    // Content-Length: 0 でも BodyKind::Tunnel
    let mut decoder = RequestDecoder::new();
    let request =
        "CONNECT example.com:443 HTTP/1.1\r\nHost: example.com:443\r\nContent-Length: 0\r\n\r\n";
    decoder.feed(request.as_bytes()).unwrap();
    let (head, body_kind) = decoder.decode_headers().unwrap().unwrap();
    assert_eq!(head.method(), "CONNECT");
    assert_eq!(body_kind, BodyKind::Tunnel);
    assert!(decoder.is_tunnel());
    assert!(
        decoder.take_remaining().is_empty(),
        "ヘッダー終端のみで後続データがない場合は空"
    );

    // Transfer-Encoding: chunked でも BodyKind::Tunnel
    let mut decoder = RequestDecoder::new();
    let request = "CONNECT example.com:443 HTTP/1.1\r\nHost: example.com:443\r\nTransfer-Encoding: chunked\r\n\r\n";
    decoder.feed(request.as_bytes()).unwrap();
    let (head, body_kind) = decoder.decode_headers().unwrap().unwrap();
    assert_eq!(head.method(), "CONNECT");
    assert_eq!(body_kind, BodyKind::Tunnel);
    assert!(decoder.is_tunnel());

    // ヘッダーなし (最も一般的なケース) → BodyKind::Tunnel
    let mut decoder = RequestDecoder::new();
    let request = "CONNECT example.com:443 HTTP/1.1\r\nHost: example.com:443\r\n\r\n";
    decoder.feed(request.as_bytes()).unwrap();
    let (head, body_kind) = decoder.decode_headers().unwrap().unwrap();
    assert_eq!(head.method(), "CONNECT");
    assert_eq!(body_kind, BodyKind::Tunnel);
    assert!(decoder.is_tunnel());
}

/// CONNECT トンネル化後の decode_headers / decode は明示的にエラーを返す。
/// HTTP Request Smuggling 防止のため、トンネルデータを次のリクエストとして
/// parse させないこと。
#[test]
fn test_connect_request_decode_headers_in_tunnel_returns_error() {
    let mut decoder = RequestDecoder::new();
    let request = "CONNECT example.com:443 HTTP/1.1\r\nHost: example.com:443\r\n\r\nGET /admin HTTP/1.1\r\nHost: internal\r\n\r\n";
    decoder.feed(request.as_bytes()).unwrap();
    decoder.decode_headers().unwrap().unwrap();
    assert!(decoder.is_tunnel());

    // 後続バイトに見える「GET /admin」は次のリクエストではなくトンネルデータ
    assert!(
        decoder.decode_headers().is_err(),
        "トンネルモードで decode_headers を呼ぶとエラーを返す想定"
    );

    // take_remaining で生バイトとして取り出せる
    let remaining = decoder.take_remaining();
    assert!(
        remaining.starts_with(b"GET /admin HTTP/1.1\r\n"),
        "ヘッダー終端後のバイトは next request としてではなくトンネルデータとして取得できる"
    );
}

/// reset() でトンネルモードから脱出できる (CONNECT 失敗時の復帰経路)
#[test]
fn test_connect_request_reset_clears_tunnel_mode() {
    let mut decoder = RequestDecoder::new();
    let request = "CONNECT example.com:443 HTTP/1.1\r\nHost: example.com:443\r\n\r\n";
    decoder.feed(request.as_bytes()).unwrap();
    decoder.decode_headers().unwrap().unwrap();
    assert!(decoder.is_tunnel());

    decoder.reset();
    assert!(!decoder.is_tunnel(), "reset 後はトンネルモードから脱出する");

    // 通常リクエストを decode できる
    let next = "GET / HTTP/1.1\r\nHost: example.com\r\n\r\n";
    decoder.feed(next.as_bytes()).unwrap();
    let (head, body_kind) = decoder.decode_headers().unwrap().unwrap();
    assert_eq!(head.method(), "GET");
    assert_eq!(body_kind, BodyKind::None);
}
