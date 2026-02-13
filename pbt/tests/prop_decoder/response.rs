//! ResponseDecoder のプロパティテスト

use proptest::prelude::*;
use shiguredo_http11::{BodyKind, BodyProgress, DecoderLimits, Error, Response, ResponseDecoder};

use super::{body, reason_phrase, status_code};

// ========================================
// ステータス行のエラー PBT
// ========================================

proptest! {
    #[test]
    fn prop_status_line_missing_parts_error(
        version in prop_oneof![Just("HTTP/1.0"), Just("HTTP/1.1"), Just("RTSP/1.0"), Just("RTSP/2.0")]
    ) {
        // ステータスコードがないステータス行はエラー
        let data = format!("{}\r\n\r\n", version);
        let mut decoder = ResponseDecoder::new();
        decoder.feed(data.as_bytes()).unwrap();
        prop_assert!(decoder.decode_headers().is_err());
    }
}

proptest! {
    #[test]
    fn prop_status_code_invalid_error(
        invalid_code in "[a-zA-Z]{1,5}"
    ) {
        // 数字でないステータスコードはエラー
        let data = format!("HTTP/1.1 {} OK\r\n\r\n", invalid_code);
        let mut decoder = ResponseDecoder::new();
        decoder.feed(data.as_bytes()).unwrap();
        prop_assert!(decoder.decode_headers().is_err());
    }
}

proptest! {
    #[test]
    fn prop_status_line_no_reason_phrase_ok(
        status_code in 200..600u16
    ) {
        // reason phrase なしは OK
        let data = format!("HTTP/1.1 {}\r\n\r\n", status_code);
        let mut decoder = ResponseDecoder::new();
        decoder.feed(data.as_bytes()).unwrap();
        let (head, _) = decoder.decode_headers().unwrap().unwrap();
        prop_assert_eq!(head.status_code, status_code);
        prop_assert_eq!(head.reason_phrase, "");
    }
}

// ========================================
// HEAD リクエストへのレスポンス PBT
// ========================================

proptest! {
    #[test]
    fn prop_head_response_with_content_length(
        content_length in 1..10000usize
    ) {
        // HEAD レスポンスは Content-Length があってもボディなし
        let data = format!("HTTP/1.1 200 OK\r\nContent-Length: {}\r\n\r\n", content_length);
        let mut decoder = ResponseDecoder::new();
        decoder.set_expect_no_body(true);
        decoder.feed(data.as_bytes()).unwrap();
        let (_, body_kind) = decoder.decode_headers().unwrap().unwrap();
        prop_assert_eq!(body_kind, BodyKind::None);
    }
}

proptest! {
    #[test]
    fn prop_head_response_with_transfer_encoding(
        status_code in 200..400u16
    ) {
        // HEAD レスポンスは Transfer-Encoding があってもボディなし
        let data = format!("HTTP/1.1 {} OK\r\nTransfer-Encoding: chunked\r\n\r\n", status_code);
        let mut decoder = ResponseDecoder::new();
        decoder.set_expect_no_body(true);
        decoder.feed(data.as_bytes()).unwrap();
        let (_, body_kind) = decoder.decode_headers().unwrap().unwrap();
        prop_assert_eq!(body_kind, BodyKind::None);
    }
}

// ========================================
// ボディなしステータスコード PBT
// ========================================

proptest! {
    #[test]
    fn prop_status_1xx_no_body(
        code in 100u16..200,
        content_length in 1..1000usize
    ) {
        // 1xx レスポンスは Content-Length があってもボディなし
        let data = format!("HTTP/1.1 {} Continue\r\nContent-Length: {}\r\n\r\n", code, content_length);
        let mut decoder = ResponseDecoder::new();
        decoder.feed(data.as_bytes()).unwrap();
        let (_, body_kind) = decoder.decode_headers().unwrap().unwrap();
        prop_assert_eq!(body_kind, BodyKind::None);
    }
}

