//! Decoder のプロパティテスト (decoder.rs)

use proptest::prelude::*;
use shiguredo_http11::{
    BodyKind, BodyProgress, DecoderLimits, HttpHead, Request, RequestDecoder, Response,
    ResponseDecoder, ResponseHead, encode_chunk, encode_chunks,
};

// ========================================
// Strategy 定義
// ========================================

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

fn body() -> impl Strategy<Value = Vec<u8>> {
    proptest::collection::vec(any::<u8>(), 0..256)
}

/// 無効なヘッダー名の文字を生成する Strategy
/// 注: `:` はヘッダーの区切り文字として解釈されるため除外
fn invalid_header_name_char() -> impl Strategy<Value = char> {
    prop_oneof![
        Just('@'),
        Just('['),
        Just(']'),
        Just('\\'),
        Just('{'),
        Just('}'),
        Just('<'),
        Just('>'),
        Just('('),
        Just(')'),
        Just(','),
        Just(';'),
        Just('"'),
        Just('/'),
        Just('?'),
        Just('='),
    ]
}

/// 有効なヘッダー名の文字を生成する Strategy
fn valid_header_name_special_char() -> impl Strategy<Value = char> {
    prop_oneof![
        Just('!'),
        Just('#'),
        Just('$'),
        Just('%'),
        Just('&'),
        Just('\''),
        Just('*'),
        Just('+'),
        Just('^'),
        Just('`'),
        Just('|'),
        Just('~'),
        Just('-'),
        Just('_'),
        Just('.'),
    ]
}

/// Transfer-Encoding トークン生成 Strategy
fn transfer_encoding_token() -> impl Strategy<Value = String> {
    prop_oneof![
        Just("chunked".to_string()),
        Just("gzip".to_string()),
        Just("deflate".to_string()),
        Just("compress".to_string()),
        Just("identity".to_string()),
    ]
}

// ========================================
// ヘッダーパースエラーの PBT
// ========================================

proptest! {
    #[test]
    fn header_obs_fold_space_error(
        header_name in "[A-Za-z]{1,16}",
        header_value in "[A-Za-z0-9]{1,16}"
    ) {
        // obs-fold (行頭スペース) はエラー
        let data = format!("GET / HTTP/1.1\r\n {}: {}\r\n\r\n", header_name, header_value);
        let mut decoder = RequestDecoder::new();
        decoder.feed(data.as_bytes()).unwrap();
        prop_assert!(decoder.decode_headers().is_err());
    }
}

proptest! {
    #[test]
    fn header_obs_fold_tab_error(
        header_name in "[A-Za-z]{1,16}",
        header_value in "[A-Za-z0-9]{1,16}"
    ) {
        // obs-fold (行頭タブ) はエラー
        let data = format!("GET / HTTP/1.1\r\n\t{}: {}\r\n\r\n", header_name, header_value);
        let mut decoder = RequestDecoder::new();
        decoder.feed(data.as_bytes()).unwrap();
        prop_assert!(decoder.decode_headers().is_err());
    }
}

proptest! {
    #[test]
    fn header_contains_cr_error(
        prefix in "[A-Za-z]{1,8}",
        suffix in "[A-Za-z]{1,8}"
    ) {
        // ヘッダー名に CR を含むとエラー
        let data = format!("GET / HTTP/1.1\r\n{}\r{}: value\r\n\r\n", prefix, suffix);
        let mut decoder = RequestDecoder::new();
        decoder.feed(data.as_bytes()).unwrap();
        prop_assert!(decoder.decode_headers().is_err());
    }
}

proptest! {
    #[test]
    fn header_contains_lf_error(
        prefix in "[A-Za-z]{1,8}",
        suffix in "[A-Za-z]{1,8}"
    ) {
        // ヘッダー名に LF を含むとエラー
        let data = format!("GET / HTTP/1.1\r\n{}\n{}: value\r\n\r\n", prefix, suffix);
        let mut decoder = RequestDecoder::new();
        decoder.feed(data.as_bytes()).unwrap();
        prop_assert!(decoder.decode_headers().is_err());
    }
}

proptest! {
    #[test]
    fn header_missing_colon_error(
        header_name in "[A-Za-z]{1,16}",
        header_value in "[A-Za-z0-9]{1,16}"
    ) {
        // コロンがないとエラー
        let data = format!("GET / HTTP/1.1\r\n{} {}\r\n\r\n", header_name, header_value);
        let mut decoder = RequestDecoder::new();
        decoder.feed(data.as_bytes()).unwrap();
        prop_assert!(decoder.decode_headers().is_err());
    }
}

proptest! {
    #[test]
    fn header_empty_name_error(
        header_value in "[A-Za-z0-9]{1,16}"
    ) {
        // 空のヘッダー名はエラー
        let data = format!("GET / HTTP/1.1\r\n: {}\r\n\r\n", header_value);
        let mut decoder = RequestDecoder::new();
        decoder.feed(data.as_bytes()).unwrap();
        prop_assert!(decoder.decode_headers().is_err());
    }
}

proptest! {
    #[test]
    fn header_name_with_space_error(
        prefix in "[A-Za-z]{1,8}",
        suffix in "[A-Za-z]{1,8}"
    ) {
        // ヘッダー名にスペースを含むとエラー
        let data = format!("GET / HTTP/1.1\r\n{} {}: value\r\n\r\n", prefix, suffix);
        let mut decoder = RequestDecoder::new();
        decoder.feed(data.as_bytes()).unwrap();
        prop_assert!(decoder.decode_headers().is_err());
    }
}

proptest! {
    #[test]
    fn header_name_trailing_space_error(
        header_name in "[A-Za-z]{1,16}"
    ) {
        // ヘッダー名の後にスペースがあるとエラー
        let data = format!("GET / HTTP/1.1\r\n{} : value\r\n\r\n", header_name);
        let mut decoder = RequestDecoder::new();
        decoder.feed(data.as_bytes()).unwrap();
        prop_assert!(decoder.decode_headers().is_err());
    }
}

