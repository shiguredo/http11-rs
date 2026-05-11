//! RequestDecoder のプロパティテスト

use proptest::prelude::*;
use shiguredo_http11::{
    BodyKind, BodyProgress, DecoderLimits, Error, HttpHead, Request, RequestDecoder,
};

use super::{body, http_method, http_uri};

// ========================================
// リクエスト行のエラー PBT
// ========================================

proptest! {
    #[test]
    fn prop_request_line_missing_parts_error(
        method in http_method(),
        uri in http_uri()
    ) {
        // バージョンがないリクエスト行はエラー
        let data = format!("{} {}\r\n\r\n", method, uri);
        let mut decoder = RequestDecoder::new();
        decoder.feed(data.as_bytes()).unwrap();
        prop_assert!(decoder.decode_headers().is_err());
    }
}

proptest! {
    #[test]
    fn prop_request_line_empty_error(
        header_name in "[A-Za-z]{1,16}",
        header_value in "[A-Za-z0-9]{1,16}"
    ) {
        // 空のリクエスト行はエラー
        let data = format!("\r\n{}: {}\r\n\r\n", header_name, header_value);
        let mut decoder = RequestDecoder::new();
        decoder.feed(data.as_bytes()).unwrap();
        prop_assert!(decoder.decode_headers().is_err());
    }
}

// ========================================
// UTF-8 エラー PBT (リクエスト)
// ========================================

proptest! {
    #[test]
    fn prop_invalid_utf8_request_line_error(
        method in http_method(),
        invalid_byte in 128u8..=255
    ) {
        // 無効な UTF-8 バイトを含むリクエスト行はエラー
        let mut data = format!("{} /", method).into_bytes();
        data.push(invalid_byte);
        data.extend(b" HTTP/1.1\r\n\r\n");
        let mut decoder = RequestDecoder::new();
        decoder.feed(&data).unwrap();
        prop_assert!(decoder.decode_headers().is_err());
    }
}

proptest! {
    #[test]
    fn prop_invalid_utf8_header_error(
        header_name in "[A-Za-z]{1,16}",
        invalid_byte in 128u8..=255
    ) {
        // 無効な UTF-8 バイトを含むヘッダーはエラー
        let mut data = b"GET / HTTP/1.1\r\nHost: localhost\r\n".to_vec();
        data.extend(header_name.as_bytes());
        data.extend(b": ");
        data.push(invalid_byte);
        data.extend(b"\r\n\r\n");
        let mut decoder = RequestDecoder::new();
        decoder.feed(&data).unwrap();
        prop_assert!(decoder.decode_headers().is_err());
    }
}

// ========================================
// 部分的なデータ (None を返す) PBT (リクエスト)
// ========================================

proptest! {
    #[test]
    fn prop_incomplete_request_line(
        method in http_method(),
        uri in http_uri()
    ) {
        // CRLF がないリクエスト行は None
        let data = format!("{} {} HTTP/1.1", method, uri);
        let mut decoder = RequestDecoder::new();
        decoder.feed(data.as_bytes()).unwrap();
        prop_assert!(decoder.decode_headers().unwrap().is_none());
    }
}

proptest! {
    #[test]
    fn prop_incomplete_headers(
        header_name in "[A-Za-z]{1,16}",
        header_value in "[A-Za-z0-9]{1,16}"
    ) {
        // ヘッダー終端 CRLF がない場合は None
        let data = format!("GET / HTTP/1.1\r\nHost: localhost\r\n{}: {}", header_name, header_value);
        let mut decoder = RequestDecoder::new();
        decoder.feed(data.as_bytes()).unwrap();
        prop_assert!(decoder.decode_headers().unwrap().is_none());
    }
}

proptest! {
    #[test]
    fn prop_incomplete_body(
        body_length in 10..100usize,
        partial_length in 1..10usize
    ) {
        // 不完全なボディは peek_body で部分データを返す
        let full_body = "x".repeat(body_length);
        let partial_body = &full_body[..partial_length];
        let data = format!("GET / HTTP/1.1\r\nHost: localhost\r\nContent-Length: {}\r\n\r\n{}", body_length, partial_body);
        let mut decoder = RequestDecoder::new();
        decoder.feed(data.as_bytes()).unwrap();
        let (_, body_kind) = decoder.decode_headers().unwrap().unwrap();
        prop_assert_eq!(body_kind, BodyKind::ContentLength(body_length as u64));
        let peeked = decoder.peek_body().unwrap();
        prop_assert_eq!(peeked, partial_body.as_bytes());
    }
}

