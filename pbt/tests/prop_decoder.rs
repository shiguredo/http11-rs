//! Decoder のプロパティテスト (decoder.rs)

use proptest::prelude::*;
use shiguredo_http11::{
    DecoderLimits, Request, RequestDecoder, Response, ResponseDecoder, encode_chunk, encode_chunks,
};

// ========================================
// Strategy 定義
// ========================================

fn token_char() -> impl Strategy<Value = char> {
    prop_oneof![
        prop::char::range('a', 'z'),
        prop::char::range('A', 'Z'),
        prop::char::range('0', '9'),
        Just('-'),
        Just('_'),
        Just('.'),
    ]
}

fn token_string(max_len: usize) -> impl Strategy<Value = String> {
    proptest::collection::vec(token_char(), 1..=max_len)
        .prop_map(|chars| chars.into_iter().collect())
}

fn header_name() -> impl Strategy<Value = String> {
    token_string(32)
}

fn header_value() -> impl Strategy<Value = String> {
    "[^\r\n]{0,64}".prop_filter("non-empty preferred", |s| !s.is_empty())
}

fn http_method() -> impl Strategy<Value = String> {
    prop_oneof![
        Just("GET".to_string()),
        Just("POST".to_string()),
        Just("PUT".to_string()),
        Just("DELETE".to_string()),
        Just("HEAD".to_string()),
        Just("OPTIONS".to_string()),
        Just("PATCH".to_string()),
    ]
}

fn http_uri() -> impl Strategy<Value = String> {
    prop_oneof![
        Just("/".to_string()),
        "/[a-zA-Z0-9/_.-]{1,64}".prop_map(|s| s),
    ]
}

fn status_code() -> impl Strategy<Value = u16> {
    prop_oneof![
        100u16..=101,
        200u16..=206,
        300u16..=308,
        400u16..=451,
        500u16..=511,
    ]
}

fn reason_phrase() -> impl Strategy<Value = String> {
    prop_oneof![
        Just("OK".to_string()),
        Just("Not Found".to_string()),
        Just("Internal Server Error".to_string()),
        "[A-Za-z ]{1,32}".prop_map(|s| s),
    ]
}

fn headers() -> impl Strategy<Value = Vec<(String, String)>> {
    proptest::collection::vec((header_name(), header_value()), 0..10)
}

fn body() -> impl Strategy<Value = Vec<u8>> {
    proptest::collection::vec(any::<u8>(), 0..256)
}

// ========================================
// ヘッダーパースエラーのテスト
// ========================================

#[test]
fn header_obs_fold_space_error() {
    let data = b"GET / HTTP/1.1\r\n Header: value\r\n\r\n";
    let mut decoder = RequestDecoder::new();
    decoder.feed(data).unwrap();
    assert!(decoder.decode().is_err());
}

#[test]
fn header_obs_fold_tab_error() {
    let data = b"GET / HTTP/1.1\r\n\tHeader: value\r\n\r\n";
    let mut decoder = RequestDecoder::new();
    decoder.feed(data).unwrap();
    assert!(decoder.decode().is_err());
}

#[test]
fn header_contains_cr_error() {
    let data = b"GET / HTTP/1.1\r\nHead\rer: value\r\n\r\n";
    let mut decoder = RequestDecoder::new();
    decoder.feed(data).unwrap();
    assert!(decoder.decode().is_err());
}

#[test]
fn header_contains_lf_error() {
    let data = b"GET / HTTP/1.1\r\nHead\ner: value\r\n\r\n";
    let mut decoder = RequestDecoder::new();
    decoder.feed(data).unwrap();
    assert!(decoder.decode().is_err());
}

#[test]
fn header_missing_colon_error() {
    let data = b"GET / HTTP/1.1\r\nHeader value\r\n\r\n";
    let mut decoder = RequestDecoder::new();
    decoder.feed(data).unwrap();
    assert!(decoder.decode().is_err());
}

#[test]
fn header_empty_name_error() {
    let data = b"GET / HTTP/1.1\r\n: value\r\n\r\n";
    let mut decoder = RequestDecoder::new();
    decoder.feed(data).unwrap();
    assert!(decoder.decode().is_err());
}

#[test]
fn header_name_with_space_error() {
    let data = b"GET / HTTP/1.1\r\nHead er: value\r\n\r\n";
    let mut decoder = RequestDecoder::new();
    decoder.feed(data).unwrap();
    assert!(decoder.decode().is_err());
}

#[test]
fn header_name_trailing_space_error() {
    let data = b"GET / HTTP/1.1\r\nHeader : value\r\n\r\n";
    let mut decoder = RequestDecoder::new();
    decoder.feed(data).unwrap();
    assert!(decoder.decode().is_err());
}

#[test]
fn header_invalid_name_char_error() {
    let data = b"GET / HTTP/1.1\r\nHead@er: value\r\n\r\n";
    let mut decoder = RequestDecoder::new();
    decoder.feed(data).unwrap();
    assert!(decoder.decode().is_err());
}

#[test]
fn valid_header_name_chars() {
    let valid_names = [
        "Accept",
        "Content-Type",
        "X-Custom-Header",
        "X_Custom_Header",
        "X.Custom.Header",
        "Header123",
        "X!Header",
        "X#Header",
        "X$Header",
        "X%Header",
        "X&Header",
        "X'Header",
        "X*Header",
        "X+Header",
        "X^Header",
        "X`Header",
        "X|Header",
        "X~Header",
    ];

    for name in valid_names {
        let data = format!("GET / HTTP/1.1\r\n{}: value\r\n\r\n", name);
        let mut decoder = RequestDecoder::new();
        decoder.feed(data.as_bytes()).unwrap();
        assert!(
            decoder.decode().is_ok(),
            "Header name '{}' should be valid",
            name
        );
    }
}

