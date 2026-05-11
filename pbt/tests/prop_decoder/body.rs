//! ボディデコードの PBT (chunked, content-length, close-delimited)

use proptest::prelude::*;
use shiguredo_http11::{
    BodyKind, BodyProgress, DecoderLimits, Error, RequestDecoder, ResponseDecoder, encode_chunk,
    encode_chunks,
};

use super::body;

// ========================================
// チャンクエンコーディング PBT
// ========================================

proptest! {
    #[test]
    fn prop_chunked_invalid_size_error(
        invalid_size in "[G-Zg-z]{1,5}"
    ) {
        // 無効なチャンクサイズはエラー
        let data = format!("HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n{}\r\n", invalid_size);
        let mut decoder = ResponseDecoder::new();
        decoder.feed(data.as_bytes()).unwrap();
        let (_, body_kind) = decoder.decode_headers().unwrap().unwrap();
        prop_assert_eq!(body_kind, BodyKind::Chunked);
        prop_assert!(decoder.progress().is_err());
    }
}

proptest! {
    #[test]
    fn prop_chunked_size_with_extension_ok(
        body_content in "[a-z]{1,32}",
        ext_name in "[a-z]{1,8}",
        ext_value in "[a-z0-9]{1,8}"
    ) {
        // チャンク拡張は OK
        let len = body_content.len();
        let data = format!(
            "HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n{:x};{}={}\r\n{}\r\n0\r\n\r\n",
            len, ext_name, ext_value, body_content
        );
        let mut decoder = ResponseDecoder::new();
        decoder.feed(data.as_bytes()).unwrap();
        let (_, body_kind) = decoder.decode_headers().unwrap().unwrap();
        prop_assert_eq!(body_kind, BodyKind::Chunked);

        let mut body = Vec::new();
        loop {
            if let Some(data) = decoder.peek_body() {
                body.extend_from_slice(data);
                let len = data.len();
                if let BodyProgress::Complete { .. } = decoder.consume_body(len).unwrap() {
                    break;
                }
            } else if let BodyProgress::Complete { .. } = decoder.progress().unwrap() {
                break;
            }
        }
        prop_assert_eq!(body, body_content.as_bytes());
    }
}

proptest! {
    #[test]
    fn prop_chunked_with_trailer_ok(
        body_content in "[a-z]{1,32}",
        trailer_name in "[A-Za-z]{1,16}",
        trailer_value in "[a-z0-9]{1,16}"
    ) {
        // トレーラーは OK
        // RFC 9110 Section 6.5.1 ホワイトリスト方式: `Trailer:` ヘッダーで
        // 事前申告したフィールドのみ受理される。
        let len = body_content.len();
        let data = format!(
            "HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\nTrailer: {}\r\n\r\n{:x}\r\n{}\r\n0\r\n{}: {}\r\n\r\n",
            trailer_name, len, body_content, trailer_name, trailer_value
        );
        let mut decoder = ResponseDecoder::new();
        decoder.feed(data.as_bytes()).unwrap();
        let (_, body_kind) = decoder.decode_headers().unwrap().unwrap();
        prop_assert_eq!(body_kind, BodyKind::Chunked);

        let mut body = Vec::new();
        let trailers = loop {
            if let Some(data) = decoder.peek_body() {
                body.extend_from_slice(data);
                let len = data.len();
                if let BodyProgress::Complete { trailers } = decoder.consume_body(len).unwrap() {
                    break trailers;
                }
            } else if let BodyProgress::Complete { trailers } = decoder.progress().unwrap() {
                break trailers;
            }
        };
        prop_assert_eq!(body, body_content.as_bytes());
        prop_assert_eq!(trailers.len(), 1);
        prop_assert_eq!(&trailers[0].0, &trailer_name);
        prop_assert_eq!(&trailers[0].1, &trailer_value);
    }
}