proptest! {
    #[test]
    fn prop_status_204_no_body(
        content_length in 1..1000usize
    ) {
        // 204 No Content はボディなし
        let data = format!("HTTP/1.1 204 No Content\r\nContent-Length: {}\r\n\r\n", content_length);
        let mut decoder = ResponseDecoder::new();
        decoder.feed(data.as_bytes()).unwrap();
        let (_, body_kind) = decoder.decode_headers().unwrap().unwrap();
        prop_assert_eq!(body_kind, BodyKind::None);
    }
}

proptest! {
    #[test]
    fn prop_status_304_no_body(
        content_length in 1..1000usize
    ) {
        // 304 Not Modified はボディなし
        let data = format!("HTTP/1.1 304 Not Modified\r\nContent-Length: {}\r\n\r\n", content_length);
        let mut decoder = ResponseDecoder::new();
        decoder.feed(data.as_bytes()).unwrap();
        let (_, body_kind) = decoder.decode_headers().unwrap().unwrap();
        prop_assert_eq!(body_kind, BodyKind::None);
    }
}

proptest! {
    #[test]
    fn prop_status_code_boundary_199(
        code in 100u16..200
    ) {
        // 199 以下は 1xx
        let data = format!("HTTP/1.1 {} Info\r\n\r\n", code);
        let mut decoder = ResponseDecoder::new();
        decoder.feed(data.as_bytes()).unwrap();
        let (head, body_kind) = decoder.decode_headers().unwrap().unwrap();
        prop_assert!(head.is_informational());
        prop_assert_eq!(body_kind, BodyKind::None);
    }
}

proptest! {
    #[test]
    fn prop_status_code_boundary_200(
        code in 200u16..300
    ) {
        // 200-299 は成功
        let data = format!("HTTP/1.1 {} OK\r\nContent-Length: 0\r\n\r\n", code);
        let mut decoder = ResponseDecoder::new();
        decoder.feed(data.as_bytes()).unwrap();
        let (head, _) = decoder.decode_headers().unwrap().unwrap();
        // 204 は特別扱い
        if code != 204 {
            prop_assert!(head.is_success());
        }
    }
}

proptest! {
    #[test]
    fn prop_status_code_boundary_203(
        code in 200u16..204
    ) {
        // 200-203 はボディあり可能
        let data = format!("HTTP/1.1 {} OK\r\nContent-Length: 5\r\n\r\nhello", code);
        let mut decoder = ResponseDecoder::new();
        decoder.feed(data.as_bytes()).unwrap();
        let (_, body_kind) = decoder.decode_headers().unwrap().unwrap();
        prop_assert_eq!(body_kind, BodyKind::ContentLength(5));
    }
}

// ========================================
// UTF-8 エラー PBT (レスポンス)
// ========================================

proptest! {
    #[test]
    fn prop_invalid_utf8_chunk_size_error(
        invalid_byte in 128u8..=255
    ) {
        // 無効な UTF-8 バイトを含むチャンクサイズはエラー
        let mut data = b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n".to_vec();
        data.push(invalid_byte);
        data.extend(b"\r\n");
        let mut decoder = ResponseDecoder::new();
        decoder.feed(&data).unwrap();
        let (_, _) = decoder.decode_headers().unwrap().unwrap();
        prop_assert!(decoder.progress().is_err());
    }
}

proptest! {
    #[test]
    fn prop_invalid_utf8_status_line_error(
        invalid_byte in 128u8..=255
    ) {
        // 無効な UTF-8 バイトを含むステータス行はエラー
        let mut data = b"HTTP/1.1 200 ".to_vec();
        data.push(invalid_byte);
        data.extend(b"OK\r\n\r\n");
        let mut decoder = ResponseDecoder::new();
        decoder.feed(&data).unwrap();
        prop_assert!(decoder.decode_headers().is_err());
    }
}

proptest! {
    #[test]
    fn prop_invalid_utf8_response_header_error(
        header_name in "[A-Za-z]{1,16}",
        invalid_byte in 128u8..=255
    ) {
        // 無効な UTF-8 バイトを含むレスポンスヘッダーはエラー
        let mut data = b"HTTP/1.1 200 OK\r\n".to_vec();
        data.extend(header_name.as_bytes());
        data.extend(b": ");
        data.push(invalid_byte);
        data.extend(b"\r\n\r\n");
        let mut decoder = ResponseDecoder::new();
        decoder.feed(&data).unwrap();
        prop_assert!(decoder.decode_headers().is_err());
    }
}