// ========================================
// デコーダーリミット PBT (リクエスト)
// ========================================

proptest! {
    #[test]
    fn prop_request_decoder_buffer_overflow(
        data_size in 1000..2000usize
    ) {
        let limits = DecoderLimits {
            max_buffer_size: 100,
            ..DecoderLimits::default()
        };
        let mut decoder = RequestDecoder::with_limits(limits);
        let data = "x".repeat(data_size);
        let result = decoder.feed(data.as_bytes());
        let is_buffer_overflow = matches!(result, Err(Error::BufferOverflow { .. }));
        prop_assert!(is_buffer_overflow, "expected BufferOverflow, got {:?}", result);
    }
}

proptest! {
    #[test]
    fn prop_request_decoder_exact_buffer_limit(
        extra_bytes in 0..10usize
    ) {
        let limits = DecoderLimits {
            max_buffer_size: 100,
            ..DecoderLimits::default()
        };
        let mut decoder = RequestDecoder::with_limits(limits);
        let data = "x".repeat(100 + extra_bytes);
        if extra_bytes == 0 {
            prop_assert!(decoder.feed(data.as_bytes()).is_ok());
        } else {
            let result = decoder.feed(data.as_bytes());
            let is_buffer_overflow = matches!(result, Err(Error::BufferOverflow { .. }));
            prop_assert!(is_buffer_overflow, "expected BufferOverflow, got {:?}", result);
        }
    }
}

proptest! {
    #[test]
    fn prop_request_decoder_header_line_too_long(
        header_value_len in 200..500usize
    ) {
        let limits = DecoderLimits {
            max_header_line_size: 100,
            ..DecoderLimits::default()
        };
        let mut decoder = RequestDecoder::with_limits(limits);
        let header_value = "x".repeat(header_value_len);
        let data = format!("GET / HTTP/1.1\r\nHost: localhost\r\nX-Long: {}\r\n\r\n", header_value);
        decoder.feed(data.as_bytes()).unwrap();
        let result = decoder.decode_headers();
        let is_header_line_too_long = matches!(result, Err(Error::HeaderLineTooLong { .. }));
        prop_assert!(is_header_line_too_long, "expected HeaderLineTooLong, got {:?}", result);
    }
}

proptest! {
    #[test]
    fn prop_request_decoder_too_many_headers(
        header_count in 20..50usize
    ) {
        let limits = DecoderLimits {
            max_headers_count: 10,
            ..DecoderLimits::default()
        };
        let mut decoder = RequestDecoder::with_limits(limits);
        let headers = (0..header_count)
            .map(|i| format!("X-Header{}: value{}", i, i))
            .collect::<Vec<_>>()
            .join("\r\n");
        let data = format!("GET / HTTP/1.1\r\nHost: localhost\r\n{}\r\n\r\n", headers);
        decoder.feed(data.as_bytes()).unwrap();
        let result = decoder.decode_headers();
        let is_too_many_headers = matches!(result, Err(Error::TooManyHeaders { .. }));
        prop_assert!(is_too_many_headers, "expected TooManyHeaders, got {:?}", result);
    }
}

proptest! {
    #[test]
    fn prop_request_decoder_exact_header_count(
        extra_headers in 0..5usize
    ) {
        let max_count = 10;
        let limits = DecoderLimits {
            max_headers_count: max_count,
            ..DecoderLimits::default()
        };
        let mut decoder = RequestDecoder::with_limits(limits);
        // Host ヘッダーが1つあるので、残りの枠は max_count - 1
        let header_count = (max_count - 1) + extra_headers;
        let headers = (0..header_count)
            .map(|i| format!("X-H{}: v{}", i, i))
            .collect::<Vec<_>>()
            .join("\r\n");
        let data = format!("GET / HTTP/1.1\r\nHost: localhost\r\n{}\r\n\r\n", headers);
        decoder.feed(data.as_bytes()).unwrap();
        if extra_headers == 0 {
            // Host + 9 headers = 10 (ちょうど max_count)
            prop_assert!(decoder.decode_headers().is_ok());
        } else {
            let result = decoder.decode_headers();
            let is_too_many_headers = matches!(result, Err(Error::TooManyHeaders { .. }));
            prop_assert!(is_too_many_headers, "expected TooManyHeaders, got {:?}", result);
        }
    }
}

