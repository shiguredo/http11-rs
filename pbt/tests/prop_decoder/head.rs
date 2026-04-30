//! ヘッダーパース・HttpHead トレイトの PBT

use proptest::prelude::*;
use shiguredo_http11::{BodyKind, HttpHead, RequestDecoder, ResponseDecoder, ResponseHead};

use super::{
    http_method, invalid_header_name_char, transfer_encoding_token, valid_header_name_special_char,
};

// ========================================
// ヘッダーパースエラーの PBT
// ========================================

proptest! {
    #[test]
    fn prop_header_obs_fold_space_error(
        header_name in "[A-Za-z]{1,16}",
        header_value in "[A-Za-z0-9]{1,16}"
    ) {
        // obs-fold (行頭スペース) はエラー
        let data = format!("GET / HTTP/1.1\r\nHost: localhost\r\n {}: {}\r\n\r\n", header_name, header_value);
        let mut decoder = RequestDecoder::new();
        decoder.feed(data.as_bytes()).unwrap();
        prop_assert!(decoder.decode_headers().is_err());
    }
}

proptest! {
    #[test]
    fn prop_header_obs_fold_tab_error(
        header_name in "[A-Za-z]{1,16}",
        header_value in "[A-Za-z0-9]{1,16}"
    ) {
        // obs-fold (行頭タブ) はエラー
        let data = format!("GET / HTTP/1.1\r\nHost: localhost\r\n\t{}: {}\r\n\r\n", header_name, header_value);
        let mut decoder = RequestDecoder::new();
        decoder.feed(data.as_bytes()).unwrap();
        prop_assert!(decoder.decode_headers().is_err());
    }
}

proptest! {
    #[test]
    fn prop_header_contains_cr_error(
        prefix in "[A-Za-z]{1,8}",
        suffix in "[A-Za-z]{1,8}"
    ) {
        // ヘッダー名に CR を含むとエラー
        let data = format!("GET / HTTP/1.1\r\nHost: localhost\r\n{}\r{}: value\r\n\r\n", prefix, suffix);
        let mut decoder = RequestDecoder::new();
        decoder.feed(data.as_bytes()).unwrap();
        prop_assert!(decoder.decode_headers().is_err());
    }
}

proptest! {
    #[test]
    fn prop_header_contains_lf_error(
        prefix in "[A-Za-z]{1,8}",
        suffix in "[A-Za-z]{1,8}"
    ) {
        // ヘッダー名に LF を含むとエラー
        let data = format!("GET / HTTP/1.1\r\nHost: localhost\r\n{}\n{}: value\r\n\r\n", prefix, suffix);
        let mut decoder = RequestDecoder::new();
        decoder.feed(data.as_bytes()).unwrap();
        prop_assert!(decoder.decode_headers().is_err());
    }
}

proptest! {
    #[test]
    fn prop_header_missing_colon_error(
        header_name in "[A-Za-z]{1,16}",
        header_value in "[A-Za-z0-9]{1,16}"
    ) {
        // コロンがないとエラー
        let data = format!("GET / HTTP/1.1\r\nHost: localhost\r\n{} {}\r\n\r\n", header_name, header_value);
        let mut decoder = RequestDecoder::new();
        decoder.feed(data.as_bytes()).unwrap();
        prop_assert!(decoder.decode_headers().is_err());
    }
}

proptest! {
    #[test]
    fn prop_header_empty_name_error(
        header_value in "[A-Za-z0-9]{1,16}"
    ) {
        // 空のヘッダー名はエラー
        let data = format!("GET / HTTP/1.1\r\nHost: localhost\r\n: {}\r\n\r\n", header_value);
        let mut decoder = RequestDecoder::new();
        decoder.feed(data.as_bytes()).unwrap();
        prop_assert!(decoder.decode_headers().is_err());
    }
}