// ========================================
// 部分的なデータ (None を返す) PBT (レスポンス)
// ========================================

proptest! {
    #[test]
    fn prop_incomplete_chunk_size(
        size in 1..100usize
    ) {
        // 不完全なチャンクサイズ行は None
        let data = format!("HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n{:x}", size);
        let mut decoder = ResponseDecoder::new();
        decoder.feed(data.as_bytes()).unwrap();
        let (_, _) = decoder.decode_headers().unwrap().unwrap();
        // peek_body は None (チャンクサイズ行が不完全)
        prop_assert!(decoder.peek_body().is_none());
    }
}

proptest! {
    #[test]
    fn prop_incomplete_chunk_data(
        chunk_size in 10..100usize,
        partial_size in 1..10usize
    ) {
        // 不完全なチャンクデータは部分データを返す
        let partial_data = "x".repeat(partial_size);
        let data = format!(
            "HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n{:x}\r\n{}",
            chunk_size, partial_data
        );
        let mut decoder = ResponseDecoder::new();
        decoder.feed(data.as_bytes()).unwrap();
        let (_, _) = decoder.decode_headers().unwrap().unwrap();
        decoder.progress().unwrap(); // チャンクサイズを処理
        let peeked = decoder.peek_body().unwrap();
        prop_assert_eq!(peeked, partial_data.as_bytes());
    }
}

proptest! {
    #[test]
    fn prop_incomplete_trailer(
        body_content in "[a-z]{1,32}",
        trailer_name in "[A-Za-z]{1,16}"
    ) {
        // 不完全なトレーラーは Continue
        let len = body_content.len();
        let data = format!(
            "HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n{:x}\r\n{}\r\n0\r\n{}: value",
            len, body_content, trailer_name
        );
        let mut decoder = ResponseDecoder::new();
        decoder.feed(data.as_bytes()).unwrap();
        let (_, _) = decoder.decode_headers().unwrap().unwrap();

        // ボディを消費
        loop {
            if let Some(data) = decoder.peek_body() {
                let len = data.len();
                let result = decoder.consume_body(len).unwrap();
                if matches!(result, BodyProgress::Complete { .. }) {
                    break;
                }
            } else {
                let result = decoder.progress().unwrap();
                // トレーラーが不完全なので Continue
                prop_assert!(matches!(result, BodyProgress::Continue));
                break;
            }
        }
    }
}

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
        prop_assert!(is_buffer_overflow, "expected BufferOverflow, got {:?}", result);
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
        prop_assert!(is_header_line_too_long, "expected HeaderLineTooLong, got {:?}", result);
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
        prop_assert!(is_too_many_headers, "expected TooManyHeaders, got {:?}", result);
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
        prop_assert!(is_body_too_large, "expected BodyTooLarge, got {:?}", result);
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
        prop_assert!(false, "expected BodyTooLarge error but consumed {} bytes", consumed);
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

proptest! {
    #[test]
    fn prop_response_decoder_remaining(
        data_len in 10..100usize
    ) {
        let mut decoder = ResponseDecoder::new();
        let data = "x".repeat(data_len);
        decoder.feed(data.as_bytes()).unwrap();
        prop_assert_eq!(decoder.remaining().len(), data_len);
    }
}

proptest! {
    #[test]
    fn prop_response_decoder_reset(
        // 204, 304 はボディなしなので除外 (2xx のうちボディがあるステータスコードのみ)
        status_code in prop_oneof![200u16..=203, 205u16..=299]
    ) {
        let mut decoder = ResponseDecoder::new();
        let data = format!("HTTP/1.1 {} OK\r\nContent-Length: 5\r\n\r\nhello", status_code);
        decoder.feed(data.as_bytes()).unwrap();
        let _ = decoder.decode_headers().unwrap().unwrap();
        decoder.reset();
        prop_assert_eq!(decoder.remaining().len(), 0);
    }
}