proptest! {
    #[test]
    fn prop_request_decoder_body_too_large_content_length(
        body_size in 200..500usize
    ) {
        let limits = DecoderLimits {
            max_body_size: 100,
            ..DecoderLimits::default()
        };
        let mut decoder = RequestDecoder::with_limits(limits);
        let body = "x".repeat(body_size);
        let data = format!("POST / HTTP/1.1\r\nHost: localhost\r\nContent-Length: {}\r\n\r\n{}", body_size, body);
        decoder.feed(data.as_bytes()).unwrap();
        let result = decoder.decode_headers();
        // Content-Length が制限を超えているのでエラー
        prop_assert!(result.is_err());
    }
}

proptest! {
    #[test]
    fn prop_request_decoder_exact_body_size(
        extra_bytes in 0..10usize
    ) {
        let max_size = 100;
        let limits = DecoderLimits {
            max_body_size: max_size,
            ..DecoderLimits::default()
        };
        let mut decoder = RequestDecoder::with_limits(limits);
        let body_size = max_size + extra_bytes;
        let body = "x".repeat(body_size);
        let data = format!("POST / HTTP/1.1\r\nHost: localhost\r\nContent-Length: {}\r\n\r\n{}", body_size, body);
        decoder.feed(data.as_bytes()).unwrap();
        if extra_bytes == 0 {
            prop_assert!(decoder.decode_headers().is_ok());
        } else {
            prop_assert!(decoder.decode_headers().is_err());
        }
    }
}