// ========================================
// Transfer-Encoding と Content-Length のエラー
// ========================================

#[test]
fn transfer_encoding_and_content_length_error() {
    let data = b"GET / HTTP/1.1\r\nTransfer-Encoding: chunked\r\nContent-Length: 10\r\n\r\n";
    let mut decoder = RequestDecoder::new();
    decoder.feed(data).unwrap();
    assert!(decoder.decode().is_err());
}

#[test]
fn transfer_encoding_unsupported_error() {
    let data = b"GET / HTTP/1.1\r\nTransfer-Encoding: gzip\r\n\r\n";
    let mut decoder = RequestDecoder::new();
    decoder.feed(data).unwrap();
    assert!(decoder.decode().is_err());
}

#[test]
fn transfer_encoding_empty_token_error() {
    let data = b"GET / HTTP/1.1\r\nTransfer-Encoding: chunked,,chunked\r\n\r\n";
    let mut decoder = RequestDecoder::new();
    decoder.feed(data).unwrap();
    assert!(decoder.decode().is_err());
}

#[test]
fn transfer_encoding_empty_value_error() {
    let data = b"GET / HTTP/1.1\r\nTransfer-Encoding: \r\n\r\n";
    let mut decoder = RequestDecoder::new();
    decoder.feed(data).unwrap();
    assert!(decoder.decode().is_err());
}

#[test]
fn transfer_encoding_case_insensitive() {
    let data = b"HTTP/1.1 200 OK\r\ntransfer-encoding: CHUNKED\r\n\r\n5\r\nhello\r\n0\r\n\r\n";
    let mut decoder = ResponseDecoder::new();
    decoder.feed(data).unwrap();
    let response = decoder.decode().unwrap().unwrap();
    assert_eq!(response.body, b"hello");
}

// ========================================
// Content-Length のエラー
// ========================================

#[test]
fn content_length_not_number_error() {
    let data = b"GET / HTTP/1.1\r\nContent-Length: abc\r\n\r\n";
    let mut decoder = RequestDecoder::new();
    decoder.feed(data).unwrap();
    assert!(decoder.decode().is_err());
}

#[test]
fn content_length_empty_error() {
    let data = b"GET / HTTP/1.1\r\nContent-Length: \r\n\r\n";
    let mut decoder = RequestDecoder::new();
    decoder.feed(data).unwrap();
    assert!(decoder.decode().is_err());
}

#[test]
fn content_length_mismatch_error() {
    let data = b"GET / HTTP/1.1\r\nContent-Length: 10\r\nContent-Length: 20\r\n\r\n";
    let mut decoder = RequestDecoder::new();
    decoder.feed(data).unwrap();
    assert!(decoder.decode().is_err());
}

#[test]
fn content_length_match_ok() {
    let data = b"GET / HTTP/1.1\r\nContent-Length: 5\r\nContent-Length: 5\r\n\r\nhello";
    let mut decoder = RequestDecoder::new();
    decoder.feed(data).unwrap();
    let request = decoder.decode().unwrap().unwrap();
    assert_eq!(request.body, b"hello");
}

#[test]
fn content_length_zero_no_body() {
    let data = b"GET / HTTP/1.1\r\nContent-Length: 0\r\n\r\n";
    let mut decoder = RequestDecoder::new();
    decoder.feed(data).unwrap();
    let request = decoder.decode().unwrap().unwrap();
    assert!(request.body.is_empty());
}

#[test]
fn content_length_case_insensitive() {
    let data = b"GET / HTTP/1.1\r\ncontent-length: 5\r\n\r\nhello";
    let mut decoder = RequestDecoder::new();
    decoder.feed(data).unwrap();
    let request = decoder.decode().unwrap().unwrap();
    assert_eq!(request.body, b"hello");
}

// ========================================
// リクエスト行のエラー
// ========================================

#[test]
fn request_line_missing_parts_error() {
    let data = b"GET /\r\n\r\n";
    let mut decoder = RequestDecoder::new();
    decoder.feed(data).unwrap();
    assert!(decoder.decode().is_err());
}

#[test]
fn request_line_empty_error() {
    let data = b"\r\nHost: example.com\r\n\r\n";
    let mut decoder = RequestDecoder::new();
    decoder.feed(data).unwrap();
    assert!(decoder.decode().is_err());
}

// ========================================
// ステータス行のエラー (ResponseDecoder)
// ========================================

#[test]
fn status_line_missing_parts_error() {
    let data = b"HTTP/1.1\r\n\r\n";
    let mut decoder = ResponseDecoder::new();
    decoder.feed(data).unwrap();
    assert!(decoder.decode().is_err());
}

#[test]
fn status_code_invalid_error() {
    let data = b"HTTP/1.1 abc OK\r\n\r\n";
    let mut decoder = ResponseDecoder::new();
    decoder.feed(data).unwrap();
    assert!(decoder.decode().is_err());
}