proptest! {
    #[test]
    fn prop_response_decoder_reset_expect_no_body(
        // 204, 304 はボディなしなので除外 (2xx のうちボディがあるステータスコードのみ)
        status_code in prop_oneof![200u16..=203, 205u16..=299]
    ) {
        let mut decoder = ResponseDecoder::new();
        decoder.set_expect_no_body(true);
        let data = format!("HTTP/1.1 {} OK\r\nContent-Length: 100\r\n\r\n", status_code);
        decoder.feed(data.as_bytes()).unwrap();
        let (_, body_kind) = decoder.decode_headers().unwrap().unwrap();
        prop_assert_eq!(body_kind, BodyKind::None);
        decoder.reset();
        // reset 後は expect_no_body がクリアされる
        let data2 = format!("HTTP/1.1 {} OK\r\nContent-Length: 5\r\n\r\nhello", status_code);
        decoder.feed(data2.as_bytes()).unwrap();
        let (_, body_kind2) = decoder.decode_headers().unwrap().unwrap();
        prop_assert_eq!(body_kind2, BodyKind::ContentLength(5));
    }
}

proptest! {
    #[test]
    fn prop_response_no_content_length_no_transfer_encoding(
        status_code in 200..204u16
    ) {
        // RFC 9112: Content-Length も Transfer-Encoding もない場合は close-delimited
        // (接続が閉じられるまでがボディ)
        let data = format!("HTTP/1.1 {} OK\r\n\r\n", status_code);
        let mut decoder = ResponseDecoder::new();
        decoder.feed(data.as_bytes()).unwrap();
        let (_, body_kind) = decoder.decode_headers().unwrap().unwrap();
        prop_assert_eq!(body_kind, BodyKind::CloseDelimited);
    }
}

proptest! {
    #[test]
    fn prop_response_content_length_zero(
        status_code in 200..204u16
    ) {
        // Content-Length: 0 はボディなし
        let data = format!("HTTP/1.1 {} OK\r\nContent-Length: 0\r\n\r\n", status_code);
        let mut decoder = ResponseDecoder::new();
        decoder.feed(data.as_bytes()).unwrap();
        let (_, body_kind) = decoder.decode_headers().unwrap().unwrap();
        prop_assert_eq!(body_kind, BodyKind::ContentLength(0));
    }
}

// ========================================
// 複数レスポンス PBT
// ========================================

proptest! {
    #[test]
    fn prop_multiple_responses_same_decoder(
        status_codes in proptest::collection::vec(status_code(), 2..5)
    ) {
        let mut decoder = ResponseDecoder::new();

        for code in &status_codes {
            let response = Response::new(*code, "OK");
            let encoded = response.encode();
            decoder.feed(&encoded).unwrap();
            let decoded = decoder.decode().unwrap().unwrap();
            prop_assert_eq!(decoded.status_code, *code);
            decoder.reset();
        }
    }
}

// ========================================
// ストリーミング API の PBT (レスポンス)
// ========================================

proptest! {
    #[test]
    fn prop_streaming_decode_response(
        status_code in status_code(),
        body_content in "[a-z]{1,100}"
    ) {
        let mut decoder = ResponseDecoder::new();
        let body_len = body_content.len();
        let data = format!(
            "HTTP/1.1 {} OK\r\nContent-Length: {}\r\n\r\n{}",
            status_code, body_len, body_content
        );
        decoder.feed(data.as_bytes()).unwrap();
        let (head, body_kind) = decoder.decode_headers().unwrap().unwrap();
        prop_assert_eq!(head.status_code, status_code);
        // RFC 9112 Section 6.3: 1xx, 204, 304 はボディなし
        if (100..200).contains(&status_code) || status_code == 204 || status_code == 304 {
            prop_assert_eq!(body_kind, BodyKind::None);
        } else {
            prop_assert_eq!(body_kind, BodyKind::ContentLength(body_len));
        }
    }
}

// ========================================
// Response ラウンドトリップ PBT
// ========================================