proptest! {
    #[test]
    fn prop_chunked_with_multiple_trailers_ok(
        body_content in "[a-z]{1,32}",
        trailer_count in 1..4usize
    ) {
        // 複数のトレーラーは OK (`Trailer:` ヘッダーで事前申告したもののみ)
        let len = body_content.len();
        let declared = (0..trailer_count)
            .map(|i| format!("X-Trailer{}", i))
            .collect::<Vec<_>>()
            .join(", ");
        let trailers = (0..trailer_count)
            .map(|i| format!("X-Trailer{}: value{}", i, i))
            .collect::<Vec<_>>()
            .join("\r\n");
        let data = format!(
            "HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\nTrailer: {}\r\n\r\n{:x}\r\n{}\r\n0\r\n{}\r\n\r\n",
            declared, len, body_content, trailers
        );
        let mut decoder = ResponseDecoder::new();
        decoder.feed(data.as_bytes()).unwrap();
        let (_, _) = decoder.decode_headers().unwrap().unwrap();

        let mut body = Vec::new();
        let result_trailers = loop {
            if let Some(data) = decoder.peek_body() {
                body.extend_from_slice(data);
                let len = data.len();
                if let BodyProgress::Complete { trailers } = decoder.consume_body(len).unwrap() {
                    break trailers;
                }
            } else if let BodyProgress::Complete { trailers } = decoder.progress().unwrap() {
                break trailers;
            }
        };
        prop_assert_eq!(body, body_content.as_bytes());
        prop_assert_eq!(result_trailers.len(), trailer_count);
    }
}

proptest! {
    #[test]
    fn prop_chunked_multiple_chunks(
        chunk1 in proptest::collection::vec(any::<u8>(), 1..64),
        chunk2 in proptest::collection::vec(any::<u8>(), 1..64),
        chunk3 in proptest::collection::vec(any::<u8>(), 1..64)
    ) {
        let mut data = b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n".to_vec();

        data.extend(format!("{:x}\r\n", chunk1.len()).as_bytes());
        data.extend(&chunk1);
        data.extend(b"\r\n");

        data.extend(format!("{:x}\r\n", chunk2.len()).as_bytes());
        data.extend(&chunk2);
        data.extend(b"\r\n");

        data.extend(format!("{:x}\r\n", chunk3.len()).as_bytes());
        data.extend(&chunk3);
        data.extend(b"\r\n");

        data.extend(b"0\r\n\r\n");

        let mut decoder = ResponseDecoder::new();
        decoder.feed(&data).unwrap();
        let (_, _) = decoder.decode_headers().unwrap().unwrap();

        let mut body = Vec::new();
        loop {
            if let Some(d) = decoder.peek_body() {
                body.extend_from_slice(d);
                let len = d.len();
                if let BodyProgress::Complete { .. } = decoder.consume_body(len).unwrap() {
                    break;
                }
            } else if let BodyProgress::Complete { .. } = decoder.progress().unwrap() {
                break;
            }
        }
        let expected: Vec<u8> = [chunk1, chunk2, chunk3].concat();
        prop_assert_eq!(body, expected);
    }
}

proptest! {
    #[test]
    fn prop_chunked_roundtrip(chunks in proptest::collection::vec(body(), 1..5)) {
        let non_empty_chunks: Vec<Vec<u8>> = chunks.into_iter().filter(|c| !c.is_empty()).collect();
        let chunk_refs: Vec<&[u8]> = non_empty_chunks.iter().map(|c| c.as_slice()).collect();
        let encoded = encode_chunks(&chunk_refs);

        let mut decoder = ResponseDecoder::new();
        let header = b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n";
        decoder.feed(header).unwrap();
        decoder.feed(&encoded).unwrap();

        let (_, _) = decoder.decode_headers().unwrap().unwrap();

        let mut body = Vec::new();
        loop {
            if let Some(d) = decoder.peek_body() {
                body.extend_from_slice(d);
                let len = d.len();
                if let BodyProgress::Complete { .. } = decoder.consume_body(len).unwrap() {
                    break;
                }
            } else if let BodyProgress::Complete { .. } = decoder.progress().unwrap() {
                break;
            }
        }
        let expected: Vec<u8> = non_empty_chunks.iter().flatten().copied().collect();
        prop_assert_eq!(&body, &expected);
    }
}