proptest! {
    #[test]
    fn prop_header_name_with_space_error(
        prefix in "[A-Za-z]{1,8}",
        suffix in "[A-Za-z]{1,8}"
    ) {
        // ヘッダー名にスペースを含むとエラー
        let data = format!("GET / HTTP/1.1\r\nHost: localhost\r\n{} {}: value\r\n\r\n", prefix, suffix);
        let mut decoder = RequestDecoder::new();
        decoder.feed(data.as_bytes()).unwrap();
        prop_assert!(decoder.decode_headers().is_err());
    }
}

proptest! {
    #[test]
    fn prop_header_name_trailing_space_error(
        header_name in "[A-Za-z]{1,16}"
    ) {
        // ヘッダー名の後にスペースがあるとエラー
        let data = format!("GET / HTTP/1.1\r\nHost: localhost\r\n{} : value\r\n\r\n", header_name);
        let mut decoder = RequestDecoder::new();
        decoder.feed(data.as_bytes()).unwrap();
        prop_assert!(decoder.decode_headers().is_err());
    }
}

proptest! {
    #[test]
    fn prop_header_invalid_name_char_error(
        prefix in "[A-Za-z]{1,8}",
        invalid_char in invalid_header_name_char(),
        suffix in "[A-Za-z]{1,8}"
    ) {
        // 無効な文字を含むヘッダー名はエラー
        let data = format!("GET / HTTP/1.1\r\nHost: localhost\r\n{}{}{}: value\r\n\r\n", prefix, invalid_char, suffix);
        let mut decoder = RequestDecoder::new();
        decoder.feed(data.as_bytes()).unwrap();
        prop_assert!(decoder.decode_headers().is_err());
    }
}

proptest! {
    #[test]
    fn prop_valid_header_name_chars(
        prefix in "[A-Za-z]{1,8}",
        special_char in valid_header_name_special_char(),
        suffix in "[A-Za-z]{1,8}"
    ) {
        // 有効な特殊文字を含むヘッダー名は OK
        let header_name = format!("{}{}{}", prefix, special_char, suffix);
        let data = format!("GET / HTTP/1.1\r\nHost: localhost\r\n{}: value\r\n\r\n", header_name);
        let mut decoder = RequestDecoder::new();
        decoder.feed(data.as_bytes()).unwrap();
        prop_assert!(decoder.decode_headers().is_ok());
    }
}