proptest! {
    #[test]
    fn prop_response_roundtrip(
        status in status_code(),
        reason in reason_phrase(),
        body_data in body()
    ) {
        let mut response = Response::new(status, &reason);

        // RFC 9110: 1xx/204/205/304 はエンコーダー側でボディ生成を禁止
        // (デコーダー側では 205 はメッセージ長決定規則に従うが、ラウンドトリップテストでは
        //  エンコーダーの制約に合わせる)
        let status_forbids_body = (100..200).contains(&status)
            || status == 204
            || status == 205
            || status == 304;

        if !body_data.is_empty() && !status_forbids_body {
            response = response.body(body_data.clone());
        }

        let encoded = response.encode();
        let mut decoder = ResponseDecoder::new();
        decoder.feed(&encoded).unwrap();
        let decoded = decoder.decode().unwrap().unwrap();

        prop_assert_eq!(decoded.status_code, status);
        if !status_forbids_body {
            prop_assert_eq!(decoded.body, body_data);
        }
    }
}

// ========================================
// decode_headers を2回呼んだ場合の挙動 PBT (レスポンス)
// ========================================

proptest! {
    #[test]
    fn prop_response_decode_headers_twice_returns_none(
        status_code in 200..600u16
    ) {
        // ボディなしレスポンスの場合、2 回目の decode_headers は Ok(None)
        let data = format!("HTTP/1.1 {} OK\r\nContent-Length: 0\r\n\r\n", status_code);
        let mut decoder = ResponseDecoder::new();
        decoder.feed(data.as_bytes()).unwrap();
        let _ = decoder.decode_headers().unwrap().unwrap();
        // 2回目は次のメッセージがないので Ok(None)
        prop_assert!(decoder.decode_headers().unwrap().is_none());
    }
}

// ========================================
// consume_body を decode_headers 前に呼ぶとエラー PBT (レスポンス)
// ========================================

proptest! {
    #[test]
    fn prop_response_consume_body_before_decode_headers_error(
        status_code in 200..600u16
    ) {
        let mut decoder = ResponseDecoder::new();
        let data = format!("HTTP/1.1 {} OK\r\n\r\n", status_code);
        decoder.feed(data.as_bytes()).unwrap();
        prop_assert!(decoder.progress().is_err());
    }
}

// ========================================
// decode() API の連続デコードテスト (Keep-Alive) PBT (レスポンス)
// ========================================

proptest! {
    #[test]
    fn prop_decode_multiple_responses_keep_alive(
        status_codes in proptest::collection::vec(status_code(), 2..5)
    ) {
        let mut decoder = ResponseDecoder::new();

        // 全レスポンスを一度にバッファに入れる
        let mut all_data = Vec::new();
        for code in &status_codes {
            let response = Response::new(*code, "OK");
            all_data.extend(response.encode());
        }
        decoder.feed(&all_data).unwrap();

        // decode() を連続して呼ぶ（reset() なし）
        for code in &status_codes {
            let response = decoder.decode().unwrap().unwrap();
            prop_assert_eq!(response.status_code, *code);
        }
    }
}

// ========================================
// decode_headers の Complete → StartLine 遷移 PBT (レスポンス)
// ========================================

proptest! {
    #[test]
    fn prop_response_decode_headers_multiple_no_body_messages(
        count in 2..5usize,
        base_status in 200..400u16
    ) {
        // 複数のボディなしレスポンスを decode_headers で連続処理
        let mut decoder = ResponseDecoder::new();
        for i in 0..count {
            let status = base_status + i as u16;
            let data = format!("HTTP/1.1 {} OK\r\nContent-Length: 0\r\n\r\n", status);
            decoder.feed(data.as_bytes()).unwrap();
        }

        for i in 0..count {
            let (head, _) = decoder.decode_headers().unwrap().unwrap();
            prop_assert_eq!(head.status_code, base_status + i as u16);
        }

        // 次のメッセージがなければ Ok(None)
        prop_assert!(decoder.decode_headers().unwrap().is_none());
    }
}

// ========================================
// CONNECT トンネルモードの PBT
// ========================================