proptest! {
    #[test]
    fn prop_encode_chunk_valid(data in body()) {
        let chunk = encode_chunk(&data);

        if data.is_empty() {
            prop_assert_eq!(&chunk, b"0\r\n\r\n");
        } else {
            let expected_size = format!("{:x}\r\n", data.len());
            prop_assert!(chunk.starts_with(expected_size.as_bytes()));
            prop_assert!(chunk.ends_with(b"\r\n"));
        }
    }
}

// ========================================
// チャンク CRLF 検証 PBT
// ========================================

proptest! {
    #[test]
    fn prop_chunked_invalid_crlf_after_data_error(
        body_content in "[a-z]{5,10}",
        invalid_char1 in prop::char::range('A', 'Z'),
        invalid_char2 in prop::char::range('A', 'Z')
    ) {
        // チャンクデータ後に CRLF ではなく別のバイトがある
        let len = body_content.len();
        let data = format!(
            "HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n{:x}\r\n{}{}{}",
            len, body_content, invalid_char1, invalid_char2
        );
        let mut decoder = ResponseDecoder::new();
        decoder.feed(data.as_bytes()).unwrap();
        let (_, _) = decoder.decode_headers().unwrap().unwrap();

        // チャンクサイズを処理
        decoder.progress().unwrap();
        // チャンクデータを消費
        let peeked = decoder.peek_body().unwrap();
        prop_assert_eq!(peeked, body_content.as_bytes());
        let result = decoder.consume_body(len);
        // CRLF ではないのでエラー
        prop_assert!(result.is_err());
    }
}

proptest! {
    #[test]
    fn prop_chunked_invalid_crlf_partial_then_error(
        body_content in "[a-z]{5,10}",
        invalid_chars in "[A-Z]{2,4}"
    ) {
        // 部分的にデータを受け取り、その後 CRLF ではない
        let len = body_content.len();
        let mut decoder = ResponseDecoder::new();
        let initial_data = format!(
            "HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n{:x}\r\n{}",
            len, body_content
        );
        decoder.feed(initial_data.as_bytes()).unwrap();
        let (_, _) = decoder.decode_headers().unwrap().unwrap();

        // チャンクサイズを処理
        decoder.progress().unwrap();
        // チャンクデータを消費（CRLF はまだない）
        decoder.consume_body(len).unwrap();

        // 不正な CRLF を追加
        decoder.feed(invalid_chars.as_bytes()).unwrap();
        let result = decoder.progress();
        prop_assert!(result.is_err());
    }
}

proptest! {
    #[test]
    fn prop_request_chunked_invalid_crlf_error(
        body_content in "[a-z]{5,10}",
        invalid_chars in "[A-Z]{2,4}"
    ) {
        // リクエストでもチャンクの CRLF 検証
        let len = body_content.len();
        let data = format!(
            "POST / HTTP/1.1\r\nHost: localhost\r\nTransfer-Encoding: chunked\r\n\r\n{:x}\r\n{}{}",
            len, body_content, invalid_chars
        );
        let mut decoder = RequestDecoder::new();
        decoder.feed(data.as_bytes()).unwrap();
        let (_, _) = decoder.decode_headers().unwrap().unwrap();

        decoder.progress().unwrap();
        let peeked = decoder.peek_body().unwrap();
        prop_assert_eq!(peeked, body_content.as_bytes());
        let result = decoder.consume_body(len);
        prop_assert!(result.is_err());
    }
}

// ========================================
// Chunked Keep-Alive 連続デコード PBT
// ========================================