proptest! {
    #[test]
    fn prop_request_decoder_body_too_large_chunked(
        chunk_size in 200..500usize
    ) {
        let limits = DecoderLimits {
            max_body_size: 100,
            ..DecoderLimits::default()
        };
        let mut decoder = RequestDecoder::with_limits(limits);
        let chunk = "x".repeat(chunk_size);
        let data = format!(
            "POST / HTTP/1.1\r\nHost: localhost\r\nTransfer-Encoding: chunked\r\n\r\n{:x}\r\n{}\r\n0\r\n\r\n",
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
    fn prop_request_decoder_feed_unchecked_no_limit(
        data_size in 1000..2000usize
    ) {
        let limits = DecoderLimits {
            max_buffer_size: 100,
            ..DecoderLimits::default()
        };
        let mut decoder = RequestDecoder::with_limits(limits);
        let data = "x".repeat(data_size);
        // feed_unchecked はバッファ制限をチェックしない
        decoder.feed_unchecked(data.as_bytes());
        prop_assert_eq!(decoder.remaining().len(), data_size);
    }
}

proptest! {
    #[test]
    fn prop_request_decoder_limits_getter(
        max_buffer_size in 100..1000usize,
        max_body_size in 100..1000usize
    ) {
        let limits = DecoderLimits {
            max_buffer_size,
            max_body_size,
            ..DecoderLimits::default()
        };
        let decoder = RequestDecoder::with_limits(limits.clone());
        prop_assert_eq!(decoder.limits().max_buffer_size, max_buffer_size);
        prop_assert_eq!(decoder.limits().max_body_size, max_body_size);
    }
}

// ========================================
// 複数リクエスト PBT
// ========================================

proptest! {
    #[test]
    fn prop_multiple_requests_same_decoder(
        methods in proptest::collection::vec(http_method(), 2..5),
        uris in proptest::collection::vec(http_uri(), 2..5)
    ) {
        let count = methods.len().min(uris.len());
        let mut decoder = RequestDecoder::new();

        for i in 0..count {
            let mut request = Request::new(&methods[i], &uris[i]).unwrap();
            request.add_header("Host", "localhost").unwrap();
            let encoded = request.encode();
            decoder.feed(&encoded).unwrap();
            let decoded = decoder.decode().unwrap().unwrap();
            prop_assert_eq!(decoded.method(), methods[i].as_str());
            prop_assert_eq!(decoded.uri(), uris[i].as_str());
            decoder.reset();
        }
    }
}

proptest! {
    #[test]
    fn prop_decoder_reuse_after_error(
        valid_method in http_method(),
        valid_uri in http_uri()
    ) {
        let mut decoder = RequestDecoder::new();
        // 不正なリクエストでエラー
        decoder.feed(b"INVALID\r\n\r\n").unwrap();
        let _ = decoder.decode_headers();
        decoder.reset();
        // リセット後は正常に動作
        let mut request = Request::new(&valid_method, &valid_uri).unwrap();
        request.add_header("Host", "localhost").unwrap();
        decoder.feed(&request.encode()).unwrap();
        let decoded = decoder.decode().unwrap().unwrap();
        prop_assert_eq!(decoded.method(), valid_method.as_str());
    }
}

// ========================================
// ストリーミング API の PBT (リクエスト)
// ========================================

proptest! {
    #[test]
    fn prop_streaming_decode_request(
        method in http_method(),
        uri in http_uri(),
        header_count in 0..5usize
    ) {
        let mut decoder = RequestDecoder::new();
        let headers = (0..header_count)
            .map(|i| format!("X-Header{}: value{}", i, i))
            .collect::<Vec<_>>()
            .join("\r\n");
        let data = if header_count > 0 {
            format!("{} {} HTTP/1.1\r\nHost: localhost\r\n{}\r\n\r\n", method, uri, headers)
        } else {
            format!("{} {} HTTP/1.1\r\nHost: localhost\r\n\r\n", method, uri)
        };
        decoder.feed(data.as_bytes()).unwrap();
        let (head, body_kind) = decoder.decode_headers().unwrap().unwrap();
        prop_assert_eq!(head.method(), method);
        prop_assert_eq!(head.uri(), uri);
        prop_assert_eq!(body_kind, BodyKind::None);
    }
}

proptest! {
    #[test]
    fn prop_streaming_decode_request_with_body(
        method in prop_oneof![Just("POST"), Just("PUT")],
        body_content in "[a-z]{1,100}"
    ) {
        let mut decoder = RequestDecoder::new();
        let body_len = body_content.len();
        let data = format!(
            "{} / HTTP/1.1\r\nHost: localhost\r\nContent-Length: {}\r\n\r\n{}",
            method, body_len, body_content
        );
        decoder.feed(data.as_bytes()).unwrap();
        let (head, body_kind) = decoder.decode_headers().unwrap().unwrap();
        prop_assert_eq!(head.method(), method);
        prop_assert_eq!(body_kind, BodyKind::ContentLength(body_len as u64));

        let mut body = Vec::new();
        while let Some(data) = decoder.peek_body() {
            body.extend_from_slice(data);
            let len = data.len();
            if let BodyProgress::Complete { .. } = decoder.consume_body(len).unwrap() {
                break;
            }
        }
        prop_assert_eq!(body, body_content.as_bytes());
    }
}

// ========================================
// Request ラウンドトリップ PBT
// ========================================

proptest! {
    #[test]
    fn prop_request_roundtrip(
        method in http_method(),
        uri in http_uri(),
        body_data in body()
    ) {
        let mut request = Request::new(&method, &uri)
            .unwrap()
            .header("Host", "example.com")
            .unwrap();

        if !body_data.is_empty() {
            request = request.body(body_data.clone());
        }

        let encoded = request.encode();
        let mut decoder = RequestDecoder::new();
        decoder.feed(&encoded).unwrap();
        let decoded = decoder.decode().unwrap().unwrap();

        prop_assert_eq!(decoded.method(), method.as_str());
        prop_assert_eq!(decoded.uri(), uri.as_str());
        // body_data が空のときは .body() を呼んでいないので request.body == None。
        // エンコード時に Content-Length も付かないため、デコーダーは body == None を返す。
        let expected: Option<&[u8]> = if body_data.is_empty() {
            None
        } else {
            Some(body_data.as_slice())
        };
        prop_assert_eq!(decoded.body_bytes(), expected);
    }
}

// ========================================
// decode_headers を2回呼んだ場合の挙動 PBT (リクエスト)
// ========================================

proptest! {
    #[test]
    fn prop_decode_headers_twice_returns_none(
        uri in "[a-z]{1,10}"
    ) {
        // ボディなしメッセージの場合、2 回目の decode_headers は Ok(None)
        let data = format!("GET /{} HTTP/1.1\r\nHost: localhost\r\n\r\n", uri);
        let mut decoder = RequestDecoder::new();
        decoder.feed(data.as_bytes()).unwrap();
        let _ = decoder.decode_headers().unwrap().unwrap();
        // 2回目は次のメッセージがないので Ok(None)
        prop_assert!(decoder.decode_headers().unwrap().is_none());
    }
}

// ========================================
// consume_body を decode_headers 前に呼ぶとエラー PBT (リクエスト)
// ========================================

proptest! {
    #[test]
    fn prop_consume_body_before_decode_headers_error(
        method in http_method()
    ) {
        let mut decoder = RequestDecoder::new();
        let data = format!("{} / HTTP/1.1\r\nHost: localhost\r\n\r\n", method);
        decoder.feed(data.as_bytes()).unwrap();
        prop_assert!(decoder.progress().is_err());
    }
}

// ========================================
// decode() API の連続デコードテスト (Keep-Alive) PBT (リクエスト)
// ========================================

proptest! {
    #[test]
    fn prop_decode_multiple_requests_keep_alive(
        methods in proptest::collection::vec(http_method(), 2..5),
        uris in proptest::collection::vec(http_uri(), 2..5)
    ) {
        let count = methods.len().min(uris.len());
        let mut decoder = RequestDecoder::new();

        // 全リクエストを一度にバッファに入れる
        let mut all_data = Vec::new();
        for i in 0..count {
            let mut request = Request::new(&methods[i], &uris[i]).unwrap();
            request.add_header("Host", "localhost").unwrap();
            all_data.extend(request.encode());
        }
        decoder.feed(&all_data).unwrap();

        // decode() を連続して呼ぶ（reset() なし）
        for i in 0..count {
            let request = decoder.decode().unwrap().unwrap();
            prop_assert_eq!(request.method(), methods[i].as_str());
            prop_assert_eq!(request.uri(), uris[i].as_str());
        }
    }
}

proptest! {
    #[test]
    fn prop_decode_multiple_requests_with_body_keep_alive(
        bodies in proptest::collection::vec(
            proptest::collection::vec(any::<u8>(), 0..64),
            2..4
        )
    ) {
        let mut decoder = RequestDecoder::new();

        // 全リクエストを一度にバッファに入れる
        let mut all_data = Vec::new();
        for body_data in &bodies {
            let mut request = Request::new("POST", "/").unwrap();
            request.add_header("Host", "localhost").unwrap();
            let request = request.body(body_data.clone());
            all_data.extend(request.encode());
        }
        decoder.feed(&all_data).unwrap();

        // decode() を連続して呼ぶ（reset() なし）
        // body == Some(empty) でもエンコード時に Content-Length: 0 が付くため、
        // デコーダー側も Some(empty) を返す。
        for body_data in &bodies {
            let request = decoder.decode().unwrap().unwrap();
            prop_assert_eq!(request.body_bytes(), Some(body_data.as_slice()));
        }
    }
}

// ========================================
// decode_headers の Complete → StartLine 遷移 PBT (リクエスト)
// ========================================

proptest! {
    #[test]
    fn prop_decode_headers_multiple_no_body_messages(
        count in 2..5usize
    ) {
        // 複数のボディなしメッセージを decode_headers で連続処理
        let mut decoder = RequestDecoder::new();
        for i in 0..count {
            let data = format!("GET /{} HTTP/1.1\r\nHost: localhost\r\n\r\n", i);
            decoder.feed(data.as_bytes()).unwrap();
        }

        for i in 0..count {
            let (head, body_kind) = decoder.decode_headers().unwrap().unwrap();
            prop_assert_eq!(head.uri(), format!("/{}", i));
            prop_assert!(matches!(body_kind, BodyKind::None));
        }

        // 次のメッセージがなければ Ok(None)
        prop_assert!(decoder.decode_headers().unwrap().is_none());
    }
}

// ========================================
// 小文字メソッドの PBT
// ========================================

proptest! {
    #![proptest_config(proptest::prelude::ProptestConfig::with_cases(100))]

    /// RFC 9110 Section 9: method = token
    /// 小文字メソッドも token として有効なので受理される
    #[test]
    fn prop_lowercase_method_accepted(
        method in prop_oneof![
            Just("get"),
            Just("post"),
            Just("put"),
            Just("delete"),
            Just("Get"),
            Just("Post"),
            Just("gET"),
        ]
    ) {
        let mut decoder = RequestDecoder::new();
        let data = format!("{} / HTTP/1.1\r\nHost: localhost\r\n\r\n", method);
        decoder.feed(data.as_bytes()).unwrap();
        let result = decoder.decode_headers();
        prop_assert!(result.is_ok(), "token method '{}' should be accepted, got {:?}", method, result);
    }

    /// 大文字メソッドが許可されることを確認
    #[test]
    fn prop_uppercase_method_allowed(
        method in prop_oneof![
            Just("GET"),
            Just("POST"),
            Just("PUT"),
            Just("DELETE"),
            Just("HEAD"),
            Just("OPTIONS"),
            Just("PATCH"),
        ]
    ) {
        let mut decoder = RequestDecoder::new();
        let data = format!("{} / HTTP/1.1\r\nHost: localhost\r\n\r\n", method);
        decoder.feed(data.as_bytes()).unwrap();
        let result = decoder.decode_headers();
        prop_assert!(result.is_ok(), "expected success for uppercase method '{}', got {:?}", method, result);
    }

    /// アンダースコアとハイフンを含むメソッドが許可されることを確認
    #[test]
    fn prop_method_with_underscore_hyphen_allowed(
        method in prop_oneof![
            Just("X-CUSTOM"),
            Just("MY_METHOD"),
            Just("GET_PARAMETER"),
            Just("SET_PARAMETER"),
            Just("X-MY-METHOD"),
        ]
    ) {
        let mut decoder = RequestDecoder::new();
        let data = format!("{} / HTTP/1.1\r\nHost: localhost\r\n\r\n", method);
        decoder.feed(data.as_bytes()).unwrap();
        let result = decoder.decode_headers();
        prop_assert!(result.is_ok(), "expected success for method with underscore/hyphen '{}', got {:?}", method, result);
    }
}

// ========================================
// ストリーミング API 混在エラーの PBT
// ========================================

proptest! {
    /// decode_headers() → body phase → decode() エラー
    #[test]
    fn prop_request_decode_mixed_api_error(
        body_data in proptest::collection::vec(any::<u8>(), 1..64)
    ) {
        let headers = format!(
            "POST / HTTP/1.1\r\nHost: localhost\r\nContent-Length: {}\r\n\r\n",
            body_data.len()
        );
        let mut full = headers.into_bytes();
        full.extend_from_slice(&body_data);

        let mut decoder = RequestDecoder::new();
        decoder.feed(&full).unwrap();

        // ストリーミング API で decode_headers() を呼ぶ
        let (_, body_kind) = decoder.decode_headers().unwrap().unwrap();
        prop_assert_eq!(body_kind, BodyKind::ContentLength(body_data.len() as u64));

        // decode() を呼ぶとエラー (ストリーミング API と混在)
        let result = decoder.decode();
        prop_assert!(result.is_err());
        if let Err(Error::InvalidData(msg)) = result {
            prop_assert!(msg.contains("mixed"));
        }
    }
}

// ========================================
// request-target 形式のテスト (RFC 9112 Section 3.2)
// ========================================

proptest! {
    /// OPTIONS * (asterisk-form)
    #[test]
    fn prop_request_asterisk_form(
        version in prop_oneof![Just("HTTP/1.1"), Just("HTTP/1.0"), Just("RTSP/1.0"), Just("RTSP/2.0")]
    ) {
        let data = format!("OPTIONS * {}\r\nHost: localhost\r\n\r\n", version);
        let mut decoder = RequestDecoder::new();
        decoder.feed(data.as_bytes()).unwrap();
        let result = decoder.decode_headers().unwrap();
        prop_assert!(result.is_some());
        let (head, _) = result.unwrap();
        prop_assert_eq!(head.method(), "OPTIONS");
        prop_assert_eq!(head.uri(), "*");
    }
}

proptest! {
    /// CONNECT host:port (authority-form)
    #[test]
    fn prop_request_authority_form(
        host in "[a-z]{1,16}\\.[a-z]{2,4}",
        port in 1u16..=65535
    ) {
        let target = format!("{}:{}", host, port);
        let data = format!("CONNECT {} HTTP/1.1\r\nHost: {}\r\n\r\n", target, target);
        let mut decoder = RequestDecoder::new();
        decoder.feed(data.as_bytes()).unwrap();
        let result = decoder.decode_headers().unwrap();
        prop_assert!(result.is_some());
        let (head, _) = result.unwrap();
        prop_assert_eq!(head.method(), "CONNECT");
        prop_assert_eq!(head.uri(), target.as_str());
    }
}

proptest! {
    /// absolute-form (http://host/path)
    #[test]
    fn prop_request_absolute_form(
        method in prop_oneof![Just("GET"), Just("POST"), Just("PUT")],
        host in "[a-z]{1,16}\\.[a-z]{2,4}",
        path in "/[a-z]{1,16}"
    ) {
        let target = format!("http://{}{}", host, path);
        let data = format!("{} {} HTTP/1.1\r\nHost: {}\r\n\r\n", method, target, host);
        let mut decoder = RequestDecoder::new();
        decoder.feed(data.as_bytes()).unwrap();
        let result = decoder.decode_headers().unwrap();
        prop_assert!(result.is_some());
        let (head, _) = result.unwrap();
        prop_assert_eq!(head.method(), method);
        prop_assert_eq!(head.uri(), target.as_str());
    }
}

proptest! {
    /// origin-form にクエリパラメータを含む URI
    #[test]
    fn prop_request_origin_form_with_query(
        path in "/[a-zA-Z0-9]{1,16}",
        key in "[a-zA-Z]{1,8}",
        value in "[a-zA-Z0-9]{1,8}"
    ) {
        let uri = format!("{}?{}={}", path, key, value);
        let data = format!("GET {} HTTP/1.1\r\nHost: localhost\r\n\r\n", uri);
        let mut decoder = RequestDecoder::new();
        decoder.feed(data.as_bytes()).unwrap();
        let result = decoder.decode_headers().unwrap();
        prop_assert!(result.is_some());
        let (head, _) = result.unwrap();
        prop_assert_eq!(head.uri(), uri.as_str());
    }
}

proptest! {
    /// origin-form にパーセントエンコーディングを含むパス
    #[test]
    fn prop_request_origin_form_with_percent_encoding(
        prefix in "/[a-zA-Z]{1,8}",
        hex1 in prop_oneof![Just("2F"), Just("20"), Just("3D"), Just("3F"), Just("41")],
        suffix in "[a-zA-Z]{1,8}"
    ) {
        let uri = format!("{}%{}{}", prefix, hex1, suffix);
        let data = format!("GET {} HTTP/1.1\r\nHost: localhost\r\n\r\n", uri);
        let mut decoder = RequestDecoder::new();
        decoder.feed(data.as_bytes()).unwrap();
        let result = decoder.decode_headers().unwrap();
        prop_assert!(result.is_some());
        let (head, _) = result.unwrap();
        prop_assert_eq!(head.uri(), uri.as_str());
    }
}

proptest! {
    /// CONNECT 以外のメソッドで authority-form はエラー
    #[test]
    fn prop_request_non_connect_authority_form_error(
        method in prop_oneof![Just("GET"), Just("POST"), Just("PUT"), Just("DELETE")],
        host in "[a-z]{1,16}\\.[a-z]{2,4}",
        port in 1u16..=65535
    ) {
        let target = format!("{}:{}", host, port);
        let data = format!("{} {} HTTP/1.1\r\nHost: localhost\r\n\r\n", method, target);
        let mut decoder = RequestDecoder::new();
        decoder.feed(data.as_bytes()).unwrap();
        let result = decoder.decode_headers();
        prop_assert!(result.is_err());
    }
}

proptest! {
    /// GET/POST 等で asterisk-form はエラー
    #[test]
    fn prop_request_non_options_asterisk_form_error(
        method in prop_oneof![Just("GET"), Just("POST"), Just("PUT"), Just("DELETE"), Just("PATCH")]
    ) {
        let data = format!("{} * HTTP/1.1\r\nHost: localhost\r\n\r\n", method);
        let mut decoder = RequestDecoder::new();
        decoder.feed(data.as_bytes()).unwrap();
        let result = decoder.decode_headers();
        prop_assert!(result.is_err());
    }
}

// ========================================
// 直接書き込み API (mut_buf / advance_buf / available_buf) のプロパティ
// ========================================

fn message_with_chunks() -> impl Strategy<Value = (Vec<u8>, Vec<usize>)> {
    body().prop_flat_map(|body_data| {
        let headers = format!(
            "POST / HTTP/1.1\r\nHost: example.com\r\nContent-Length: {}\r\n\r\n",
            body_data.len()
        );
        let mut full = headers.into_bytes();
        full.extend_from_slice(&body_data);
        let len = full.len();
        let chunks = if len == 0 {
            Just(Vec::<usize>::new()).boxed()
        } else {
            proptest::collection::vec(1usize..=len.max(1), 0..=8).boxed()
        };
        (Just(full), chunks)
    })
}

proptest! {
    /// `feed` と `mut_buf` + `advance_buf` で同じ結果になることを確認
    #[test]
    fn prop_feed_mut_buf_equivalence(
        (full, chunk_sizes) in message_with_chunks(),
    ) {
        let by_feed = {
            let mut decoder = RequestDecoder::new();
            let mut offset = 0usize;
            for &size in &chunk_sizes {
                if offset >= full.len() { break; }
                let end = (offset + size).min(full.len());
                decoder.feed(&full[offset..end]).unwrap();
                offset = end;
            }
            if offset < full.len() {
                decoder.feed(&full[offset..]).unwrap();
            }
            decoder.decode().unwrap()
        };

        let by_mut_buf = {
            let mut decoder = RequestDecoder::new();
            let mut offset = 0usize;
            for &size in &chunk_sizes {
                if offset >= full.len() { break; }
                let end = (offset + size).min(full.len());
                let len = end - offset;
                let dst = decoder.mut_buf(len).unwrap();
                dst.copy_from_slice(&full[offset..end]);
                decoder.advance_buf(len);
                offset = end;
            }
            if offset < full.len() {
                let len = full.len() - offset;
                let dst = decoder.mut_buf(len).unwrap();
                dst.copy_from_slice(&full[offset..]);
                decoder.advance_buf(len);
            }
            decoder.decode().unwrap()
        };

        let by_feed = by_feed.expect("feed path produced request");
        let by_mut_buf = by_mut_buf.expect("mut_buf path produced request");
        prop_assert_eq!(by_feed.method(), by_mut_buf.method());
        prop_assert_eq!(by_feed.uri(), by_mut_buf.uri());
        prop_assert_eq!(HttpHead::headers(&by_feed), HttpHead::headers(&by_mut_buf));
        prop_assert_eq!(by_feed.body_bytes(), by_mut_buf.body_bytes());
    }
}

proptest! {
    /// `mut_buf(len)` の戻りスライス長は常に `len`
    #[test]
    fn prop_mut_buf_returns_exact_length(len in 0usize..4096) {
        let mut decoder = RequestDecoder::new();
        let buf = decoder.mut_buf(len).unwrap();
        prop_assert_eq!(buf.len(), len);
        decoder.advance_buf(0);
    }
}

proptest! {
    /// `advance_buf(n)` 後の `remaining().len()` は (前回の remaining) + n になる
    #[test]
    fn prop_advance_buf_grows_remaining(
        prefix in proptest::collection::vec(any::<u8>(), 0..64),
        write_len in 0usize..256,
        advance in 0usize..256,
    ) {
        let advance = advance.min(write_len);
        let mut decoder = RequestDecoder::new();
        if !prefix.is_empty() {
            decoder.feed(&prefix).unwrap();
        }
        let before = decoder.remaining().len();
        let buf = decoder.mut_buf(write_len).unwrap();
        for (i, slot) in buf.iter_mut().enumerate() {
            *slot = (i & 0xff) as u8;
        }
        decoder.advance_buf(advance);
        prop_assert_eq!(decoder.remaining().len(), before + advance);
    }
}

proptest! {
    /// `mut_buf` 後 `advance_buf(0)` で `remaining()` が `mut_buf` 前と同じになる
    #[test]
    fn prop_advance_zero_is_identity(
        prefix in proptest::collection::vec(any::<u8>(), 0..64),
        write_len in 0usize..256,
    ) {
        let mut decoder = RequestDecoder::new();
        if !prefix.is_empty() {
            decoder.feed(&prefix).unwrap();
        }
        let before = decoder.remaining().to_vec();
        let _ = decoder.mut_buf(write_len).unwrap();
        decoder.advance_buf(0);
        prop_assert_eq!(decoder.remaining(), before.as_slice());
    }
}