proptest! {
    /// CONNECT + 2xx の全ステータスコードでトンネルモードになることを確認
    #[test]
    fn prop_connect_all_2xx_tunnel(status in 200u16..300u16) {
        let mut decoder = ResponseDecoder::new();
        decoder.set_request_method("CONNECT");

        let response = format!("HTTP/1.1 {} OK\r\n\r\n", status);
        decoder.feed(response.as_bytes()).unwrap();

        let result = decoder.decode_headers().unwrap().unwrap();
        prop_assert_eq!(result.1, BodyKind::Tunnel, "expected Tunnel for CONNECT {}", status);
        prop_assert!(decoder.is_tunnel());
    }
}

// ========================================
// RFC 9112 Section 6.3 準拠テスト
// ========================================

proptest! {
    /// 1xx レスポンスで不正な Transfer-Encoding があってもエラーにならない
    #[test]
    fn prop_1xx_ignores_invalid_te(status in 100u16..200u16) {
        let mut decoder = ResponseDecoder::new();
        // gzip のみは通常エラーだが、1xx では無視される
        let response = format!(
            "HTTP/1.1 {} Continue\r\nTransfer-Encoding: gzip\r\n\r\n",
            status
        );
        decoder.feed(response.as_bytes()).unwrap();

        let result = decoder.decode_headers().unwrap().unwrap();
        prop_assert_eq!(result.1, BodyKind::None, "1xx should have no body");
    }
}

proptest! {
    /// 同じ値のカンマ区切り Content-Length は受理される
    #[test]
    fn prop_cl_comma_same_values(len in 0usize..10000) {
        let mut decoder = ResponseDecoder::new();
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Length: {}, {}, {}\r\n\r\n",
            len, len, len
        );
        decoder.feed(response.as_bytes()).unwrap();

        let result = decoder.decode_headers().unwrap().unwrap();
        prop_assert_eq!(result.1, BodyKind::ContentLength(len));
    }
}

proptest! {
    /// 異なる値のカンマ区切り Content-Length はエラー
    #[test]
    fn prop_cl_comma_different_values_error(
        len1 in 0usize..10000,
        len2 in 0usize..10000
    ) {
        prop_assume!(len1 != len2);

        let mut decoder = ResponseDecoder::new();
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Length: {}, {}\r\n\r\n",
            len1, len2
        );
        decoder.feed(response.as_bytes()).unwrap();

        prop_assert!(decoder.decode_headers().is_err());
    }
}

// ========================================
// close-delimited ボディの PBT
// ========================================

proptest! {
    /// close-delimited ボディの decode() + mark_eof() ラウンドトリップ
    #[test]
    fn prop_response_decode_close_delimited_with_mark_eof(
        body_data in proptest::collection::vec(any::<u8>(), 1..256)
    ) {
        let mut decoder = ResponseDecoder::new();
        decoder.feed(b"HTTP/1.1 200 OK\r\n\r\n").unwrap();
        decoder.feed(&body_data).unwrap();

        // mark_eof() 前は None
        let result = decoder.decode().unwrap();
        prop_assert!(result.is_none());

        // mark_eof() 後に decode() で取得可能
        decoder.mark_eof();
        let response = decoder.decode().unwrap().unwrap();
        prop_assert_eq!(response.status_code, 200);
        prop_assert_eq!(&response.body, &body_data);
    }
}

proptest! {
    /// mark_eof() 前の close-delimited は常に None を返す
    #[test]
    fn prop_response_decode_close_delimited_returns_none_before_eof(
        body_data in proptest::collection::vec(any::<u8>(), 0..256)
    ) {
        let mut decoder = ResponseDecoder::new();
        decoder.feed(b"HTTP/1.1 200 OK\r\n\r\n").unwrap();
        decoder.feed(&body_data).unwrap();

        // mark_eof() を呼ばずに decode() → None
        let result = decoder.decode().unwrap();
        prop_assert!(result.is_none());

        // 追加データを feed しても None
        decoder.feed(b"more data").unwrap();
        let result = decoder.decode().unwrap();
        prop_assert!(result.is_none());
    }
}

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
        prop_assert!(is_body_too_large, "expected BodyTooLarge, got {:?}", result);
    }
}