proptest! {
    #[test]
    fn header_invalid_name_char_error(
        prefix in "[A-Za-z]{1,8}",
        invalid_char in invalid_header_name_char(),
        suffix in "[A-Za-z]{1,8}"
    ) {
        // 無効な文字を含むヘッダー名はエラー
        let data = format!("GET / HTTP/1.1\r\n{}{}{}: value\r\n\r\n", prefix, invalid_char, suffix);
        let mut decoder = RequestDecoder::new();
        decoder.feed(data.as_bytes()).unwrap();
        prop_assert!(decoder.decode_headers().is_err());
    }
}

proptest! {
    #[test]
    fn valid_header_name_chars(
        prefix in "[A-Za-z]{1,8}",
        special_char in valid_header_name_special_char(),
        suffix in "[A-Za-z]{1,8}"
    ) {
        // 有効な特殊文字を含むヘッダー名は OK
        let header_name = format!("{}{}{}", prefix, special_char, suffix);
        let data = format!("GET / HTTP/1.1\r\n{}: value\r\n\r\n", header_name);
        let mut decoder = RequestDecoder::new();
        decoder.feed(data.as_bytes()).unwrap();
        prop_assert!(decoder.decode_headers().is_ok());
    }
}

proptest! {
    #[test]
    fn header_value_leading_trailing_spaces(
        header_name in "[A-Za-z]{1,16}",
        value in "[A-Za-z0-9]{1,16}",
        leading_spaces in 0..4usize,
        trailing_spaces in 0..4usize
    ) {
        // ヘッダー値の前後スペースは許可され、トリムされる
        let padded_value = format!(
            "{}{}{}",
            " ".repeat(leading_spaces),
            value,
            " ".repeat(trailing_spaces)
        );
        let data = format!("GET / HTTP/1.1\r\n{}:{}\r\n\r\n", header_name, padded_value);
        let mut decoder = RequestDecoder::new();
        decoder.feed(data.as_bytes()).unwrap();
        let result = decoder.decode_headers();
        prop_assert!(result.is_ok());
        if let Ok(Some((head, _))) = result {
            let header_value = head.get_header(&header_name).unwrap();
            prop_assert_eq!(header_value, value);
        }
    }
}

// ========================================
// Transfer-Encoding と Content-Length のエラー PBT
// ========================================

proptest! {
    #[test]
    fn transfer_encoding_and_content_length_error(
        content_length in 1..1000usize
    ) {
        // Transfer-Encoding と Content-Length の両方があるとエラー
        let data = format!(
            "GET / HTTP/1.1\r\nTransfer-Encoding: chunked\r\nContent-Length: {}\r\n\r\n",
            content_length
        );
        let mut decoder = RequestDecoder::new();
        decoder.feed(data.as_bytes()).unwrap();
        prop_assert!(decoder.decode_headers().is_err());
    }
}

proptest! {
    #[test]
    fn transfer_encoding_unsupported_error(
        coding in transfer_encoding_token().prop_filter("not chunked", |t| !t.eq_ignore_ascii_case("chunked"))
    ) {
        // chunked 以外の Transfer-Encoding はエラー
        let data = format!("GET / HTTP/1.1\r\nTransfer-Encoding: {}\r\n\r\n", coding);
        let mut decoder = RequestDecoder::new();
        decoder.feed(data.as_bytes()).unwrap();
        prop_assert!(decoder.decode_headers().is_err());
    }
}

proptest! {
    #[test]
    fn transfer_encoding_empty_token_error(
        before_comma in 0..3usize,
        after_comma in 0..3usize
    ) {
        // 空のトークン (連続カンマ) はエラー
        let data = format!(
            "GET / HTTP/1.1\r\nTransfer-Encoding: {},,{}\r\n\r\n",
            "chunked".repeat(before_comma.max(1)),
            "chunked".repeat(after_comma.max(1))
        );
        let mut decoder = RequestDecoder::new();
        decoder.feed(data.as_bytes()).unwrap();
        prop_assert!(decoder.decode_headers().is_err());
    }
}

proptest! {
    #[test]
    fn transfer_encoding_empty_value_error(
        method in http_method()
    ) {
        // 空の Transfer-Encoding はエラー
        let data = format!("{} / HTTP/1.1\r\nTransfer-Encoding: \r\n\r\n", method);
        let mut decoder = RequestDecoder::new();
        decoder.feed(data.as_bytes()).unwrap();
        prop_assert!(decoder.decode_headers().is_err());
    }
}

proptest! {
    #[test]
    fn transfer_encoding_case_insensitive(
        chunked_case in prop_oneof![
            Just("chunked"),
            Just("CHUNKED"),
            Just("Chunked"),
            Just("cHuNkEd"),
        ]
    ) {
        // Transfer-Encoding は大文字小文字を区別しない
        let data = format!(
            "HTTP/1.1 200 OK\r\ntransfer-encoding: {}\r\n\r\n5\r\nhello\r\n0\r\n\r\n",
            chunked_case
        );
        let mut decoder = ResponseDecoder::new();
        decoder.feed(data.as_bytes()).unwrap();
        let (head, body_kind) = decoder.decode_headers().unwrap().unwrap();
        prop_assert_eq!(head.status_code, 200);
        prop_assert_eq!(body_kind, BodyKind::Chunked);

        // ボディを読み取る
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
        prop_assert_eq!(body, b"hello");
    }
}

proptest! {
    #[test]
    fn multiple_transfer_encoding_chunked_ok(
        count in 1..3usize
    ) {
        // 複数の chunked ヘッダーは OK
        let headers = (0..count)
            .map(|_| "Transfer-Encoding: chunked")
            .collect::<Vec<_>>()
            .join("\r\n");
        let data = format!(
            "HTTP/1.1 200 OK\r\n{}\r\n\r\n5\r\nhello\r\n0\r\n\r\n",
            headers
        );
        let mut decoder = ResponseDecoder::new();
        decoder.feed(data.as_bytes()).unwrap();
        let (_, body_kind) = decoder.decode_headers().unwrap().unwrap();
        prop_assert_eq!(body_kind, BodyKind::Chunked);
    }
}

// ========================================
// Content-Length のエラー PBT
// ========================================

