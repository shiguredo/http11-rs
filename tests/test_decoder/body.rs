//! ボディデコード基本のテスト
//!
//! - 1xx / 204 / 304 / HEAD のボディなし扱い (RFC 9112 Section 6.3)
//! - Transfer-Encoding の組み合わせ (gzip, chunked / deflate, chunked / etc.) の BodyKind 判定
//! - Content-Length のカンマ区切り表記の整合性
//! - HTTP/1.0 / 1.1 の Transfer-Encoding 受理可否
//! - chunked パラメータ (`chunked; q=...`) 拒否 (RFC 9112 Section 7.1)
//! - chunked トレーラー (禁止フィールド、ホワイトリスト、サイズ / 行長制限)
//! - chunked データ後の CRLF 分割到着
//! - `peek_body_decompressed` の挙動

use shiguredo_http11::{BodyKind, BodyProgress, DecoderLimits, RequestDecoder, ResponseDecoder};

// ========================================
// RFC 9112 Section 6.3 準拠テスト
// ========================================

// --- 1xx/204/304/HEAD で TE/CL を無視 ---

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

// --- Transfer-Encoding の RFC 準拠処理 ---

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

/// レスポンスで Transfer-Encoding と Content-Length 両方ある場合はエラー
///
/// RFC 9112 Section 6.3 (3): "Such a message might indicate an attempt to
/// perform request smuggling (Section 11.2) or response splitting
/// (Section 11.1) and ought to be handled as an error."
/// RFC 9112 Section 6.1: "the server MUST close the connection after
/// responding to such a request to avoid the potential attacks."
///
/// 旧挙動では silent に TE 優先で受理していたが、smuggling / response
/// splitting (CWE-444 / CWE-113) の兆候を上位層が検知できなくなるため、
/// リクエスト経路と対称にエラー化する。
#[test]
fn test_response_te_and_cl_is_rejected() {
    let mut decoder = ResponseDecoder::new();
    let response = "HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\nContent-Length: 100\r\n\r\n";
    decoder.feed(response.as_bytes()).unwrap();

    let result = decoder.decode_headers();
    assert!(
        result.is_err(),
        "TE + CL の組合せは smuggling 兆候として reject される想定 (RFC 9112 Section 6.3)"
    );
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

// --- Content-Length カンマ区切り対応 ---

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
// HTTP/1.0 + Transfer-Encoding 拒否テスト (RFC 9112 Section 6.1)
// リクエスト側は decode_body.rs の `http10_with_transfer_encoding_should_fail` で網羅する。
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
    // RFC 9110 Section 6.5.1 ホワイトリスト方式: Trailer ヘッダーで事前申告する。
    // 3 つのトレーラーで制限 2 を超える。
    let response = "HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\nTrailer: X-A, X-B, X-C\r\n\r\n\
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
    // トレーラー行 "X-Trailer: " + 30 文字 = 41 文字 > 30。
    // RFC 9110 Section 6.5.1 ホワイトリスト方式: Trailer ヘッダーで事前申告する。
    let long_value = "a".repeat(30);
    let response = format!(
        "HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\nTrailer: X-Trailer\r\n\r\n0\r\nX-Trailer: {}\r\n\r\n",
        long_value
    );
    decoder.feed(response.as_bytes()).unwrap();
    decoder.decode_headers().unwrap().unwrap();

    let result = decoder.progress();
    assert!(result.is_err());
}

/// RFC 9110 Section 6.5.1 ホワイトリスト方式: `Trailer:` ヘッダーで申告されたフィールドのみ受理される
#[test]
fn test_chunked_trailer_whitelist_accepts_declared_field() {
    let mut decoder = ResponseDecoder::new();
    let response = "HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\nTrailer: X-Checksum\r\n\r\n\
                    0\r\nX-Checksum: abc123\r\n\r\n";
    decoder.feed(response.as_bytes()).unwrap();
    decoder.decode_headers().unwrap().unwrap();
    // 進める。Complete に到達するまでループする
    loop {
        match decoder.progress().unwrap() {
            shiguredo_http11::BodyProgress::Complete { trailers } => {
                assert_eq!(trailers.len(), 1);
                assert!(trailers[0].0.eq_ignore_ascii_case("X-Checksum"));
                assert_eq!(trailers[0].1, "abc123");
                break;
            }
            _ => continue,
        }
    }
}

