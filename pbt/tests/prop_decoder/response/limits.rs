//! ResponseDecoder のリミット関連プロパティテスト
//!
//! `DecoderLimits` の各上限 (buffer / header line / headers count / body size) を
//! 超過した場合のエラーパスと、`limits()` ゲッターの動作を対象にする。

use proptest::prelude::*;
use shiguredo_http11::{BodyKind, DecoderLimits, Error, ResponseDecoder};

// ========================================
// デコーダーリミット PBT (レスポンス)
// ========================================

proptest! {
    #[test]
    fn prop_response_decoder_buffer_overflow(
        data_size in 1000..2000usize
    ) {
        let limits = DecoderLimits {
            max_buffer_size: 100,
            ..DecoderLimits::default()
        };
        let mut decoder = ResponseDecoder::with_limits(limits);
        let data = "x".repeat(data_size);
        let result = decoder.feed(data.as_bytes());
        let is_buffer_overflow = matches!(result, Err(Error::BufferOverflow { .. }));
        prop_assert!(is_buffer_overflow, "BufferOverflow を期待したが {:?} だった", result);
    }
}

proptest! {
    #[test]
    fn prop_response_decoder_header_line_too_long(
        header_value_len in 200..500usize
    ) {
        let limits = DecoderLimits {
            max_header_line_size: 100,
            ..DecoderLimits::default()
        };
        let mut decoder = ResponseDecoder::with_limits(limits);
        let header_value = "x".repeat(header_value_len);
        let data = format!("HTTP/1.1 200 OK\r\nX-Long: {}\r\n\r\n", header_value);
        decoder.feed(data.as_bytes()).unwrap();
        let result = decoder.decode_headers();
        let is_header_line_too_long = matches!(result, Err(Error::HeaderLineTooLong { .. }));
        prop_assert!(is_header_line_too_long, "HeaderLineTooLong を期待したが {:?} だった", result);
    }
}

proptest! {
    #[test]
    fn prop_response_decoder_too_many_headers(
        header_count in 20..50usize
    ) {
        let limits = DecoderLimits {
            max_headers_count: 10,
            ..DecoderLimits::default()
        };
        let mut decoder = ResponseDecoder::with_limits(limits);
        let headers = (0..header_count)
            .map(|i| format!("X-Header{}: value{}", i, i))
            .collect::<Vec<_>>()
            .join("\r\n");
        let data = format!("HTTP/1.1 200 OK\r\n{}\r\n\r\n", headers);
        decoder.feed(data.as_bytes()).unwrap();
        let result = decoder.decode_headers();
        let is_too_many_headers = matches!(result, Err(Error::TooManyHeaders { .. }));
        prop_assert!(is_too_many_headers, "TooManyHeaders を期待したが {:?} だった", result);
    }
}

proptest! {
    #[test]
    fn prop_response_decoder_body_too_large_content_length(
        body_size in 200..500usize
    ) {
        let limits = DecoderLimits {
            max_body_size: 100,
            ..DecoderLimits::default()
        };
        let mut decoder = ResponseDecoder::with_limits(limits);
        let body = "x".repeat(body_size);
        let data = format!("HTTP/1.1 200 OK\r\nContent-Length: {}\r\n\r\n{}", body_size, body);
        decoder.feed(data.as_bytes()).unwrap();
        let result = decoder.decode_headers();
        let is_body_too_large = matches!(result, Err(Error::BodyTooLarge { .. }));
        prop_assert!(is_body_too_large, "BodyTooLarge を期待したが {:?} だった", result);
    }
}