proptest! {
    #[test]
    fn prop_header_value_leading_trailing_spaces(
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
        let data = format!("GET / HTTP/1.1\r\nHost: localhost\r\n{}:{}\r\n\r\n", header_name, padded_value);
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
    fn prop_transfer_encoding_and_content_length_error(
        content_length in 1..1000usize
    ) {
        // Transfer-Encoding と Content-Length の両方があるとエラー
        let data = format!(
            "GET / HTTP/1.1\r\nHost: localhost\r\nTransfer-Encoding: chunked\r\nContent-Length: {}\r\n\r\n",
            content_length
        );
        let mut decoder = RequestDecoder::new();
        decoder.feed(data.as_bytes()).unwrap();
        prop_assert!(decoder.decode_headers().is_err());
    }
}

proptest! {
    #[test]
    fn prop_transfer_encoding_unsupported_error(
        coding in transfer_encoding_token().prop_filter("not chunked", |t| !t.eq_ignore_ascii_case("chunked"))
    ) {
        // chunked 以外の Transfer-Encoding はエラー
        let data = format!("GET / HTTP/1.1\r\nHost: localhost\r\nTransfer-Encoding: {}\r\n\r\n", coding);
        let mut decoder = RequestDecoder::new();
        decoder.feed(data.as_bytes()).unwrap();
        prop_assert!(decoder.decode_headers().is_err());
    }
}

proptest! {
    #[test]
    fn prop_transfer_encoding_duplicate_or_unsupported_with_empty_elements(
        before_comma in 0..3usize,
        after_comma in 0..3usize
    ) {
        // RFC 9110 Section 5.6.1.2: 空リスト要素は無視する
        // 空要素を無視した後も duplicate chunked または unsupported coding でエラー
        let data = format!(
            "GET / HTTP/1.1\r\nHost: localhost\r\nTransfer-Encoding: {},,{}\r\n\r\n",
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
    fn prop_transfer_encoding_empty_value_accepted(
        method in http_method()
    ) {
        // RFC 9110 Section 5.6.1.2: 空リスト要素は無視する (MUST)
        // 有効要素なし → Transfer-Encoding なしとして受理する
        let data = format!("{} / HTTP/1.1\r\nHost: localhost\r\nTransfer-Encoding: \r\n\r\n", method);
        let mut decoder = RequestDecoder::new();
        decoder.feed(data.as_bytes()).unwrap();
        prop_assert!(decoder.decode_headers().is_ok());
    }
}

proptest! {
    #[test]
    fn prop_transfer_encoding_case_insensitive(
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
                if let shiguredo_http11::BodyProgress::Complete { .. } = decoder.consume_body(len).unwrap() {
                    break;
                }
            } else if let shiguredo_http11::BodyProgress::Complete { .. } = decoder.progress().unwrap() {
                break;
            }
        }
        prop_assert_eq!(body, b"hello");
    }
}

proptest! {
    #[test]
    fn prop_multiple_transfer_encoding_chunked_error(
        count in 2..4usize
    ) {
        // RFC 9112: chunked は一度だけ指定可能、重複はエラー
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
        // 重複 chunked はエラーを返すべき
        prop_assert!(decoder.decode_headers().is_err());
    }
}

proptest! {
    #[test]
    fn prop_single_transfer_encoding_chunked_ok(
        body in "[a-z]{1,100}"
    ) {
        // 単一の chunked ヘッダーは OK
        let chunk_size = format!("{:x}", body.len());
        let data = format!(
            "HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n{}\r\n{}\r\n0\r\n\r\n",
            chunk_size, body
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
    fn prop_content_length_not_number_error(
        invalid_value in "[a-zA-Z]{1,8}"
    ) {
        // 数字でない Content-Length はエラー
        let data = format!("GET / HTTP/1.1\r\nHost: localhost\r\nContent-Length: {}\r\n\r\n", invalid_value);
        let mut decoder = RequestDecoder::new();
        decoder.feed(data.as_bytes()).unwrap();
        prop_assert!(decoder.decode_headers().is_err());
    }
}

proptest! {
    #[test]
    fn prop_content_length_empty_error(
        method in http_method()
    ) {
        // 空の Content-Length はエラー
        let data = format!("{} / HTTP/1.1\r\nHost: localhost\r\nContent-Length: \r\n\r\n", method);
        let mut decoder = RequestDecoder::new();
        decoder.feed(data.as_bytes()).unwrap();
        prop_assert!(decoder.decode_headers().is_err());
    }
}

proptest! {
    #[test]
    fn prop_content_length_mismatch_error(
        len1 in 1..100usize,
        len2 in 101..200usize
    ) {
        // 異なる値の Content-Length はエラー
        let data = format!(
            "GET / HTTP/1.1\r\nHost: localhost\r\nContent-Length: {}\r\nContent-Length: {}\r\n\r\n",
            len1, len2
        );
        let mut decoder = RequestDecoder::new();
        decoder.feed(data.as_bytes()).unwrap();
        prop_assert!(decoder.decode_headers().is_err());
    }
}

proptest! {
    #[test]
    fn prop_content_length_match_ok(
        length in 1..100usize,
        body_content in "[a-z]{1,100}"
    ) {
        // 同じ値の Content-Length は OK
        let body_bytes = &body_content.as_bytes()[..length.min(body_content.len())];
        let actual_len = body_bytes.len();
        let data = format!(
            "GET / HTTP/1.1\r\nHost: localhost\r\nContent-Length: {}\r\nContent-Length: {}\r\n\r\n",
            actual_len, actual_len
        );
        let mut full_data = data.into_bytes();
        full_data.extend_from_slice(body_bytes);

        let mut decoder = RequestDecoder::new();
        decoder.feed(&full_data).unwrap();
        let (_, body_kind) = decoder.decode_headers().unwrap().unwrap();
        prop_assert_eq!(body_kind, BodyKind::ContentLength(actual_len as u64));

        let mut body = Vec::new();
        while let Some(data) = decoder.peek_body() {
            body.extend_from_slice(data);
            let len = data.len();
            if let shiguredo_http11::BodyProgress::Complete { .. } = decoder.consume_body(len).unwrap() {
                break;
            }
        }
        prop_assert_eq!(&body, body_bytes);
    }
}

proptest! {
    #[test]
    fn prop_content_length_zero_no_body(
        method in http_method()
    ) {
        // Content-Length: 0 はボディなし
        let data = format!("{} / HTTP/1.1\r\nHost: localhost\r\nContent-Length: 0\r\n\r\n", method);
        let mut decoder = RequestDecoder::new();
        decoder.feed(data.as_bytes()).unwrap();
        let (_, body_kind) = decoder.decode_headers().unwrap().unwrap();
        prop_assert_eq!(body_kind, BodyKind::ContentLength(0));
    }
}

proptest! {
    #[test]
    fn prop_content_length_case_insensitive(
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
        let data = format!("GET / HTTP/1.1\r\nHost: localhost\r\n{}: {}\r\n\r\n{}", header_case, length, body_content);
        let mut decoder = RequestDecoder::new();
        decoder.feed(data.as_bytes()).unwrap();
        let (_, body_kind) = decoder.decode_headers().unwrap().unwrap();
        prop_assert_eq!(body_kind, BodyKind::ContentLength(length as u64));

        let mut body = Vec::new();
        while let Some(data) = decoder.peek_body() {
            body.extend_from_slice(data);
            let len = data.len();
            if let shiguredo_http11::BodyProgress::Complete { .. } = decoder.consume_body(len).unwrap() {
                break;
            }
        }
        prop_assert_eq!(body, body_content.as_bytes());
    }
}

// ========================================
// is_chunked() トークン解析 PBT
// ========================================

proptest! {
    #[test]
    fn prop_is_chunked_only_chunked_token(
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
    fn prop_is_chunked_last_token_determines_result(
        other_token in transfer_encoding_token().prop_filter("not chunked", |t| !t.eq_ignore_ascii_case("chunked")),
        chunked_first in any::<bool>()
    ) {
        // RFC 9112 Section 6.3: 最後のトークンが chunked かどうかで判定
        let te_value = if chunked_first {
            // "chunked, other" → 最後が chunked でない → false
            format!("chunked, {}", other_token)
        } else {
            // "other, chunked" → 最後が chunked → true
            format!("{}, chunked", other_token)
        };
        let head = ResponseHead {
            version: "HTTP/1.1".to_string(),
            status_code: 200,
            reason_phrase: "OK".to_string(),
            headers: vec![("Transfer-Encoding".to_string(), te_value)],
        };
        if chunked_first {
            prop_assert!(!head.is_chunked());
        } else {
            prop_assert!(head.is_chunked());
        }
    }
}

proptest! {
    #[test]
    fn prop_is_chunked_other_token_only_returns_false(
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
    fn prop_is_chunked_no_header_returns_false(
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
    fn prop_is_chunked_consistency_with_body_kind(
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
    fn prop_is_keep_alive_close_token_returns_false(
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
    fn prop_is_keep_alive_keep_alive_token_returns_true(
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
    fn prop_is_keep_alive_default_by_version(
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
    fn prop_is_keep_alive_close_priority_over_keep_alive(
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
// 複数 Connection ヘッダーの is_keep_alive() PBT
// RFC 9110 Section 9.1: 複数ヘッダーはリストとして結合して処理
// ========================================

proptest! {
    #[test]
    fn prop_is_keep_alive_multiple_headers_all_keep_alive(
        header_count in 2..5usize
    ) {
        // 複数の Connection: keep-alive ヘッダーがある場合は true
        let headers: Vec<(String, String)> = (0..header_count)
            .map(|_| ("Connection".to_string(), "keep-alive".to_string()))
            .collect();
        let head = ResponseHead {
            version: "HTTP/1.1".to_string(),
            status_code: 200,
            reason_phrase: "OK".to_string(),
            headers,
        };
        prop_assert!(head.is_keep_alive());
    }
}

proptest! {
    #[test]
    fn prop_is_keep_alive_multiple_headers_close_in_later(
        keep_alive_count in 1..4usize
    ) {
        // 最初に keep-alive、後に close がある場合は false (close 優先)
        let mut headers: Vec<(String, String)> = (0..keep_alive_count)
            .map(|_| ("Connection".to_string(), "keep-alive".to_string()))
            .collect();
        headers.push(("Connection".to_string(), "close".to_string()));

        let head = ResponseHead {
            version: "HTTP/1.1".to_string(),
            status_code: 200,
            reason_phrase: "OK".to_string(),
            headers,
        };
        prop_assert!(!head.is_keep_alive());
    }
}

proptest! {
    #[test]
    fn prop_is_keep_alive_multiple_headers_close_in_first(
        keep_alive_count in 1..4usize
    ) {
        // 最初に close、後に keep-alive がある場合も false (close 優先)
        let mut headers: Vec<(String, String)> = vec![("Connection".to_string(), "close".to_string())];
        for _ in 0..keep_alive_count {
            headers.push(("Connection".to_string(), "keep-alive".to_string()));
        }

        let head = ResponseHead {
            version: "HTTP/1.1".to_string(),
            status_code: 200,
            reason_phrase: "OK".to_string(),
            headers,
        };
        prop_assert!(!head.is_keep_alive());
    }
}

proptest! {
    #[test]
    fn prop_is_keep_alive_multiple_headers_mixed_tokens(
        version in prop_oneof![Just("HTTP/1.0"), Just("HTTP/1.1")],
        close_position in 0..3usize
    ) {
        // 複数のヘッダーに分散した keep-alive と close
        // close がどの位置にあっても false
        let mut headers: Vec<(String, String)> = vec![
            ("Connection".to_string(), "keep-alive".to_string()),
            ("Connection".to_string(), "keep-alive".to_string()),
            ("Connection".to_string(), "keep-alive".to_string()),
        ];
        headers[close_position] = ("Connection".to_string(), "close".to_string());

        let head = ResponseHead {
            version: version.to_string(),
            status_code: 200,
            reason_phrase: "OK".to_string(),
            headers,
        };
        prop_assert!(!head.is_keep_alive());
    }
}

proptest! {
    #[test]
    fn prop_is_keep_alive_multiple_headers_no_connection_token(
        version in prop_oneof![Just("HTTP/1.0"), Just("HTTP/1.1")],
        other_token in "[a-z]{1,8}"
    ) {
        // Connection ヘッダーはあるが keep-alive も close もない場合
        // デフォルト動作（HTTP/1.1 は true、HTTP/1.0 は false）
        let headers = vec![("Connection".to_string(), other_token)];

        let head = ResponseHead {
            version: version.to_string(),
            status_code: 200,
            reason_phrase: "OK".to_string(),
            headers,
        };

        if version == "HTTP/1.1" {
            prop_assert!(head.is_keep_alive());
        } else {
            prop_assert!(!head.is_keep_alive());
        }
    }
}

// ========================================
// HttpHead トレイトメソッドの PBT
// ========================================

proptest! {
    /// 複数 TE ヘッダーの結合処理
    #[test]
    fn prop_is_chunked_multiple_te_headers(count in 2..5usize) {
        // 複数の Transfer-Encoding: chunked ヘッダー → 最後のトークンが chunked → true
        let headers: Vec<(String, String)> = (0..count)
            .map(|_| ("Transfer-Encoding".to_string(), "chunked".to_string()))
            .collect();
        let head = ResponseHead {
            version: "HTTP/1.1".to_string(),
            status_code: 200,
            reason_phrase: "OK".to_string(),
            headers,
        };
        prop_assert!(head.is_chunked());
    }
}

proptest! {
    /// 不正な Content-Length → content_length() は None を返す
    #[test]
    fn prop_content_length_invalid_returns_none(
        invalid_value in "[a-zA-Z]{1,8}"
    ) {
        let head = ResponseHead {
            version: "HTTP/1.1".to_string(),
            status_code: 200,
            reason_phrase: "OK".to_string(),
            headers: vec![
                ("Content-Length".to_string(), invalid_value),
            ],
        };
        prop_assert!(head.content_length().is_none());
    }
}

// ========================================
// ResponseHead ヘルパーメソッドの PBT
// ========================================

proptest! {
    /// is_redirect: 3xx ステータスコードで true
    #[test]
    fn prop_response_head_is_redirect(status in 300u16..=399) {
        let data = format!("HTTP/1.1 {} Redirect\r\n\r\n", status);
        let mut decoder = ResponseDecoder::new();
        decoder.feed(data.as_bytes()).unwrap();
        let (head, _) = decoder.decode_headers().unwrap().unwrap();
        prop_assert!(head.is_redirect());
        prop_assert!(!head.is_success());
        prop_assert!(!head.is_client_error());
        prop_assert!(!head.is_server_error());
    }
}

proptest! {
    /// is_client_error: 4xx ステータスコードで true
    #[test]
    fn prop_response_head_is_client_error(status in 400u16..=451) {
        let data = format!("HTTP/1.1 {} Error\r\n\r\n", status);
        let mut decoder = ResponseDecoder::new();
        decoder.feed(data.as_bytes()).unwrap();
        let (head, _) = decoder.decode_headers().unwrap().unwrap();
        prop_assert!(head.is_client_error());
        prop_assert!(!head.is_success());
        prop_assert!(!head.is_redirect());
        prop_assert!(!head.is_server_error());
    }
}

proptest! {
    /// is_server_error: 5xx ステータスコードで true
    #[test]
    fn prop_response_head_is_server_error(status in 500u16..=511) {
        let data = format!("HTTP/1.1 {} Error\r\n\r\n", status);
        let mut decoder = ResponseDecoder::new();
        decoder.feed(data.as_bytes()).unwrap();
        let (head, _) = decoder.decode_headers().unwrap().unwrap();
        prop_assert!(head.is_server_error());
        prop_assert!(!head.is_success());
        prop_assert!(!head.is_redirect());
        prop_assert!(!head.is_client_error());
    }
}

proptest! {
    /// has_header / connection メソッドの検証
    #[test]
    fn prop_response_head_has_header_and_connection(
        conn_value in prop_oneof![Just("keep-alive"), Just("close")]
    ) {
        let data = format!(
            "HTTP/1.1 200 OK\r\nConnection: {}\r\nX-Custom: test\r\n\r\n",
            conn_value
        );
        let mut decoder = ResponseDecoder::new();
        decoder.feed(data.as_bytes()).unwrap();
        let (head, _) = decoder.decode_headers().unwrap().unwrap();
        prop_assert!(head.has_header("Connection"));
        prop_assert!(head.has_header("X-Custom"));
        prop_assert!(!head.has_header("X-Missing"));
        prop_assert_eq!(head.connection().unwrap(), conn_value);
    }
}

proptest! {
    /// RequestHead の version() メソッド
    #[test]
    fn prop_request_head_version(
        version in prop_oneof![Just("HTTP/1.0"), Just("HTTP/1.1")]
    ) {
        let data = format!("GET / {}\r\nHost: localhost\r\n\r\n", version);
        let mut decoder = RequestDecoder::new();
        decoder.feed(data.as_bytes()).unwrap();
        let (head, _) = decoder.decode_headers().unwrap().unwrap();
        prop_assert_eq!(head.version(), version);
    }
}