proptest! {
    #[test]
    fn prop_decode_multiple_chunked_responses_with_body_limit(
        body1_len in 10..50usize,
        body2_len in 10..50usize
    ) {
        let limits = DecoderLimits {
            max_body_size: 100,
            ..DecoderLimits::default()
        };
        let mut decoder = ResponseDecoder::with_limits(limits);

        let body1 = "x".repeat(body1_len);
        let body2 = "y".repeat(body2_len);
        let resp1 = format!(
            "HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n{:x}\r\n{}\r\n0\r\n\r\n",
            body1.len(), body1
        );
        let resp2 = format!(
            "HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n{:x}\r\n{}\r\n0\r\n\r\n",
            body2.len(), body2
        );

        decoder.feed(resp1.as_bytes()).unwrap();
        decoder.feed(resp2.as_bytes()).unwrap();

        // 1 回目のデコード
        let response1 = decoder.decode().unwrap().unwrap();
        prop_assert_eq!(response1.body_bytes().map(<[u8]>::len), Some(body1_len));

        // 2 回目のデコード
        let response2 = decoder.decode().unwrap().unwrap();
        prop_assert_eq!(response2.body_bytes().map(<[u8]>::len), Some(body2_len));
    }
}

proptest! {
    #[test]
    fn prop_decode_multiple_chunked_requests_with_body_limit(
        body1_len in 10..50usize,
        body2_len in 10..50usize
    ) {
        let limits = DecoderLimits {
            max_body_size: 100,
            ..DecoderLimits::default()
        };
        let mut decoder = RequestDecoder::with_limits(limits);

        let body1 = "a".repeat(body1_len);
        let body2 = "b".repeat(body2_len);
        let req1 = format!(
            "POST /1 HTTP/1.1\r\nHost: localhost\r\nTransfer-Encoding: chunked\r\n\r\n{:x}\r\n{}\r\n0\r\n\r\n",
            body1.len(), body1
        );
        let req2 = format!(
            "POST /2 HTTP/1.1\r\nHost: localhost\r\nTransfer-Encoding: chunked\r\n\r\n{:x}\r\n{}\r\n0\r\n\r\n",
            body2.len(), body2
        );

        decoder.feed(req1.as_bytes()).unwrap();
        decoder.feed(req2.as_bytes()).unwrap();

        let request1 = decoder.decode().unwrap().unwrap();
        prop_assert_eq!(request1.body_bytes().map(<[u8]>::len), Some(body1_len));

        let request2 = decoder.decode().unwrap().unwrap();
        prop_assert_eq!(request2.body_bytes().map(<[u8]>::len), Some(body2_len));
    }
}

proptest! {
    #[test]
    fn prop_decode_multiple_chunked_responses_keep_alive_pbt(
        bodies in proptest::collection::vec(
            proptest::collection::vec(any::<u8>(), 0..64),
            2..4
        )
    ) {
        let mut decoder = ResponseDecoder::new();

        // chunked レスポンスを生成してバッファに追加
        let mut all_data = Vec::new();
        for body_data in &bodies {
            let mut data = b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n".to_vec();
            // 空ボディでない場合のみチャンクデータを追加
            if !body_data.is_empty() {
                data.extend(format!("{:x}\r\n", body_data.len()).as_bytes());
                data.extend(body_data);
                data.extend(b"\r\n");
            }
            // 終端チャンク
            data.extend(b"0\r\n\r\n");
            all_data.extend(data);
        }
        decoder.feed(&all_data).unwrap();

        // decode() を連続して呼ぶ
        for body_data in &bodies {
            let response = decoder.decode().unwrap().unwrap();
            prop_assert_eq!(response.body_bytes(), Some(body_data.as_slice()));
        }
    }
}

