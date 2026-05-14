//! ResponseDecoder のステータス行関連プロパティテスト

use proptest::prelude::*;
use shiguredo_http11::{BodyKind, ResponseDecoder, StatusClass};

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
        prop_assert_eq!(head.status_code(), status_code);
        prop_assert_eq!(head.reason_phrase(), "");
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
        decoder.set_request_method("HEAD");
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
        decoder.set_request_method("HEAD");
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
        prop_assert_eq!(head.status_class(), StatusClass::Informational);
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
            prop_assert_eq!(head.status_class(), StatusClass::Successful);
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
// UTF-8 エラー PBT (ステータス行 / ヘッダー)
// ========================================

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
// HTTP/1.1 以外で Transfer-Encoding 受理を拒否 (issue 0046)
// ========================================

proptest! {
    /// HTTP/1.1 完全一致以外のレスポンスで Transfer-Encoding は reject される
    #[test]
    fn prop_response_te_rejected_for_non_http11(
        version in prop_oneof![
            Just("HTTP/0.9".to_string()),
            Just("HTTP/1.0".to_string()),
            Just("HTTP/2.0".to_string()),
            Just("HTTP/3.0".to_string()),
            Just("RTSP/1.0".to_string()),
            Just("RTSP/2.0".to_string()),
            Just("FOO/1.0".to_string()),
        ]
    ) {
        let data = format!(
            "{} 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n",
            version
        );
        let mut decoder = ResponseDecoder::new();
        decoder.feed(data.as_bytes()).unwrap();
        let result = decoder.decode_headers();
        prop_assert!(result.is_err());
    }
}

proptest! {
    /// HTTP/1.1 のレスポンスで Transfer-Encoding: chunked は引き続き受理される
    #[test]
    fn prop_response_te_accepted_for_http11(_dummy in 0u8..1) {
        let data = b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n";
        let mut decoder = ResponseDecoder::new();
        decoder.feed(data).unwrap();
        let (_, body_kind) = decoder.decode_headers().unwrap().unwrap();
        prop_assert!(matches!(body_kind, BodyKind::Chunked));
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