proptest! {
    #[test]
    fn content_length_not_number_error(
        invalid_value in "[a-zA-Z]{1,8}"
    ) {
        // 数字でない Content-Length はエラー
        let data = format!("GET / HTTP/1.1\r\nContent-Length: {}\r\n\r\n", invalid_value);
        let mut decoder = RequestDecoder::new();
        decoder.feed(data.as_bytes()).unwrap();
        prop_assert!(decoder.decode_headers().is_err());
    }
}

proptest! {
    #[test]
    fn content_length_empty_error(
        method in http_method()
    ) {
        // 空の Content-Length はエラー
        let data = format!("{} / HTTP/1.1\r\nContent-Length: \r\n\r\n", method);
        let mut decoder = RequestDecoder::new();
        decoder.feed(data.as_bytes()).unwrap();
        prop_assert!(decoder.decode_headers().is_err());
    }
}

proptest! {
    #[test]
    fn content_length_mismatch_error(
        len1 in 1..100usize,
        len2 in 101..200usize
    ) {
        // 異なる値の Content-Length はエラー
        let data = format!(
            "GET / HTTP/1.1\r\nContent-Length: {}\r\nContent-Length: {}\r\n\r\n",
            len1, len2
        );
        let mut decoder = RequestDecoder::new();
        decoder.feed(data.as_bytes()).unwrap();
        prop_assert!(decoder.decode_headers().is_err());
    }
}

proptest! {
    #[test]
    fn content_length_match_ok(
        length in 1..100usize,
        body_content in "[a-z]{1,100}"
    ) {
        // 同じ値の Content-Length は OK
        let body_bytes = &body_content.as_bytes()[..length.min(body_content.len())];
        let actual_len = body_bytes.len();
        let data = format!(
            "GET / HTTP/1.1\r\nContent-Length: {}\r\nContent-Length: {}\r\n\r\n",
            actual_len, actual_len
        );
        let mut full_data = data.into_bytes();
        full_data.extend_from_slice(body_bytes);

        let mut decoder = RequestDecoder::new();
        decoder.feed(&full_data).unwrap();
        let (_, body_kind) = decoder.decode_headers().unwrap().unwrap();
        prop_assert_eq!(body_kind, BodyKind::ContentLength(actual_len));

        let mut body = Vec::new();
        while let Some(data) = decoder.peek_body() {
            body.extend_from_slice(data);
            let len = data.len();
            if let BodyProgress::Complete { .. } = decoder.consume_body(len).unwrap() {
                break;
            }
        }
        prop_assert_eq!(&body, body_bytes);
    }
}

proptest! {
    #[test]
    fn content_length_zero_no_body(
        method in http_method()
    ) {
        // Content-Length: 0 はボディなし
        let data = format!("{} / HTTP/1.1\r\nContent-Length: 0\r\n\r\n", method);
        let mut decoder = RequestDecoder::new();
        decoder.feed(data.as_bytes()).unwrap();
        let (_, body_kind) = decoder.decode_headers().unwrap().unwrap();
        prop_assert_eq!(body_kind, BodyKind::ContentLength(0));
    }
}