// ========================================
// トンネルモードの PBT
// ========================================

proptest! {
    /// CONNECT 2xx 後に decode() → エラー
    #[test]
    fn prop_response_decode_tunnel_error(status in 200u16..300) {
        let mut decoder = ResponseDecoder::new();
        decoder.set_request_method("CONNECT");

        let response_data = format!("HTTP/1.1 {} OK\r\n\r\n", status);
        decoder.feed(response_data.as_bytes()).unwrap();

        let result = decoder.decode();
        prop_assert!(result.is_err());
        if let Err(Error::InvalidData(msg)) = result {
            prop_assert!(msg.contains("tunnel"));
        }
    }
}

proptest! {
    /// is_close_delimited() の状態確認
    #[test]
    fn prop_response_is_close_delimited(
        body_data in proptest::collection::vec(any::<u8>(), 0..64)
    ) {
        let mut decoder = ResponseDecoder::new();
        decoder.feed(b"HTTP/1.1 200 OK\r\n\r\n").unwrap();
        decoder.feed(&body_data).unwrap();

        let (_, body_kind) = decoder.decode_headers().unwrap().unwrap();
        prop_assert_eq!(body_kind, BodyKind::CloseDelimited);
        prop_assert!(decoder.is_close_delimited());

        // mark_eof() 後は false
        decoder.mark_eof();
        prop_assert!(!decoder.is_close_delimited());
    }
}

proptest! {
    /// トンネルモード後の take_remaining()
    #[test]
    fn prop_response_take_remaining_tunnel(
        extra_data in proptest::collection::vec(any::<u8>(), 1..128)
    ) {
        let mut decoder = ResponseDecoder::new();
        decoder.set_request_method("CONNECT");

        let mut response = b"HTTP/1.1 200 OK\r\n\r\n".to_vec();
        response.extend_from_slice(&extra_data);
        decoder.feed(&response).unwrap();

        let (_, body_kind) = decoder.decode_headers().unwrap().unwrap();
        prop_assert_eq!(body_kind, BodyKind::Tunnel);
        prop_assert!(decoder.is_tunnel());

        let remaining = decoder.take_remaining();
        prop_assert_eq!(&remaining, &extra_data);
    }
}

// ========================================
// close-delimited 段階的フィードの PBT
// ========================================

proptest! {
    /// close-delimited を段階的に feed + mark_eof
    #[test]
    fn prop_response_decode_close_delimited_incremental(
        chunks in proptest::collection::vec(
            proptest::collection::vec(any::<u8>(), 1..64),
            2..5
        )
    ) {
        let mut decoder = ResponseDecoder::new();
        decoder.feed(b"HTTP/1.1 200 OK\r\n\r\n").unwrap();

        // ヘッダーだけで decode → None
        let result = decoder.decode().unwrap();
        prop_assert!(result.is_none());

        // 各チャンクを feed して decode (すべて None)
        for chunk in &chunks {
            decoder.feed(chunk).unwrap();
            let result = decoder.decode().unwrap();
            prop_assert!(result.is_none());
        }

        // mark_eof() 後に decode() で取得
        decoder.mark_eof();
        let response = decoder.decode().unwrap().unwrap();

        let expected_body: Vec<u8> = chunks.into_iter().flatten().collect();
        prop_assert_eq!(&response.body, &expected_body);
    }
}

proptest! {
    /// close-delimited 以外で mark_eof は無視
    #[test]
    fn prop_response_mark_eof_non_close_delimited(
        body_data in proptest::collection::vec(any::<u8>(), 1..64)
    ) {
        let headers = format!(
            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\n\r\n",
            body_data.len()
        );
        let mut full = headers.into_bytes();
        full.extend_from_slice(&body_data);

        let mut decoder = ResponseDecoder::new();
        decoder.feed(&full).unwrap();

        // mark_eof は Content-Length ボディには影響しない
        decoder.mark_eof();
        prop_assert!(!decoder.is_close_delimited());

        let response = decoder.decode().unwrap().unwrap();
        prop_assert_eq!(&response.body, &body_data);
    }
}