#[test]
fn status_line_no_reason_phrase_ok() {
    let data = b"HTTP/1.1 200\r\n\r\n";
    let mut decoder = ResponseDecoder::new();
    decoder.feed(data).unwrap();
    let response = decoder.decode().unwrap().unwrap();
    assert_eq!(response.status_code, 200);
    assert_eq!(response.reason_phrase, "");
}

// ========================================
// HEAD リクエストへのレスポンス
// ========================================

#[test]
fn head_response_with_content_length() {
    let data = b"HTTP/1.1 200 OK\r\nContent-Length: 1000\r\n\r\n";
    let mut decoder = ResponseDecoder::new();
    decoder.set_expect_no_body(true);
    decoder.feed(data).unwrap();
    let response = decoder.decode().unwrap().unwrap();
    assert!(response.body.is_empty());
}

#[test]
fn head_response_with_transfer_encoding() {
    let data = b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n";
    let mut decoder = ResponseDecoder::new();
    decoder.set_expect_no_body(true);
    decoder.feed(data).unwrap();
    let response = decoder.decode().unwrap().unwrap();
    assert!(response.body.is_empty());
}

// ========================================
// ボディなしステータスコード
// ========================================

proptest! {
    #[test]
    fn status_1xx_no_body(code in 100u16..200) {
        let status_line = format!("HTTP/1.1 {} Continue\r\nContent-Length: 100\r\n\r\n", code);
        let mut decoder = ResponseDecoder::new();
        decoder.feed(status_line.as_bytes()).unwrap();
        let response = decoder.decode().unwrap().unwrap();
        prop_assert!(response.body.is_empty());
    }
}

#[test]
fn status_204_no_body() {
    let data = b"HTTP/1.1 204 No Content\r\nContent-Length: 100\r\n\r\n";
    let mut decoder = ResponseDecoder::new();
    decoder.feed(data).unwrap();
    let response = decoder.decode().unwrap().unwrap();
    assert!(response.body.is_empty());
}

#[test]
fn status_304_no_body() {
    let data = b"HTTP/1.1 304 Not Modified\r\nContent-Length: 100\r\n\r\n";
    let mut decoder = ResponseDecoder::new();
    decoder.feed(data).unwrap();
    let response = decoder.decode().unwrap().unwrap();
    assert!(response.body.is_empty());
}

// ========================================
// チャンクエンコーディング
// ========================================

#[test]
fn chunked_invalid_size_error() {
    let data = b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\nXYZ\r\n";
    let mut decoder = ResponseDecoder::new();
    decoder.feed(data).unwrap();
    assert!(decoder.decode().is_err());
}

#[test]
fn chunked_size_with_extension_ok() {
    let data =
        b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n5;ext=val\r\nhello\r\n0\r\n\r\n";
    let mut decoder = ResponseDecoder::new();
    decoder.feed(data).unwrap();
    let response = decoder.decode().unwrap().unwrap();
    assert_eq!(response.body, b"hello");
}

#[test]
fn chunked_with_trailer_ok() {
    let data = b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n5\r\nhello\r\n0\r\nX-Trailer: value\r\n\r\n";
    let mut decoder = ResponseDecoder::new();
    decoder.feed(data).unwrap();
    let response = decoder.decode().unwrap().unwrap();
    assert_eq!(response.body, b"hello");
}

#[test]
fn chunked_with_multiple_trailers_ok() {
    let data = b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n5\r\nhello\r\n0\r\nX-Trailer1: value1\r\nX-Trailer2: value2\r\n\r\n";
    let mut decoder = ResponseDecoder::new();
    decoder.feed(data).unwrap();
    let response = decoder.decode().unwrap().unwrap();
    assert_eq!(response.body, b"hello");
}

proptest! {
    #[test]
    fn chunked_multiple_chunks(
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
        let response = decoder.decode().unwrap().unwrap();
        let expected: Vec<u8> = [chunk1, chunk2, chunk3].concat();
        prop_assert_eq!(response.body, expected);
    }
}

proptest! {
    #[test]
    fn chunked_roundtrip(chunks in proptest::collection::vec(body(), 1..5)) {
        let non_empty_chunks: Vec<Vec<u8>> = chunks.into_iter().filter(|c| !c.is_empty()).collect();
        let chunk_refs: Vec<&[u8]> = non_empty_chunks.iter().map(|c| c.as_slice()).collect();
        let encoded = encode_chunks(&chunk_refs);

        let mut decoder = ResponseDecoder::new();
        let header = b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n";
        decoder.feed(header).unwrap();
        decoder.feed(&encoded).unwrap();

        let response = decoder.decode().unwrap().unwrap();
        let expected: Vec<u8> = non_empty_chunks.iter().flatten().copied().collect();
        prop_assert_eq!(&response.body, &expected);
    }
}