proptest! {
    #[test]
    fn content_length_case_insensitive(
        header_case in prop_oneof![
            Just("Content-Length"),
            Just("content-length"),
            Just("CONTENT-LENGTH"),
            Just("Content-length"),
        ],
        length in 1..100usize
    ) {
        // Content-Length は大文字小文字を区別しない
        let body_content = "x".repeat(length);
        let data = format!("GET / HTTP/1.1\r\n{}: {}\r\n\r\n{}", header_case, length, body_content);
        let mut decoder = RequestDecoder::new();
        decoder.feed(data.as_bytes()).unwrap();
        let (_, body_kind) = decoder.decode_headers().unwrap().unwrap();
        prop_assert_eq!(body_kind, BodyKind::ContentLength(length));

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
// リクエスト行のエラー PBT
// ========================================

proptest! {
    #[test]
    fn request_line_missing_parts_error(
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
    fn request_line_empty_error(
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
// ステータス行のエラー PBT (ResponseDecoder)
// ========================================

proptest! {
    #[test]
    fn status_line_missing_parts_error(
        version in prop_oneof![Just("HTTP/1.0"), Just("HTTP/1.1")]
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
    fn status_code_invalid_error(
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
    fn status_line_no_reason_phrase_ok(
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
    fn head_response_with_content_length(
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
    fn head_response_with_transfer_encoding(
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
    fn status_1xx_no_body(
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
    fn status_204_no_body(
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
    fn status_304_no_body(
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
    fn status_code_boundary_199(
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
    fn status_code_boundary_200(
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
    fn status_code_boundary_203(
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
// チャンクエンコーディング PBT
// ========================================

proptest! {
    #[test]
    fn chunked_invalid_size_error(
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
    fn chunked_size_with_extension_ok(
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
    fn chunked_with_trailer_ok(
        body_content in "[a-z]{1,32}",
        trailer_name in "[A-Za-z]{1,16}",
        trailer_value in "[a-z0-9]{1,16}"
    ) {
        // トレーラーは OK
        let len = body_content.len();
        let data = format!(
            "HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n{:x}\r\n{}\r\n0\r\n{}: {}\r\n\r\n",
            len, body_content, trailer_name, trailer_value
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
    fn chunked_with_multiple_trailers_ok(
        body_content in "[a-z]{1,32}",
        trailer_count in 1..4usize
    ) {
        // 複数のトレーラーは OK
        let len = body_content.len();
        let trailers = (0..trailer_count)
            .map(|i| format!("X-Trailer{}: value{}", i, i))
            .collect::<Vec<_>>()
            .join("\r\n");
        let data = format!(
            "HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n{:x}\r\n{}\r\n0\r\n{}\r\n\r\n",
            len, body_content, trailers
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
    fn chunked_roundtrip(chunks in proptest::collection::vec(body(), 1..5)) {
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
// UTF-8 エラー PBT
// ========================================

proptest! {
    #[test]
    fn invalid_utf8_request_line_error(
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
    fn invalid_utf8_header_error(
        header_name in "[A-Za-z]{1,16}",
        invalid_byte in 128u8..=255
    ) {
        // 無効な UTF-8 バイトを含むヘッダーはエラー
        let mut data = b"GET / HTTP/1.1\r\n".to_vec();
        data.extend(header_name.as_bytes());
        data.extend(b": ");
        data.push(invalid_byte);
        data.extend(b"\r\n\r\n");
        let mut decoder = RequestDecoder::new();
        decoder.feed(&data).unwrap();
        prop_assert!(decoder.decode_headers().is_err());
    }
}

proptest! {
    #[test]
    fn invalid_utf8_chunk_size_error(
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
    fn invalid_utf8_status_line_error(
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
    fn invalid_utf8_response_header_error(
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
// 部分的なデータ (None を返す) PBT
// ========================================

proptest! {
    #[test]
    fn incomplete_request_line(
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
    fn incomplete_headers(
        header_name in "[A-Za-z]{1,16}",
        header_value in "[A-Za-z0-9]{1,16}"
    ) {
        // ヘッダー終端 CRLF がない場合は None
        let data = format!("GET / HTTP/1.1\r\n{}: {}", header_name, header_value);
        let mut decoder = RequestDecoder::new();
        decoder.feed(data.as_bytes()).unwrap();
        prop_assert!(decoder.decode_headers().unwrap().is_none());
    }
}

proptest! {
    #[test]
    fn incomplete_body(
        body_length in 10..100usize,
        partial_length in 1..10usize
    ) {
        // 不完全なボディは peek_body で部分データを返す
        let full_body = "x".repeat(body_length);
        let partial_body = &full_body[..partial_length];
        let data = format!("GET / HTTP/1.1\r\nContent-Length: {}\r\n\r\n{}", body_length, partial_body);
        let mut decoder = RequestDecoder::new();
        decoder.feed(data.as_bytes()).unwrap();
        let (_, body_kind) = decoder.decode_headers().unwrap().unwrap();
        prop_assert_eq!(body_kind, BodyKind::ContentLength(body_length));
        let peeked = decoder.peek_body().unwrap();
        prop_assert_eq!(peeked, partial_body.as_bytes());
    }
}

proptest! {
    #[test]
    fn incomplete_chunk_size(
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
    fn incomplete_chunk_data(
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
    fn incomplete_trailer(
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
// デコーダーリミット PBT
// ========================================

proptest! {
    #[test]
    fn request_decoder_buffer_overflow(
        data_size in 1000..2000usize
    ) {
        let limits = DecoderLimits {
            max_buffer_size: 100,
            ..DecoderLimits::default()
        };
        let mut decoder = RequestDecoder::with_limits(limits);
        let data = "x".repeat(data_size);
        prop_assert!(decoder.feed(data.as_bytes()).is_err());
    }
}

proptest! {
    #[test]
    fn request_decoder_exact_buffer_limit(
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
            prop_assert!(decoder.feed(data.as_bytes()).is_err());
        }
    }
}

proptest! {
    #[test]
    fn request_decoder_header_line_too_long(
        header_value_len in 200..500usize
    ) {
        let limits = DecoderLimits {
            max_header_line_size: 100,
            ..DecoderLimits::default()
        };
        let mut decoder = RequestDecoder::with_limits(limits);
        let header_value = "x".repeat(header_value_len);
        let data = format!("GET / HTTP/1.1\r\nX-Long: {}\r\n\r\n", header_value);
        decoder.feed(data.as_bytes()).unwrap();
        prop_assert!(decoder.decode_headers().is_err());
    }
}

proptest! {
    #[test]
    fn request_decoder_too_many_headers(
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
        let data = format!("GET / HTTP/1.1\r\n{}\r\n\r\n", headers);
        decoder.feed(data.as_bytes()).unwrap();
        prop_assert!(decoder.decode_headers().is_err());
    }
}

proptest! {
    #[test]
    fn request_decoder_exact_header_count(
        extra_headers in 0..5usize
    ) {
        let max_count = 10;
        let limits = DecoderLimits {
            max_headers_count: max_count,
            ..DecoderLimits::default()
        };
        let mut decoder = RequestDecoder::with_limits(limits);
        let header_count = max_count + extra_headers;
        let headers = (0..header_count)
            .map(|i| format!("X-H{}: v{}", i, i))
            .collect::<Vec<_>>()
            .join("\r\n");
        let data = format!("GET / HTTP/1.1\r\n{}\r\n\r\n", headers);
        decoder.feed(data.as_bytes()).unwrap();
        if extra_headers == 0 {
            prop_assert!(decoder.decode_headers().is_ok());
        } else {
            prop_assert!(decoder.decode_headers().is_err());
        }
    }
}

proptest! {
    #[test]
    fn request_decoder_body_too_large_content_length(
        body_size in 200..500usize
    ) {
        let limits = DecoderLimits {
            max_body_size: 100,
            ..DecoderLimits::default()
        };
        let mut decoder = RequestDecoder::with_limits(limits);
        let body = "x".repeat(body_size);
        let data = format!("POST / HTTP/1.1\r\nContent-Length: {}\r\n\r\n{}", body_size, body);
        decoder.feed(data.as_bytes()).unwrap();
        let result = decoder.decode_headers();
        // Content-Length が制限を超えているのでエラー
        prop_assert!(result.is_err());
    }
}

proptest! {
    #[test]
    fn request_decoder_exact_body_size(
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
        let data = format!("POST / HTTP/1.1\r\nContent-Length: {}\r\n\r\n{}", body_size, body);
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
    fn request_decoder_body_too_large_chunked(
        chunk_size in 200..500usize
    ) {
        let limits = DecoderLimits {
            max_body_size: 100,
            ..DecoderLimits::default()
        };
        let mut decoder = RequestDecoder::with_limits(limits);
        let chunk = "x".repeat(chunk_size);
        let data = format!(
            "POST / HTTP/1.1\r\nTransfer-Encoding: chunked\r\n\r\n{:x}\r\n{}\r\n0\r\n\r\n",
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
    fn request_decoder_feed_unchecked_no_limit(
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
    fn request_decoder_limits_getter(
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

proptest! {
    #[test]
    fn response_decoder_buffer_overflow(
        data_size in 1000..2000usize
    ) {
        let limits = DecoderLimits {
            max_buffer_size: 100,
            ..DecoderLimits::default()
        };
        let mut decoder = ResponseDecoder::with_limits(limits);
        let data = "x".repeat(data_size);
        prop_assert!(decoder.feed(data.as_bytes()).is_err());
    }
}

proptest! {
    #[test]
    fn response_decoder_header_line_too_long(
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
        prop_assert!(decoder.decode_headers().is_err());
    }
}

proptest! {
    #[test]
    fn response_decoder_too_many_headers(
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
        prop_assert!(decoder.decode_headers().is_err());
    }
}

proptest! {
    #[test]
    fn response_decoder_body_too_large_content_length(
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
        prop_assert!(result.is_err());
    }
}

proptest! {
    #[test]
    fn response_decoder_body_too_large_chunked(
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
    fn response_decoder_feed_unchecked_no_limit(
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
    fn response_decoder_limits_getter(
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
    fn response_decoder_remaining(
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
    fn response_decoder_reset(
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
    fn response_decoder_reset_expect_no_body(
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
    fn response_no_content_length_no_transfer_encoding(
        status_code in 200..204u16
    ) {
        // Content-Length も Transfer-Encoding もない場合はボディなし
        let data = format!("HTTP/1.1 {} OK\r\n\r\n", status_code);
        let mut decoder = ResponseDecoder::new();
        decoder.feed(data.as_bytes()).unwrap();
        let (_, body_kind) = decoder.decode_headers().unwrap().unwrap();
        prop_assert_eq!(body_kind, BodyKind::None);
    }
}

proptest! {
    #[test]
    fn response_content_length_zero(
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
// 複数リクエスト/レスポンス PBT
// ========================================

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
            decoder.reset();
        }
    }
}

proptest! {
    #[test]
    fn multiple_responses_same_decoder(
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

proptest! {
    #[test]
    fn decoder_reuse_after_error(
        valid_method in http_method(),
        valid_uri in http_uri()
    ) {
        let mut decoder = RequestDecoder::new();
        // 不正なリクエストでエラー
        decoder.feed(b"INVALID\r\n\r\n").unwrap();
        let _ = decoder.decode_headers();
        decoder.reset();
        // リセット後は正常に動作
        let request = Request::new(&valid_method, &valid_uri);
        decoder.feed(&request.encode()).unwrap();
        let decoded = decoder.decode().unwrap().unwrap();
        prop_assert_eq!(decoded.method, valid_method);
    }
}

// ========================================
// ストリーミング API の PBT
// ========================================

proptest! {
    #[test]
    fn streaming_decode_request(
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
            format!("{} {} HTTP/1.1\r\n{}\r\n\r\n", method, uri, headers)
        } else {
            format!("{} {} HTTP/1.1\r\n\r\n", method, uri)
        };
        decoder.feed(data.as_bytes()).unwrap();
        let (head, body_kind) = decoder.decode_headers().unwrap().unwrap();
        prop_assert_eq!(head.method, method);
        prop_assert_eq!(head.uri, uri);
        prop_assert_eq!(body_kind, BodyKind::None);
    }
}

proptest! {
    #[test]
    fn streaming_decode_request_with_body(
        method in prop_oneof![Just("POST"), Just("PUT")],
        body_content in "[a-z]{1,100}"
    ) {
        let mut decoder = RequestDecoder::new();
        let body_len = body_content.len();
        let data = format!(
            "{} / HTTP/1.1\r\nContent-Length: {}\r\n\r\n{}",
            method, body_len, body_content
        );
        decoder.feed(data.as_bytes()).unwrap();
        let (head, body_kind) = decoder.decode_headers().unwrap().unwrap();
        prop_assert_eq!(head.method, method);
        prop_assert_eq!(body_kind, BodyKind::ContentLength(body_len));

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

proptest! {
    #[test]
    fn streaming_decode_response(
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
        // 1xx, 204, 304 はボディなし (RFC 9110)
        if (100..200).contains(&status_code) || status_code == 204 || status_code == 304 {
            prop_assert_eq!(body_kind, BodyKind::None);
        } else {
            prop_assert_eq!(body_kind, BodyKind::ContentLength(body_len));
        }
    }
}

// ========================================
// Request/Response ラウンドトリップ PBT
// ========================================

proptest! {
    #[test]
    fn request_roundtrip(
        method in http_method(),
        uri in http_uri(),
        body_data in body()
    ) {
        let mut request = Request::new(&method, &uri)
            .header("Host", "example.com");

        if !body_data.is_empty() {
            request = request.body(body_data.clone());
        }

        let encoded = request.encode();
        let mut decoder = RequestDecoder::new();
        decoder.feed(&encoded).unwrap();
        let decoded = decoder.decode().unwrap().unwrap();

        prop_assert_eq!(decoded.method, method);
        prop_assert_eq!(decoded.uri, uri);
        prop_assert_eq!(decoded.body, body_data);
    }
}

proptest! {
    #[test]
    fn response_roundtrip(
        status in status_code(),
        reason in reason_phrase(),
        body_data in body()
    ) {
        let mut response = Response::new(status, &reason);

        if !body_data.is_empty() {
            response = response.body(body_data.clone());
        }

        let encoded = response.encode();
        let mut decoder = ResponseDecoder::new();
        decoder.feed(&encoded).unwrap();
        let decoded = decoder.decode().unwrap().unwrap();

        prop_assert_eq!(decoded.status_code, status);
        if status != 204 && status != 304 && !(100..200).contains(&status) {
            prop_assert_eq!(decoded.body, body_data);
        }
    }
}

// ========================================
// パース入力パニックなし PBT
// ========================================

proptest! {
    #[test]
    fn request_decoder_parse_no_panic(data in proptest::collection::vec(any::<u8>(), 0..512)) {
        let mut decoder = RequestDecoder::new();
        let _ = decoder.feed(&data);
        let _ = decoder.decode_headers();
        let _ = decoder.peek_body();
        let _ = decoder.progress();
    }
}

proptest! {
    #[test]
    fn response_decoder_parse_no_panic(data in proptest::collection::vec(any::<u8>(), 0..512)) {
        let mut decoder = ResponseDecoder::new();
        let _ = decoder.feed(&data);
        let _ = decoder.decode_headers();
        let _ = decoder.peek_body();
        let _ = decoder.progress();
    }
}

// ========================================
// decode_headers を2回呼んだ場合の挙動 PBT
// ========================================

proptest! {
    #[test]
    fn decode_headers_twice_returns_none(
        uri in "[a-z]{1,10}"
    ) {
        // ボディなしメッセージの場合、2 回目の decode_headers は Ok(None)
        let data = format!("GET /{} HTTP/1.1\r\n\r\n", uri);
        let mut decoder = RequestDecoder::new();
        decoder.feed(data.as_bytes()).unwrap();
        let _ = decoder.decode_headers().unwrap().unwrap();
        // 2回目は次のメッセージがないので Ok(None)
        prop_assert!(decoder.decode_headers().unwrap().is_none());
    }
}

proptest! {
    #[test]
    fn response_decode_headers_twice_returns_none(
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
// consume_body を decode_headers 前に呼ぶとエラー PBT
// ========================================

proptest! {
    #[test]
    fn consume_body_before_decode_headers_error(
        method in http_method()
    ) {
        let mut decoder = RequestDecoder::new();
        let data = format!("{} / HTTP/1.1\r\n\r\n", method);
        decoder.feed(data.as_bytes()).unwrap();
        prop_assert!(decoder.progress().is_err());
    }
}

proptest! {
    #[test]
    fn response_consume_body_before_decode_headers_error(
        status_code in 200..600u16
    ) {
        let mut decoder = ResponseDecoder::new();
        let data = format!("HTTP/1.1 {} OK\r\n\r\n", status_code);
        decoder.feed(data.as_bytes()).unwrap();
        prop_assert!(decoder.progress().is_err());
    }
}

// ========================================
// decode() API の連続デコードテスト (Keep-Alive) PBT
// ========================================

proptest! {
    #[test]
    fn decode_multiple_requests_keep_alive(
        methods in proptest::collection::vec(http_method(), 2..5),
        uris in proptest::collection::vec(http_uri(), 2..5)
    ) {
        let count = methods.len().min(uris.len());
        let mut decoder = RequestDecoder::new();

        // 全リクエストを一度にバッファに入れる
        let mut all_data = Vec::new();
        for i in 0..count {
            let request = Request::new(&methods[i], &uris[i]);
            all_data.extend(request.encode());
        }
        decoder.feed(&all_data).unwrap();

        // decode() を連続して呼ぶ（reset() なし）
        for i in 0..count {
            let request = decoder.decode().unwrap().unwrap();
            prop_assert_eq!(&request.method, &methods[i]);
            prop_assert_eq!(&request.uri, &uris[i]);
        }
    }
}

proptest! {
    #[test]
    fn decode_multiple_requests_with_body_keep_alive(
        bodies in proptest::collection::vec(
            proptest::collection::vec(any::<u8>(), 0..64),
            2..4
        )
    ) {
        let mut decoder = RequestDecoder::new();

        // 全リクエストを一度にバッファに入れる
        let mut all_data = Vec::new();
        for body_data in &bodies {
            let mut request = Request::new("POST", "/");
            request.body = body_data.clone();
            all_data.extend(request.encode());
        }
        decoder.feed(&all_data).unwrap();

        // decode() を連続して呼ぶ（reset() なし）
        for body_data in &bodies {
            let request = decoder.decode().unwrap().unwrap();
            prop_assert_eq!(&request.body, body_data);
        }
    }
}

proptest! {
    #[test]
    fn decode_multiple_responses_keep_alive(
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

proptest! {
    #[test]
    fn decode_two_requests_keep_alive_simple(
        method1 in http_method(),
        uri1 in http_uri(),
        method2 in http_method(),
        uri2 in http_uri()
    ) {
        let mut decoder = RequestDecoder::new();

        let req1 = Request::new(&method1, &uri1);
        let req2 = Request::new(&method2, &uri2);

        decoder.feed(&req1.encode()).unwrap();
        decoder.feed(&req2.encode()).unwrap();

        let decoded1 = decoder.decode().unwrap().unwrap();
        prop_assert_eq!(decoded1.method, method1);
        prop_assert_eq!(decoded1.uri, uri1);

        let decoded2 = decoder.decode().unwrap().unwrap();
        prop_assert_eq!(decoded2.method, method2);
        prop_assert_eq!(decoded2.uri, uri2);
    }
}

proptest! {
    #[test]
    fn decode_two_responses_keep_alive_simple(
        code1 in status_code(),
        code2 in status_code()
    ) {
        let mut decoder = ResponseDecoder::new();

        let resp1 = Response::new(code1, "OK");
        let resp2 = Response::new(code2, "OK");

        decoder.feed(&resp1.encode()).unwrap();
        decoder.feed(&resp2.encode()).unwrap();

        let decoded1 = decoder.decode().unwrap().unwrap();
        prop_assert_eq!(decoded1.status_code, code1);

        let decoded2 = decoder.decode().unwrap().unwrap();
        prop_assert_eq!(decoded2.status_code, code2);
    }
}

// ========================================
// チャンク CRLF 検証 PBT
// ========================================

proptest! {
    #[test]
    fn chunked_invalid_crlf_after_data_error(
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
    fn chunked_invalid_crlf_partial_then_error(
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
    fn request_chunked_invalid_crlf_error(
        body_content in "[a-z]{5,10}",
        invalid_chars in "[A-Z]{2,4}"
    ) {
        // リクエストでもチャンクの CRLF 検証
        let len = body_content.len();
        let data = format!(
            "POST / HTTP/1.1\r\nTransfer-Encoding: chunked\r\n\r\n{:x}\r\n{}{}",
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
    fn decode_multiple_chunked_responses_keep_alive(
        body1 in "[a-z]{1,32}",
        body2 in "[a-z]{1,32}"
    ) {
        let mut decoder = ResponseDecoder::new();

        // 2 つの chunked レスポンスを作成
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
        prop_assert_eq!(response1.body, body1.as_bytes());

        // 2 回目のデコード
        let response2 = decoder.decode().unwrap().unwrap();
        prop_assert_eq!(response2.body, body2.as_bytes());
    }
}

proptest! {
    #[test]
    fn decode_multiple_chunked_requests_keep_alive(
        body1 in "[a-z]{1,32}",
        body2 in "[a-z]{1,32}"
    ) {
        let mut decoder = RequestDecoder::new();

        // 2 つの chunked リクエストを作成
        let req1 = format!(
            "POST /first HTTP/1.1\r\nTransfer-Encoding: chunked\r\n\r\n{:x}\r\n{}\r\n0\r\n\r\n",
            body1.len(), body1
        );
        let req2 = format!(
            "POST /second HTTP/1.1\r\nTransfer-Encoding: chunked\r\n\r\n{:x}\r\n{}\r\n0\r\n\r\n",
            body2.len(), body2
        );

        decoder.feed(req1.as_bytes()).unwrap();
        decoder.feed(req2.as_bytes()).unwrap();

        // 1 回目のデコード
        let request1 = decoder.decode().unwrap().unwrap();
        prop_assert_eq!(request1.uri, "/first");
        prop_assert_eq!(request1.body, body1.as_bytes());

        // 2 回目のデコード
        let request2 = decoder.decode().unwrap().unwrap();
        prop_assert_eq!(request2.uri, "/second");
        prop_assert_eq!(request2.body, body2.as_bytes());
    }
}

proptest! {
    #[test]
    fn decode_multiple_chunked_responses_with_body_limit(
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
        prop_assert_eq!(response1.body.len(), body1_len);

        // 2 回目のデコード
        let response2 = decoder.decode().unwrap().unwrap();
        prop_assert_eq!(response2.body.len(), body2_len);
    }
}

proptest! {
    #[test]
    fn decode_multiple_chunked_requests_with_body_limit(
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
            "POST /1 HTTP/1.1\r\nTransfer-Encoding: chunked\r\n\r\n{:x}\r\n{}\r\n0\r\n\r\n",
            body1.len(), body1
        );
        let req2 = format!(
            "POST /2 HTTP/1.1\r\nTransfer-Encoding: chunked\r\n\r\n{:x}\r\n{}\r\n0\r\n\r\n",
            body2.len(), body2
        );

        decoder.feed(req1.as_bytes()).unwrap();
        decoder.feed(req2.as_bytes()).unwrap();

        let request1 = decoder.decode().unwrap().unwrap();
        prop_assert_eq!(request1.body.len(), body1_len);

        let request2 = decoder.decode().unwrap().unwrap();
        prop_assert_eq!(request2.body.len(), body2_len);
    }
}

proptest! {
    #[test]
    fn decode_multiple_chunked_responses_keep_alive_pbt(
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
            prop_assert_eq!(&response.body, body_data);
        }
    }
}

proptest! {
    #[test]
    fn decode_multiple_chunked_requests_keep_alive_pbt(
        bodies in proptest::collection::vec(
            proptest::collection::vec(any::<u8>(), 0..64),
            2..4
        )
    ) {
        let mut decoder = RequestDecoder::new();

        // chunked リクエストを生成してバッファに追加
        let mut all_data = Vec::new();
        for (i, body_data) in bodies.iter().enumerate() {
            let mut data = format!("POST /{} HTTP/1.1\r\nTransfer-Encoding: chunked\r\n\r\n", i)
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
            prop_assert_eq!(&request.body, body_data);
        }
    }
}

// ========================================
// is_chunked() トークン解析 PBT
// ========================================

proptest! {
    #[test]
    fn is_chunked_only_chunked_token(
        leading_spaces in 0..4usize,
        trailing_spaces in 0..4usize
    ) {
        // "chunked" のみ (前後にスペースあり) の場合は true
        let te_value = format!(
            "{}chunked{}",
            " ".repeat(leading_spaces),
            " ".repeat(trailing_spaces)
        );
        let head = ResponseHead {
            version: "HTTP/1.1".to_string(),
            status_code: 200,
            reason_phrase: "OK".to_string(),
            headers: vec![("Transfer-Encoding".to_string(), te_value)],
        };
        prop_assert!(head.is_chunked());
    }
}

proptest! {
    #[test]
    fn is_chunked_with_other_token_returns_false(
        other_token in transfer_encoding_token().prop_filter("not chunked", |t| !t.eq_ignore_ascii_case("chunked")),
        chunked_first in any::<bool>()
    ) {
        // chunked 以外のトークンがある場合は false
        let te_value = if chunked_first {
            format!("chunked, {}", other_token)
        } else {
            format!("{}, chunked", other_token)
        };
        let head = ResponseHead {
            version: "HTTP/1.1".to_string(),
            status_code: 200,
            reason_phrase: "OK".to_string(),
            headers: vec![("Transfer-Encoding".to_string(), te_value)],
        };
        prop_assert!(!head.is_chunked());
    }
}

proptest! {
    #[test]
    fn is_chunked_other_token_only_returns_false(
        token in transfer_encoding_token().prop_filter("not chunked", |t| !t.eq_ignore_ascii_case("chunked"))
    ) {
        // chunked 以外のトークンのみの場合は false
        let head = ResponseHead {
            version: "HTTP/1.1".to_string(),
            status_code: 200,
            reason_phrase: "OK".to_string(),
            headers: vec![("Transfer-Encoding".to_string(), token)],
        };
        prop_assert!(!head.is_chunked());
    }
}

proptest! {
    #[test]
    fn is_chunked_no_header_returns_false(
        status_code in 200..600u16
    ) {
        // Transfer-Encoding ヘッダーがない場合は false
        let head = ResponseHead {
            version: "HTTP/1.1".to_string(),
            status_code,
            reason_phrase: "OK".to_string(),
            headers: vec![],
        };
        prop_assert!(!head.is_chunked());
    }
}

proptest! {
    #[test]
    fn is_chunked_consistency_with_body_kind(
        use_chunked in any::<bool>()
    ) {
        // is_chunked() と BodyKind::Chunked の整合性を検証
        let mut decoder = ResponseDecoder::new();
        let data = if use_chunked {
            b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n".to_vec()
        } else {
            b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\n\r\n".to_vec()
        };
        decoder.feed(&data).unwrap();
        let (head, body_kind) = decoder.decode_headers().unwrap().unwrap();

        if use_chunked {
            prop_assert!(head.is_chunked());
            prop_assert!(matches!(body_kind, BodyKind::Chunked));
        } else {
            prop_assert!(!head.is_chunked());
            prop_assert!(!matches!(body_kind, BodyKind::Chunked));
        }
    }
}

// ========================================
// is_keep_alive() トークン解析 PBT
// ========================================

proptest! {
    #[test]
    fn is_keep_alive_close_token_returns_false(
        version in prop_oneof![Just("HTTP/1.0"), Just("HTTP/1.1")]
    ) {
        // "close" トークンがあれば false
        let head = ResponseHead {
            version: version.to_string(),
            status_code: 200,
            reason_phrase: "OK".to_string(),
            headers: vec![("Connection".to_string(), "close".to_string())],
        };
        prop_assert!(!head.is_keep_alive());
    }
}

proptest! {
    #[test]
    fn is_keep_alive_keep_alive_token_returns_true(
        version in prop_oneof![Just("HTTP/1.0"), Just("HTTP/1.1")]
    ) {
        // "keep-alive" トークンがあれば true
        let head = ResponseHead {
            version: version.to_string(),
            status_code: 200,
            reason_phrase: "OK".to_string(),
            headers: vec![("Connection".to_string(), "keep-alive".to_string())],
        };
        prop_assert!(head.is_keep_alive());
    }
}

proptest! {
    #[test]
    fn is_keep_alive_default_by_version(
        status_code in 200..600u16
    ) {
        // HTTP/1.1 のデフォルトは keep-alive、HTTP/1.0 のデフォルトは close
        let head_11 = ResponseHead {
            version: "HTTP/1.1".to_string(),
            status_code,
            reason_phrase: "OK".to_string(),
            headers: vec![],
        };
        prop_assert!(head_11.is_keep_alive());

        let head_10 = ResponseHead {
            version: "HTTP/1.0".to_string(),
            status_code,
            reason_phrase: "OK".to_string(),
            headers: vec![],
        };
        prop_assert!(!head_10.is_keep_alive());
    }
}

proptest! {
    #[test]
    fn is_keep_alive_close_priority_over_keep_alive(
        keep_alive_first in any::<bool>()
    ) {
        // close と keep-alive が両方ある場合、close が優先される
        let conn_value = if keep_alive_first {
            "keep-alive, close".to_string()
        } else {
            "close, keep-alive".to_string()
        };
        let head = ResponseHead {
            version: "HTTP/1.1".to_string(),
            status_code: 200,
            reason_phrase: "OK".to_string(),
            headers: vec![("Connection".to_string(), conn_value)],
        };
        prop_assert!(!head.is_keep_alive());
    }
}

// ========================================
// decode_headers の Complete → StartLine 遷移 PBT
// ========================================

proptest! {
    #[test]
    fn decode_headers_multiple_no_body_messages(
        count in 2..5usize
    ) {
        // 複数のボディなしメッセージを decode_headers で連続処理
        let mut decoder = RequestDecoder::new();
        for i in 0..count {
            let data = format!("GET /{} HTTP/1.1\r\n\r\n", i);
            decoder.feed(data.as_bytes()).unwrap();
        }

        for i in 0..count {
            let (head, body_kind) = decoder.decode_headers().unwrap().unwrap();
            prop_assert_eq!(head.uri, format!("/{}", i));
            prop_assert!(matches!(body_kind, BodyKind::None));
        }

        // 次のメッセージがなければ Ok(None)
        prop_assert!(decoder.decode_headers().unwrap().is_none());
    }
}

proptest! {
    #[test]
    fn response_decode_headers_multiple_no_body_messages(
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
// リクエストとレスポンスの incomplete テスト (追加)
// ========================================

proptest! {
    #[test]
    fn request_incomplete_chunk_size(
        size in 1..100usize
    ) {
        // 不完全なチャンクサイズ行は None
        let data = format!("POST / HTTP/1.1\r\nTransfer-Encoding: chunked\r\n\r\n{:x}", size);
        let mut decoder = RequestDecoder::new();
        decoder.feed(data.as_bytes()).unwrap();
        let (_, _) = decoder.decode_headers().unwrap().unwrap();
        prop_assert!(decoder.peek_body().is_none());
    }
}

proptest! {
    #[test]
    fn request_incomplete_chunk_data(
        chunk_size in 10..100usize,
        partial_size in 1..10usize
    ) {
        // 不完全なチャンクデータは部分データを返す
        let partial_data = "x".repeat(partial_size);
        let data = format!(
            "POST / HTTP/1.1\r\nTransfer-Encoding: chunked\r\n\r\n{:x}\r\n{}",
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
    fn request_incomplete_trailer(
        body_content in "[a-z]{1,32}",
        trailer_name in "[A-Za-z]{1,16}"
    ) {
        // 不完全なトレーラーは Continue
        let len = body_content.len();
        let data = format!(
            "POST / HTTP/1.1\r\nTransfer-Encoding: chunked\r\n\r\n{:x}\r\n{}\r\n0\r\n{}: value",
            len, body_content, trailer_name
        );
        let mut decoder = RequestDecoder::new();
        decoder.feed(data.as_bytes()).unwrap();
        let (_, _) = decoder.decode_headers().unwrap().unwrap();

        loop {
            if let Some(data) = decoder.peek_body() {
                let len = data.len();
                let result = decoder.consume_body(len).unwrap();
                if matches!(result, BodyProgress::Complete { .. }) {
                    break;
                }
            } else {
                let result = decoder.progress().unwrap();
                prop_assert!(matches!(result, BodyProgress::Continue));
                break;
            }
        }
    }
}