/// RFC 9110 Section 6.5.1 ホワイトリスト方式: 申告されていない trailer は拒否される
#[test]
fn test_chunked_trailer_whitelist_rejects_undeclared_field() {
    let mut decoder = ResponseDecoder::new();
    // X-Checksum を申告したが、実際の trailer-section には X-Other が来る
    let response = "HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\nTrailer: X-Checksum\r\n\r\n\
                    0\r\nX-Other: leaked\r\n\r\n";
    decoder.feed(response.as_bytes()).unwrap();
    decoder.decode_headers().unwrap().unwrap();
    assert!(
        decoder.progress().is_err(),
        "申告されていない trailer フィールドは reject されるべき"
    );
}

/// RFC 9110 Section 6.5.1 ホワイトリスト方式: 認証ヘッダー後付け注入による smuggling を遮断
#[test]
fn test_chunked_trailer_whitelist_rejects_authorization_injection() {
    let mut decoder = ResponseDecoder::new();
    // 攻撃シナリオ: X-Custom を申告して通常 trailer に見せかけつつ、
    // 実際の trailer-section に Authorization を仕込む。
    // 認証カテゴリは `is_prohibited_trailer_field` で reject される。
    let response = "HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\nTrailer: X-Custom\r\n\r\n\
                    0\r\nAuthorization: Bearer attacker-token\r\n\r\n";
    decoder.feed(response.as_bytes()).unwrap();
    decoder.decode_headers().unwrap().unwrap();
    assert!(
        decoder.progress().is_err(),
        "Authorization は trailer に置けないため reject されるべき"
    );
}

/// RFC 9110 Section 6.5.1 ホワイトリスト方式: `Trailer:` ヘッダーがない場合、trailer-section は何も受理しない
#[test]
fn test_chunked_trailer_whitelist_rejects_unannounced_trailers() {
    let mut decoder = ResponseDecoder::new();
    // Trailer ヘッダー無し → 申告なし → 任意の trailer フィールドが reject される
    let response = "HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n\
                    0\r\nX-Custom: value\r\n\r\n";
    decoder.feed(response.as_bytes()).unwrap();
    decoder.decode_headers().unwrap().unwrap();
    assert!(
        decoder.progress().is_err(),
        "Trailer ヘッダー無しの場合、trailer-section は何も受理してはならない"
    );
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
// peek_body_decompressed のテスト
// ========================================

mod peek_body_decompressed {
    use shiguredo_http11::ResponseDecoder;
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

        decoder
            .decode_headers()
            .unwrap()
            .expect("ヘッダーがデコードされるべき");

        let mut output = vec![0u8; 32];
        let status = decoder
            .peek_body_decompressed(&mut output)
            .unwrap()
            .expect("ボディデータが取得できるべき");
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

        decoder
            .decode_headers()
            .unwrap()
            .expect("ヘッダーがデコードされるべき");

        let mut output = vec![0u8; 32];
        let status = decoder
            .peek_body_decompressed(&mut output)
            .unwrap()
            .expect("ボディデータが取得できるべき");
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

        decoder
            .decode_headers()
            .unwrap()
            .expect("ヘッダーがデコードされるべき");

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

        assert_eq!(total_consumed, 2, "ボディ全バイトが消費されるべき");
        assert_eq!(
            decompressed, b"AAAAXXXX",
            "解凍後の 8 バイトすべてが収集されるべき"
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

        decoder
            .decode_headers()
            .unwrap()
            .expect("ヘッダーがデコードされるべき");

        // ボディがまだ届いていない → peek_body は None → decompress(&[], output) 経由で
        // NoCompression は Complete { 0, 0 } を返す → peek_body_decompressed は None を返す
        let mut output = vec![0u8; 32];
        let result = decoder.peek_body_decompressed(&mut output).unwrap();
        assert!(result.is_none(), "進展なしのときは None になるべき");
    }
}