proptest! {
    #[test]
    fn prop_response_decoder_body_too_large_chunked(
        chunk_size in 200..500usize
    ) {
        let limits = DecoderLimits {
            max_body_size: 100,
            ..DecoderLimits::default()
        };
        let mut decoder = ResponseDecoder::with_limits(limits);
        let chunk = "x".repeat(chunk_size);
        let data = format!(
            "HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n{:x}\r\n{}\r\n0\r\n\r\n",
            chunk_size, chunk
        );
        decoder.feed(data.as_bytes()).unwrap();
        let (_, _) = decoder.decode_headers().unwrap().unwrap();
        // チャンクサイズ解析時にボディサイズ制限エラー
        let result = decoder.progress();
        prop_assert!(result.is_err());
    }
}

proptest! {
    #[test]
    fn prop_response_decoder_body_too_large_close_delimited(
        body_size in 200..500usize
    ) {
        // close-delimited ボディでも max_body_size を超えるとエラー
        let limits = DecoderLimits {
            max_body_size: 100,
            ..DecoderLimits::default()
        };
        let mut decoder = ResponseDecoder::with_limits(limits);
        // Content-Length も Transfer-Encoding もなし = close-delimited
        decoder.feed(b"HTTP/1.1 200 OK\r\n\r\n").unwrap();
        let (_, body_kind) = decoder.decode_headers().unwrap().unwrap();
        prop_assert_eq!(body_kind, BodyKind::CloseDelimited);

        // ボディデータを追加
        let body = vec![b'x'; body_size];
        decoder.feed(&body).unwrap();

        // ボディを消費していくと max_body_size 超過でエラー
        let mut consumed = 0;
        while let Some(data) = decoder.peek_body() {
            let len = data.len();
            match decoder.consume_body(len) {
                Ok(_) => consumed += len,
                Err(shiguredo_http11::Error::BodyTooLarge { .. }) => {
                    // max_body_size を超えた時点でエラー
                    prop_assert!(consumed <= 100);
                    return Ok(());
                }
                Err(e) => {
                    return Err(proptest::test_runner::TestCaseError::fail(format!(
                        "unexpected error: {:?}",
                        e
                    )));
                }
            }
        }
        // ここに到達した場合は問題
        prop_assert!(false, "BodyTooLarge エラーを期待したが {} バイト消費した", consumed);
    }
}

proptest! {
    #[test]
    fn prop_response_decoder_feed_unchecked_no_limit(
        data_size in 1000..2000usize
    ) {
        let limits = DecoderLimits {
            max_buffer_size: 100,
            ..DecoderLimits::default()
        };
        let mut decoder = ResponseDecoder::with_limits(limits);
        let data = "x".repeat(data_size);
        decoder.feed_unchecked(data.as_bytes());
        prop_assert_eq!(decoder.remaining().len(), data_size);
    }
}

proptest! {
    #[test]
    fn prop_response_decoder_limits_getter(
        max_buffer_size in 100..1000usize,
        max_body_size in 100..1000usize
    ) {
        let limits = DecoderLimits {
            max_buffer_size,
            max_body_size,
            ..DecoderLimits::default()
        };
        let decoder = ResponseDecoder::with_limits(limits.clone());
        prop_assert_eq!(decoder.limits().max_buffer_size, max_buffer_size);
        prop_assert_eq!(decoder.limits().max_body_size, max_body_size);
    }
}

// ========================================
// close-delimited ボディサイズ制限
// ========================================

proptest! {
    /// close-delimited のボディサイズ制限チェック
    #[test]
    fn prop_response_decode_close_delimited_body_too_large(
        body_size in 128usize..512
    ) {
        let limits = DecoderLimits {
            max_body_size: 64,
            ..DecoderLimits::default()
        };
        let mut decoder = ResponseDecoder::with_limits(limits);
        let body_data = vec![0x41u8; body_size];
        decoder.feed(b"HTTP/1.1 200 OK\r\n\r\n").unwrap();
        decoder.feed(&body_data).unwrap();

        let result = decoder.decode();
        let is_body_too_large = matches!(result, Err(Error::BodyTooLarge { .. }));
        prop_assert!(is_body_too_large, "BodyTooLarge を期待したが {:?} だった", result);
    }
}