proptest! {
    #[test]
    fn prop_decode_multiple_chunked_requests_keep_alive_pbt(
        bodies in proptest::collection::vec(
            proptest::collection::vec(any::<u8>(), 0..64),
            2..4
        )
    ) {
        let mut decoder = RequestDecoder::new();

        // chunked リクエストを生成してバッファに追加
        let mut all_data = Vec::new();
        for (i, body_data) in bodies.iter().enumerate() {
            let mut data = format!("POST /{} HTTP/1.1\r\nHost: localhost\r\nTransfer-Encoding: chunked\r\n\r\n", i)
                .into_bytes();
            // 空ボディでない場合のみチャンクデータを追加
            if !body_data.is_empty() {
                data.extend(format!("{:x}\r\n", body_data.len()).as_bytes());
                data.extend(body_data);
                data.extend(b"\r\n");
            }
            // 終端チャンク
            data.extend(b"0\r\n\r\n");
            all_data.extend(data);
        }
        decoder.feed(&all_data).unwrap();

        // decode() を連続して呼ぶ
        for body_data in &bodies {
            let request = decoder.decode().unwrap().unwrap();
            prop_assert_eq!(request.body_bytes(), Some(body_data.as_slice()));
        }
    }
}

// ========================================
// リクエストとレスポンスの incomplete チャンクテスト
// ========================================

proptest! {
    #[test]
    fn prop_request_incomplete_chunk_size(
        size in 1..100usize
    ) {
        // 不完全なチャンクサイズ行は None
        let data = format!("POST / HTTP/1.1\r\nHost: localhost\r\nTransfer-Encoding: chunked\r\n\r\n{:x}", size);
        let mut decoder = RequestDecoder::new();
        decoder.feed(data.as_bytes()).unwrap();
        let (_, _) = decoder.decode_headers().unwrap().unwrap();
        prop_assert!(decoder.peek_body().is_none());
    }
}

proptest! {
    #[test]
    fn prop_request_incomplete_chunk_data(
        chunk_size in 10..100usize,
        partial_size in 1..10usize
    ) {
        // 不完全なチャンクデータは部分データを返す
        let partial_data = "x".repeat(partial_size);
        let data = format!(
            "POST / HTTP/1.1\r\nHost: localhost\r\nTransfer-Encoding: chunked\r\n\r\n{:x}\r\n{}",
            chunk_size, partial_data
        );
        let mut decoder = RequestDecoder::new();
        decoder.feed(data.as_bytes()).unwrap();
        let (_, _) = decoder.decode_headers().unwrap().unwrap();
        decoder.progress().unwrap();
        let peeked = decoder.peek_body().unwrap();
        prop_assert_eq!(peeked, partial_data.as_bytes());
    }
}

proptest! {
    #[test]
    fn prop_request_incomplete_trailer(
        body_content in "[a-z]{1,32}",
        trailer_name in "[A-Za-z]{1,16}"
    ) {
        // 不完全なトレーラーは Complete に到達してはならない
        let len = body_content.len();
        let data = format!(
            "POST / HTTP/1.1\r\nHost: localhost\r\nTransfer-Encoding: chunked\r\n\r\n{:x}\r\n{}\r\n0\r\n{}: value",
            len, body_content, trailer_name
        );
        let mut decoder = RequestDecoder::new();
        decoder.feed(data.as_bytes()).unwrap();
        let (_, _) = decoder.decode_headers().unwrap().unwrap();

        // 不完全なトレーラ行は Complete に到達せず、最終的に NeedData で停止する。
        // 多段遷移 (BodyChunkedSize → ChunkedTrailer) は Advanced を経由するため、
        // ループで NeedData / Complete のいずれかに収束するまで進める。
        let final_result = loop {
            if let Some(data) = decoder.peek_body() {
                let len = data.len();
                match decoder.consume_body(len).unwrap() {
                    r @ BodyProgress::Complete { .. } => break r,
                    BodyProgress::Advanced | BodyProgress::NeedData => continue,
                }
            }
            match decoder.progress().unwrap() {
                r @ BodyProgress::Complete { .. } => break r,
                BodyProgress::Advanced => continue,
                r @ BodyProgress::NeedData => break r,
            }
        };
        let is_complete = matches!(final_result, BodyProgress::Complete { .. });
        prop_assert!(!is_complete);
    }
}

// ========================================
// ChunkLineTooLong PBT
// ========================================