proptest! {
    #[test]
    fn encode_chunk_valid(data in body()) {
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
// UTF-8 エラー
// ========================================

#[test]
fn invalid_utf8_request_line_error() {
    let data = b"GET /\xff HTTP/1.1\r\n\r\n";
    let mut decoder = RequestDecoder::new();
    decoder.feed(data).unwrap();
    assert!(decoder.decode().is_err());
}

#[test]
fn invalid_utf8_header_error() {
    let data = b"GET / HTTP/1.1\r\nX-Header: \xff\r\n\r\n";
    let mut decoder = RequestDecoder::new();
    decoder.feed(data).unwrap();
    assert!(decoder.decode().is_err());
}

#[test]
fn invalid_utf8_chunk_size_error() {
    let data = b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n\xff\r\n";
    let mut decoder = ResponseDecoder::new();
    decoder.feed(data).unwrap();
    assert!(decoder.decode().is_err());
}

// ========================================
// 部分的なデータ (None を返す)
// ========================================

#[test]
fn incomplete_request_line() {
    let mut decoder = RequestDecoder::new();
    decoder.feed(b"GET / HTTP/1.1").unwrap();
    assert!(decoder.decode().unwrap().is_none());
}

#[test]
fn incomplete_headers() {
    let mut decoder = RequestDecoder::new();
    decoder
        .feed(b"GET / HTTP/1.1\r\nHost: example.com")
        .unwrap();
    assert!(decoder.decode().unwrap().is_none());
}

#[test]
fn incomplete_body() {
    let mut decoder = RequestDecoder::new();
    decoder
        .feed(b"GET / HTTP/1.1\r\nContent-Length: 10\r\n\r\nhello")
        .unwrap();
    assert!(decoder.decode().unwrap().is_none());
}

#[test]
fn incomplete_chunk_size() {
    let mut decoder = ResponseDecoder::new();
    decoder
        .feed(b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n5")
        .unwrap();
    assert!(decoder.decode().unwrap().is_none());
}

#[test]
fn incomplete_chunk_data() {
    let mut decoder = ResponseDecoder::new();
    decoder
        .feed(b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n5\r\nhel")
        .unwrap();
    assert!(decoder.decode().unwrap().is_none());
}

#[test]
fn incomplete_trailer() {
    let mut decoder = ResponseDecoder::new();
    decoder
        .feed(b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n5\r\nhello\r\n0\r\nX-Trailer")
        .unwrap();
    assert!(decoder.decode().unwrap().is_none());
}

// ========================================
// デコーダーの状態管理
// ========================================

#[test]
fn decoder_remaining() {
    let mut decoder = RequestDecoder::new();
    decoder.feed(b"GET / HTTP/1.1\r\n\r\nextra").unwrap();
    let _ = decoder.decode().unwrap();
    assert_eq!(decoder.remaining(), b"extra");
}

#[test]
fn decoder_reset() {
    let mut decoder = RequestDecoder::new();
    decoder.feed(b"GET / HTTP/1.1\r\n").unwrap();
    decoder.reset();
    assert!(decoder.remaining().is_empty());
}

#[test]
fn decoder_with_limits() {
    let limits = DecoderLimits {
        max_buffer_size: 1024,
        max_headers_count: 10,
        max_header_line_size: 256,
        max_body_size: 512,
    };

    let decoder = RequestDecoder::with_limits(limits.clone());
    assert_eq!(decoder.limits().max_buffer_size, 1024);

    let decoder2 = ResponseDecoder::with_limits(limits);
    assert_eq!(decoder2.limits().max_body_size, 512);
}

#[test]
fn decoder_default() {
    let decoder: RequestDecoder = Default::default();
    assert!(decoder.remaining().is_empty());

    let decoder2: ResponseDecoder = Default::default();
    assert!(decoder2.remaining().is_empty());
}

// ========================================
// 制限テスト (RequestDecoder)
// ========================================

proptest! {
    #[test]
    fn request_decoder_buffer_overflow(max_buffer_size in 100usize..10000) {
        let limits = DecoderLimits {
            max_buffer_size,
            ..DecoderLimits::default()
        };
        let mut decoder = RequestDecoder::with_limits(limits);

        let data = vec![b'a'; max_buffer_size];
        prop_assert!(decoder.feed(&data).is_ok());

        decoder.reset();
        let data = vec![b'a'; max_buffer_size + 1];
        prop_assert!(decoder.feed(&data).is_err());
    }
}

proptest! {
    #[test]
    fn request_decoder_too_many_headers(max_headers_count in 1usize..50) {
        let limits = DecoderLimits {
            max_headers_count,
            max_buffer_size: 1024 * 1024,
            ..DecoderLimits::default()
        };
        let mut decoder = RequestDecoder::with_limits(limits);

        let mut request = String::from("GET / HTTP/1.1\r\n");
        for i in 0..=max_headers_count {
            request.push_str(&format!("X-Header-{}: value\r\n", i));
        }
        request.push_str("\r\n");

        decoder.feed(request.as_bytes()).unwrap();
        prop_assert!(decoder.decode().is_err());
    }
}

proptest! {
    #[test]
    fn request_decoder_header_line_too_long(max_header_line_size in 50usize..500) {
        let limits = DecoderLimits {
            max_header_line_size,
            max_buffer_size: 1024 * 1024,
            ..DecoderLimits::default()
        };
        let mut decoder = RequestDecoder::with_limits(limits);

        let long_value: String = "x".repeat(max_header_line_size + 1);
        let request = format!("GET / HTTP/1.1\r\nX-Long-Header: {}\r\n\r\n", long_value);

        decoder.feed(request.as_bytes()).unwrap();
        prop_assert!(decoder.decode().is_err());
    }
}

proptest! {
    #[test]
    fn request_decoder_body_too_large_content_length(max_body_size in 100usize..10000) {
        let limits = DecoderLimits {
            max_body_size,
            max_buffer_size: 1024 * 1024,
            ..DecoderLimits::default()
        };
        let mut decoder = RequestDecoder::with_limits(limits);

        let body_size = max_body_size + 1;
        let request = format!("POST / HTTP/1.1\r\nContent-Length: {}\r\n\r\n", body_size);

        decoder.feed(request.as_bytes()).unwrap();
        prop_assert!(decoder.decode().is_err());
    }
}

proptest! {
    #[test]
    fn request_decoder_body_too_large_chunked(max_body_size in 100usize..10000) {
        let limits = DecoderLimits {
            max_body_size,
            max_buffer_size: 1024 * 1024,
            ..DecoderLimits::default()
        };
        let mut decoder = RequestDecoder::with_limits(limits);

        let chunk_size = max_body_size + 1;
        let request = format!("POST / HTTP/1.1\r\nTransfer-Encoding: chunked\r\n\r\n{:x}\r\n", chunk_size);

        decoder.feed(request.as_bytes()).unwrap();
        prop_assert!(decoder.decode().is_err());
    }
}

proptest! {
    #[test]
    fn request_decoder_limits_getter(
        max_buffer_size in 1usize..1_000_000,
        max_headers_count in 1usize..1000,
        max_header_line_size in 1usize..100_000,
        max_body_size in 1usize..100_000_000
    ) {
        let limits = DecoderLimits {
            max_buffer_size,
            max_headers_count,
            max_header_line_size,
            max_body_size,
        };
        let decoder = RequestDecoder::with_limits(limits.clone());
        prop_assert_eq!(decoder.limits(), &limits);
    }
}

proptest! {
    #[test]
    fn request_decoder_exact_buffer_limit(max_buffer_size in 100usize..10000) {
        let limits = DecoderLimits {
            max_buffer_size,
            ..DecoderLimits::default()
        };
        let mut decoder = RequestDecoder::with_limits(limits);

        let data = vec![b'a'; max_buffer_size];
        prop_assert!(decoder.feed(&data).is_ok());
    }
}

proptest! {
    #[test]
    fn request_decoder_exact_header_count(max_headers_count in 1usize..20) {
        let limits = DecoderLimits {
            max_headers_count,
            max_buffer_size: 1024 * 1024,
            ..DecoderLimits::default()
        };
        let mut decoder = RequestDecoder::with_limits(limits);

        let mut request = String::from("GET / HTTP/1.1\r\n");
        for i in 0..max_headers_count {
            request.push_str(&format!("X-Header-{}: value\r\n", i));
        }
        request.push_str("\r\n");

        decoder.feed(request.as_bytes()).unwrap();
        prop_assert!(decoder.decode().is_ok());
    }
}

proptest! {
    #[test]
    fn request_decoder_exact_body_size(max_body_size in 100usize..10000) {
        let limits = DecoderLimits {
            max_body_size,
            max_buffer_size: 1024 * 1024,
            ..DecoderLimits::default()
        };
        let mut decoder = RequestDecoder::with_limits(limits);

        let body = vec![b'x'; max_body_size];
        let request = format!("POST / HTTP/1.1\r\nContent-Length: {}\r\n\r\n", max_body_size);

        decoder.feed(request.as_bytes()).unwrap();
        decoder.feed(&body).unwrap();
        prop_assert!(decoder.decode().is_ok());
    }
}

proptest! {
    #[test]
    fn request_decoder_feed_unchecked_no_limit(max_buffer_size in 100usize..1000) {
        let limits = DecoderLimits {
            max_buffer_size,
            ..DecoderLimits::default()
        };
        let mut decoder = RequestDecoder::with_limits(limits);

        let data = vec![b'a'; max_buffer_size * 2];
        decoder.feed_unchecked(&data);
        prop_assert_eq!(decoder.remaining().len(), max_buffer_size * 2);
    }
}

// ========================================
// 制限テスト (ResponseDecoder)
// ========================================

proptest! {
    #[test]
    fn response_decoder_buffer_overflow(max_buffer_size in 100usize..10000) {
        let limits = DecoderLimits {
            max_buffer_size,
            ..DecoderLimits::default()
        };
        let mut decoder = ResponseDecoder::with_limits(limits);

        let data = vec![b'a'; max_buffer_size];
        prop_assert!(decoder.feed(&data).is_ok());

        decoder.reset();
        let data = vec![b'a'; max_buffer_size + 1];
        prop_assert!(decoder.feed(&data).is_err());
    }
}

proptest! {
    #[test]
    fn response_decoder_too_many_headers(max_headers_count in 1usize..50) {
        let limits = DecoderLimits {
            max_headers_count,
            max_buffer_size: 1024 * 1024,
            ..DecoderLimits::default()
        };
        let mut decoder = ResponseDecoder::with_limits(limits);

        let mut response = String::from("HTTP/1.1 200 OK\r\n");
        for i in 0..=max_headers_count {
            response.push_str(&format!("X-Header-{}: value\r\n", i));
        }
        response.push_str("\r\n");

        decoder.feed(response.as_bytes()).unwrap();
        prop_assert!(decoder.decode().is_err());
    }
}

proptest! {
    #[test]
    fn response_decoder_header_line_too_long(max_header_line_size in 50usize..500) {
        let limits = DecoderLimits {
            max_header_line_size,
            max_buffer_size: 1024 * 1024,
            ..DecoderLimits::default()
        };
        let mut decoder = ResponseDecoder::with_limits(limits);

        let long_value: String = "x".repeat(max_header_line_size + 1);
        let response = format!("HTTP/1.1 200 OK\r\nX-Long-Header: {}\r\n\r\n", long_value);

        decoder.feed(response.as_bytes()).unwrap();
        prop_assert!(decoder.decode().is_err());
    }
}

proptest! {
    #[test]
    fn response_decoder_body_too_large_content_length(max_body_size in 100usize..10000) {
        let limits = DecoderLimits {
            max_body_size,
            max_buffer_size: 1024 * 1024,
            ..DecoderLimits::default()
        };
        let mut decoder = ResponseDecoder::with_limits(limits);

        let body_size = max_body_size + 1;
        let response = format!("HTTP/1.1 200 OK\r\nContent-Length: {}\r\n\r\n", body_size);

        decoder.feed(response.as_bytes()).unwrap();
        prop_assert!(decoder.decode().is_err());
    }
}

proptest! {
    #[test]
    fn response_decoder_body_too_large_chunked(max_body_size in 100usize..10000) {
        let limits = DecoderLimits {
            max_body_size,
            max_buffer_size: 1024 * 1024,
            ..DecoderLimits::default()
        };
        let mut decoder = ResponseDecoder::with_limits(limits);

        let chunk_size = max_body_size + 1;
        let response = format!("HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n{:x}\r\n", chunk_size);

        decoder.feed(response.as_bytes()).unwrap();
        prop_assert!(decoder.decode().is_err());
    }
}

proptest! {
    #[test]
    fn response_decoder_limits_getter(
        max_buffer_size in 1usize..1_000_000,
        max_headers_count in 1usize..1000,
        max_header_line_size in 1usize..100_000,
        max_body_size in 1usize..100_000_000
    ) {
        let limits = DecoderLimits {
            max_buffer_size,
            max_headers_count,
            max_header_line_size,
            max_body_size,
        };
        let decoder = ResponseDecoder::with_limits(limits.clone());
        prop_assert_eq!(decoder.limits(), &limits);
    }
}

proptest! {
    #[test]
    fn response_decoder_feed_unchecked_no_limit(max_buffer_size in 100usize..1000) {
        let limits = DecoderLimits {
            max_buffer_size,
            ..DecoderLimits::default()
        };
        let mut decoder = ResponseDecoder::with_limits(limits);

        let data = vec![b'a'; max_buffer_size * 2];
        decoder.feed_unchecked(&data);
        prop_assert_eq!(decoder.remaining().len(), max_buffer_size * 2);
    }
}

// ========================================
// ラウンドトリップテスト
// ========================================

proptest! {
    #[test]
    fn request_roundtrip(
        method in http_method(),
        uri in http_uri(),
        hdrs in headers(),
        body_data in body()
    ) {
        let mut request = Request::new(&method, &uri);
        for (name, value) in &hdrs {
            request.add_header(name, value);
        }
        if !body_data.is_empty() {
            request.body = body_data.clone();
        }

        let encoded = request.encode();

        let mut decoder = RequestDecoder::new();
        decoder.feed(&encoded).unwrap();
        let decoded = decoder.decode().unwrap().unwrap();

        prop_assert_eq!(&decoded.method, &method);
        prop_assert_eq!(&decoded.uri, &uri);
        prop_assert_eq!(&decoded.body, &request.body);
    }
}

proptest! {
    #[test]
    fn response_roundtrip(
        code in status_code(),
        phrase in reason_phrase(),
        hdrs in headers(),
        body_data in body()
    ) {
        let mut response = Response::new(code, &phrase);
        for (name, value) in &hdrs {
            response.add_header(name, value);
        }

        let has_body = !((100..200).contains(&code) || code == 204 || code == 304);
        if has_body && !body_data.is_empty() {
            response.body = body_data.clone();
        }

        let encoded = response.encode();

        let mut decoder = ResponseDecoder::new();
        decoder.feed(&encoded).unwrap();
        let decoded = decoder.decode().unwrap().unwrap();

        prop_assert_eq!(decoded.status_code, code);
        prop_assert_eq!(&decoded.reason_phrase, &phrase);

        if has_body {
            prop_assert_eq!(&decoded.body, &response.body);
        }
    }
}

proptest! {
    #[test]
    fn streaming_decode_request(method in http_method(), uri in http_uri(), hdrs in headers()) {
        let mut request = Request::new(&method, &uri);
        for (name, value) in &hdrs {
            request.add_header(name, value);
        }

        let encoded = request.encode();

        let mut decoder = RequestDecoder::new();
        for byte in &encoded {
            decoder.feed(std::slice::from_ref(byte)).unwrap();
        }
        let decoded = decoder.decode().unwrap().unwrap();

        prop_assert_eq!(&decoded.method, &method);
        prop_assert_eq!(&decoded.uri, &uri);
    }
}

proptest! {
    #[test]
    fn streaming_decode_response(code in status_code(), phrase in reason_phrase()) {
        let response = Response::new(code, &phrase);
        let encoded = response.encode();

        let mut decoder = ResponseDecoder::new();
        for byte in &encoded {
            decoder.feed(std::slice::from_ref(byte)).unwrap();
        }
        let decoded = decoder.decode().unwrap().unwrap();

        prop_assert_eq!(decoded.status_code, code);
    }
}

proptest! {
    #[test]
    fn streaming_decode_request_with_body(
        method in http_method(),
        uri in http_uri(),
        body_data in proptest::collection::vec(any::<u8>(), 1..128)
    ) {
        let mut request = Request::new(&method, &uri);
        request.body = body_data.clone();
        let encoded = request.encode();

        let mut decoder = RequestDecoder::new();
        let mut decoded = None;
        for chunk in encoded.chunks(7) {
            decoder.feed(chunk).unwrap();
            if let Ok(Some(req)) = decoder.decode() {
                decoded = Some(req);
                break;
            }
        }
        let decoded = decoded.expect("should decode");

        prop_assert_eq!(&decoded.method, &method);
        prop_assert_eq!(&decoded.body, &body_data);
    }
}

proptest! {
    #[test]
    fn multiple_requests_same_decoder(
        methods in proptest::collection::vec(http_method(), 2..5),
        uris in proptest::collection::vec(http_uri(), 2..5)
    ) {
        let count = methods.len().min(uris.len());
        let mut decoder = RequestDecoder::new();

        for i in 0..count {
            let request = Request::new(&methods[i], &uris[i]);
            let encoded = request.encode();
            decoder.feed(&encoded).unwrap();
            let decoded = decoder.decode().unwrap().unwrap();

            prop_assert_eq!(&decoded.method, &methods[i]);
            prop_assert_eq!(&decoded.uri, &uris[i]);
        }
    }
}

proptest! {
    #[test]
    fn multiple_responses_same_decoder(codes in proptest::collection::vec(status_code(), 2..5)) {
        let mut decoder = ResponseDecoder::new();

        for code in &codes {
            if (100..200).contains(code) || *code == 204 || *code == 304 {
                continue;
            }

            let response = Response::new(*code, "OK");
            let encoded = response.encode();
            decoder.feed(&encoded).unwrap();
            let decoded = decoder.decode().unwrap().unwrap();

            prop_assert_eq!(decoded.status_code, *code);
        }
    }
}

proptest! {
    #[test]
    fn decoder_reuse_after_error(
        garbage in proptest::collection::vec(any::<u8>(), 1..64),
        method in http_method(),
        uri in http_uri()
    ) {
        let mut decoder = RequestDecoder::new();

        let _ = decoder.feed(&garbage);
        let _ = decoder.decode();

        decoder.reset();
        let request = Request::new(&method, &uri);
        let encoded = request.encode();
        decoder.feed(&encoded).unwrap();
        let decoded = decoder.decode().unwrap().unwrap();

        prop_assert_eq!(&decoded.method, &method);
        prop_assert_eq!(&decoded.uri, &uri);
    }
}

// ========================================
// no_panic テスト
// ========================================

proptest! {
    #[test]
    fn request_decoder_parse_no_panic(data in proptest::collection::vec(any::<u8>(), 0..256)) {
        let mut decoder = RequestDecoder::new();
        let _ = decoder.feed_unchecked(&data);
        let _ = decoder.decode();
    }
}

proptest! {
    #[test]
    fn response_decoder_parse_no_panic(data in proptest::collection::vec(any::<u8>(), 0..256)) {
        let mut decoder = ResponseDecoder::new();
        let _ = decoder.feed_unchecked(&data);
        let _ = decoder.decode();
    }
}

// ========================================
// Content-Length オーバーフローテスト
// ========================================

#[test]
fn content_length_overflow_error() {
    // usize::MAX を超える値
    let data = b"GET / HTTP/1.1\r\nContent-Length: 99999999999999999999999999999\r\n\r\n";
    let mut decoder = RequestDecoder::new();
    decoder.feed(data).unwrap();
    assert!(decoder.decode().is_err());
}

#[test]
fn content_length_negative_like_error() {
    let data = b"GET / HTTP/1.1\r\nContent-Length: -1\r\n\r\n";
    let mut decoder = RequestDecoder::new();
    decoder.feed(data).unwrap();
    assert!(decoder.decode().is_err());
}

#[test]
fn content_length_with_spaces_error() {
    let data = b"GET / HTTP/1.1\r\nContent-Length: 1 2\r\n\r\n";
    let mut decoder = RequestDecoder::new();
    decoder.feed(data).unwrap();
    assert!(decoder.decode().is_err());
}

// ========================================
// ResponseDecoder remaining と reset のテスト
// ========================================

#[test]
fn response_decoder_remaining() {
    let mut decoder = ResponseDecoder::new();
    decoder.feed(b"HTTP/1.1 200 OK\r\n\r\nextra").unwrap();
    let _ = decoder.decode().unwrap();
    assert_eq!(decoder.remaining(), b"extra");
}

#[test]
fn response_decoder_reset() {
    let mut decoder = ResponseDecoder::new();
    decoder.feed(b"HTTP/1.1 200 OK\r\n").unwrap();
    decoder.reset();
    assert!(decoder.remaining().is_empty());
}

#[test]
fn response_decoder_reset_expect_no_body() {
    let mut decoder = ResponseDecoder::new();
    decoder.set_expect_no_body(true);
    decoder.reset();
    // reset 後は expect_no_body もリセットされる
    decoder
        .feed(b"HTTP/1.1 200 OK\r\nContent-Length: 5\r\n\r\nhello")
        .unwrap();
    let response = decoder.decode().unwrap().unwrap();
    assert_eq!(response.body, b"hello");
}

// ========================================
// Chunked エンコーディングの追加テスト
// ========================================

#[test]
fn chunked_request_body() {
    let data = b"POST / HTTP/1.1\r\nTransfer-Encoding: chunked\r\n\r\n5\r\nhello\r\n0\r\n\r\n";
    let mut decoder = RequestDecoder::new();
    decoder.feed(data).unwrap();
    let request = decoder.decode().unwrap().unwrap();
    assert_eq!(request.body, b"hello");
}

#[test]
fn chunked_request_with_trailer() {
    let data = b"POST / HTTP/1.1\r\nTransfer-Encoding: chunked\r\n\r\n5\r\nhello\r\n0\r\nX-Checksum: abc\r\n\r\n";
    let mut decoder = RequestDecoder::new();
    decoder.feed(data).unwrap();
    let request = decoder.decode().unwrap().unwrap();
    assert_eq!(request.body, b"hello");
}

#[test]
fn chunked_request_with_multiple_trailers() {
    let data = b"POST / HTTP/1.1\r\nTransfer-Encoding: chunked\r\n\r\n5\r\nhello\r\n0\r\nX-A: 1\r\nX-B: 2\r\n\r\n";
    let mut decoder = RequestDecoder::new();
    decoder.feed(data).unwrap();
    let request = decoder.decode().unwrap().unwrap();
    assert_eq!(request.body, b"hello");
}

// ========================================
// 空のボディのテスト
// ========================================

#[test]
fn response_content_length_zero() {
    let data = b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\n\r\n";
    let mut decoder = ResponseDecoder::new();
    decoder.feed(data).unwrap();
    let response = decoder.decode().unwrap().unwrap();
    assert!(response.body.is_empty());
}

#[test]
fn response_no_content_length_no_transfer_encoding() {
    let data = b"HTTP/1.1 200 OK\r\n\r\n";
    let mut decoder = ResponseDecoder::new();
    decoder.feed(data).unwrap();
    let response = decoder.decode().unwrap().unwrap();
    assert!(response.body.is_empty());
}

// ========================================
// ステータスコード境界テスト
// ========================================

#[test]
fn status_code_boundary_199() {
    let data = b"HTTP/1.1 199 Info\r\nContent-Length: 100\r\n\r\n";
    let mut decoder = ResponseDecoder::new();
    decoder.feed(data).unwrap();
    let response = decoder.decode().unwrap().unwrap();
    // 1xx はボディなし
    assert!(response.body.is_empty());
}

#[test]
fn status_code_boundary_200() {
    let data = b"HTTP/1.1 200 OK\r\nContent-Length: 5\r\n\r\nhello";
    let mut decoder = ResponseDecoder::new();
    decoder.feed(data).unwrap();
    let response = decoder.decode().unwrap().unwrap();
    assert_eq!(response.body, b"hello");
}

#[test]
fn status_code_boundary_203() {
    let data = b"HTTP/1.1 203 Non-Authoritative\r\nContent-Length: 5\r\n\r\nhello";
    let mut decoder = ResponseDecoder::new();
    decoder.feed(data).unwrap();
    let response = decoder.decode().unwrap().unwrap();
    assert_eq!(response.body, b"hello");
}

// ========================================
// 複数の Transfer-Encoding ヘッダーのテスト
// ========================================

#[test]
fn multiple_transfer_encoding_chunked_ok() {
    let data = b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\nTransfer-Encoding: chunked\r\n\r\n5\r\nhello\r\n0\r\n\r\n";
    let mut decoder = ResponseDecoder::new();
    decoder.feed(data).unwrap();
    let response = decoder.decode().unwrap().unwrap();
    assert_eq!(response.body, b"hello");
}

// ========================================
// ヘッダー値の空白トリムテスト
// ========================================

#[test]
fn header_value_leading_trailing_spaces() {
    let data = b"GET / HTTP/1.1\r\nContent-Length:   5   \r\n\r\nhello";
    let mut decoder = RequestDecoder::new();
    decoder.feed(data).unwrap();
    let request = decoder.decode().unwrap().unwrap();
    assert_eq!(request.body, b"hello");
}

// ========================================
// リクエストの incomplete チャンクテスト
// ========================================

#[test]
fn request_incomplete_chunk_size() {
    let mut decoder = RequestDecoder::new();
    decoder
        .feed(b"POST / HTTP/1.1\r\nTransfer-Encoding: chunked\r\n\r\n5")
        .unwrap();
    assert!(decoder.decode().unwrap().is_none());
}

#[test]
fn request_incomplete_chunk_data() {
    let mut decoder = RequestDecoder::new();
    decoder
        .feed(b"POST / HTTP/1.1\r\nTransfer-Encoding: chunked\r\n\r\n5\r\nhel")
        .unwrap();
    assert!(decoder.decode().unwrap().is_none());
}

#[test]
fn request_incomplete_trailer() {
    let mut decoder = RequestDecoder::new();
    decoder
        .feed(b"POST / HTTP/1.1\r\nTransfer-Encoding: chunked\r\n\r\n5\r\nhello\r\n0\r\nX-Trailer")
        .unwrap();
    assert!(decoder.decode().unwrap().is_none());
}

// ========================================
// UTF-8 エラーの追加テスト
// ========================================

#[test]
fn invalid_utf8_status_line_error() {
    let data = b"HTTP/1.1 200 \xff\r\n\r\n";
    let mut decoder = ResponseDecoder::new();
    decoder.feed(data).unwrap();
    assert!(decoder.decode().is_err());
}

#[test]
fn invalid_utf8_response_header_error() {
    let data = b"HTTP/1.1 200 OK\r\nX-Header: \xff\r\n\r\n";
    let mut decoder = ResponseDecoder::new();
    decoder.feed(data).unwrap();
    assert!(decoder.decode().is_err());
}