proptest! {
    #[test]
    fn response_decoder_chunk_line_too_long(
        ext_len in 100..200usize
    ) {
        let limits = DecoderLimits {
            max_chunk_line_size: 64,
            ..DecoderLimits::default()
        };
        let mut decoder = ResponseDecoder::with_limits(limits);
        // Transfer-Encoding: chunked のレスポンス
        decoder.feed(b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n").unwrap();
        let (_, body_kind) = decoder.decode_headers().unwrap().unwrap();
        prop_assert_eq!(body_kind, BodyKind::Chunked);

        // max_chunk_line_size を超える長いチャンク拡張を持つチャンクサイズ行
        let ext = "x".repeat(ext_len);
        let chunk_line = format!("5;ext={}\r\nhello\r\n0\r\n\r\n", ext);
        decoder.feed(chunk_line.as_bytes()).unwrap();

        // progress() でチャンクサイズ行をパースしようとするとエラー
        let result = decoder.progress();
        let is_chunk_line_too_long = matches!(result, Err(Error::ChunkLineTooLong { .. }));
        prop_assert!(is_chunk_line_too_long, "expected ChunkLineTooLong, got {:?}", result);
    }
}

proptest! {
    #[test]
    fn request_decoder_chunk_line_too_long(
        ext_len in 100..200usize
    ) {
        let limits = DecoderLimits {
            max_chunk_line_size: 64,
            ..DecoderLimits::default()
        };
        let mut decoder = RequestDecoder::with_limits(limits);
        // Transfer-Encoding: chunked のリクエスト
        decoder.feed(b"POST / HTTP/1.1\r\nHost: localhost\r\nTransfer-Encoding: chunked\r\n\r\n").unwrap();
        let (_, body_kind) = decoder.decode_headers().unwrap().unwrap();
        prop_assert_eq!(body_kind, BodyKind::Chunked);

        // max_chunk_line_size を超える長いチャンク拡張を持つチャンクサイズ行
        let ext = "x".repeat(ext_len);
        let chunk_line = format!("5;ext={}\r\nhello\r\n0\r\n\r\n", ext);
        decoder.feed(chunk_line.as_bytes()).unwrap();

        // progress() でチャンクサイズ行をパースしようとするとエラー
        let result = decoder.progress();
        let is_chunk_line_too_long = matches!(result, Err(Error::ChunkLineTooLong { .. }));
        prop_assert!(is_chunk_line_too_long, "expected ChunkLineTooLong, got {:?}", result);
    }
}

// ========================================
// decode() ラウンドトリップの PBT
// ========================================

proptest! {
    /// リクエストの chunked ボディの decode() ラウンドトリップ
    #[test]
    fn prop_request_decode_chunked_roundtrip(
        chunks in proptest::collection::vec(
            proptest::collection::vec(any::<u8>(), 1..64),
            1..4
        )
    ) {
        let headers = b"POST / HTTP/1.1\r\nHost: localhost\r\nTransfer-Encoding: chunked\r\n\r\n";
        let chunk_refs: Vec<&[u8]> = chunks.iter().map(|c| c.as_slice()).collect();
        let chunked_body = encode_chunks(&chunk_refs);

        let mut full = headers.to_vec();
        full.extend_from_slice(&chunked_body);

        let mut decoder = RequestDecoder::new();
        decoder.feed(&full).unwrap();
        let request = decoder.decode().unwrap().unwrap();

        let expected_body: Vec<u8> = chunks.into_iter().flatten().collect();
        prop_assert_eq!(request.body_bytes(), Some(expected_body.as_slice()));
        prop_assert_eq!(request.method(), "POST");
    }
}

proptest! {
    /// レスポンスの chunked ボディの decode() ラウンドトリップ
    #[test]
    fn prop_response_decode_chunked_roundtrip(
        chunks in proptest::collection::vec(
            proptest::collection::vec(any::<u8>(), 1..64),
            1..4
        )
    ) {
        let headers = b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n";
        let chunk_refs: Vec<&[u8]> = chunks.iter().map(|c| c.as_slice()).collect();
        let chunked_body = encode_chunks(&chunk_refs);

        let mut full = headers.to_vec();
        full.extend_from_slice(&chunked_body);

        let mut decoder = ResponseDecoder::new();
        decoder.feed(&full).unwrap();
        let response = decoder.decode().unwrap().unwrap();

        let expected_body: Vec<u8> = chunks.into_iter().flatten().collect();
        prop_assert_eq!(response.body_bytes(), Some(expected_body.as_slice()));
        prop_assert_eq!(response.status_code(), 200);
    }
}

// ========================================
// Content-Length ボディの decode() ラウンドトリップ PBT
// ========================================

proptest! {
    /// Content-Length ボディの decode() ラウンドトリップ (リクエスト)
    #[test]
    fn prop_request_decode_content_length_roundtrip(
        body_data in proptest::collection::vec(any::<u8>(), 1..256)
    ) {
        let headers = format!(
            "POST / HTTP/1.1\r\nHost: localhost\r\nContent-Length: {}\r\n\r\n",
            body_data.len()
        );
        let mut full = headers.into_bytes();
        full.extend_from_slice(&body_data);

        let mut decoder = RequestDecoder::new();
        decoder.feed(&full).unwrap();
        let request = decoder.decode().unwrap().unwrap();
        prop_assert_eq!(request.body_bytes(), Some(body_data.as_slice()));
        prop_assert_eq!(request.method(), "POST");
    }
}

proptest! {
    /// Content-Length ボディの decode() ラウンドトリップ (レスポンス)
    #[test]
    fn prop_response_decode_content_length_roundtrip(
        body_data in proptest::collection::vec(any::<u8>(), 1..256)
    ) {
        let headers = format!(
            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\n\r\n",
            body_data.len()
        );
        let mut full = headers.into_bytes();
        full.extend_from_slice(&body_data);

        let mut decoder = ResponseDecoder::new();
        decoder.feed(&full).unwrap();
        let response = decoder.decode().unwrap().unwrap();
        prop_assert_eq!(response.body_bytes(), Some(body_data.as_slice()));
        prop_assert_eq!(response.status_code(), 200);
    }
}

// ========================================
// Content-Length 部分消費の PBT
// ========================================

proptest! {
    /// Content-Length ボディを複数回に分けて消費
    #[test]
    fn prop_request_partial_body_consume(
        body_data in proptest::collection::vec(any::<u8>(), 4..256),
        split_ratio in 1..4usize
    ) {
        let data = format!(
            "POST / HTTP/1.1\r\nHost: localhost\r\nContent-Length: {}\r\n\r\n",
            body_data.len()
        );
        let mut full = data.into_bytes();
        full.extend_from_slice(&body_data);

        let mut decoder = RequestDecoder::new();
        decoder.feed(&full).unwrap();
        let (_, body_kind) = decoder.decode_headers().unwrap().unwrap();
        prop_assert_eq!(body_kind, BodyKind::ContentLength(body_data.len() as u64));

        // 複数回に分けて消費
        let first_len = body_data.len() / split_ratio.max(1);
        let first_len = first_len.max(1);
        let mut consumed = Vec::new();

        loop {
            let peeked = decoder.peek_body();
            if peeked.is_none() {
                // progress を試す
                match decoder.progress().unwrap() {
                    BodyProgress::Complete { .. } => break,
                    BodyProgress::Advanced => continue,
                    BodyProgress::NeedData => break,
                }
            }
            if let Some(data) = decoder.peek_body() {
                let take = data.len().min(first_len);
                consumed.extend_from_slice(&data[..take]);
                match decoder.consume_body(take).unwrap() {
                    BodyProgress::Complete { .. } => break,
                    BodyProgress::Advanced | BodyProgress::NeedData => continue,
                }
            } else {
                break;
            }
        }

        prop_assert_eq!(&consumed, &body_data);
    }
}
