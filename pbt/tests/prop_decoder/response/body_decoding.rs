//! ResponseDecoder のボディデコード関連プロパティテスト
//!
//! chunked / Content-Length / close-delimited / None / Tunnel の各 BodyKind と
//! CONNECT トンネル、1xx の TE 無視、Content-Length のカンマ区切り値などの
//! ボディ解釈ロジックを対象にする。

use proptest::prelude::*;
use shiguredo_http11::{BodyKind, BodyProgress, Error, HttpHead, Response, ResponseDecoder};

use crate::{body, reason_phrase, status_code};

// ========================================
// UTF-8 エラー PBT (チャンクサイズ)
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
        // 不完全なトレーラーは Complete に到達してはならない
        let len = body_content.len();
        let data = format!(
            "HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n{:x}\r\n{}\r\n0\r\n{}: value",
            len, body_content, trailer_name
        );
        let mut decoder = ResponseDecoder::new();
        decoder.feed(data.as_bytes()).unwrap();
        let (_, _) = decoder.decode_headers().unwrap().unwrap();

        // ボディを消費。多段遷移を経て最終的に NeedData (もしくは Complete) に収束する。
        // 不完全トレーラの場合は Complete に到達してはならない。
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
// Content-Length / Transfer-Encoding なしのボディ判定
// ========================================

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
// decode_headers 前の consume_body はエラー
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
// Response ラウンドトリップ PBT
// ========================================

proptest! {
    #[test]
    fn prop_response_roundtrip(
        status in status_code(),
        reason in reason_phrase(),
        body_data in body()
    ) {
        let mut response = Response::new(status, &reason).unwrap();

        // RFC 9110: 1xx/204/205/304 はエンコーダー側でボディ生成を禁止
        // (デコーダー側では 205 はメッセージ長決定規則に従うが、ラウンドトリップテストでは
        //  エンコーダーの制約に合わせる)
        let status_forbids_body = (100..200).contains(&status)
            || status == 204
            || status == 205
            || status == 304;

        if status_forbids_body {
            // 205 は status_has_body=true かつボディ禁止のため、
            // close-delimited を避けるには Content-Length: 0 を明示する必要がある。
            // 1xx/204/304 は status_has_body=false なので body 設定不要。
            if status == 205 {
                response = response.body(Vec::new());
            }
        } else {
            // body=None のままだと close-delimited になるため、空でも明示的に
            // body() を呼んで Content-Length: 0 を付与する。
            response = response.body(body_data.clone());
        }

        let encoded = response.encode().unwrap();
        let mut decoder = ResponseDecoder::new();
        decoder.feed(&encoded).unwrap();
        let decoded = decoder.decode().unwrap().unwrap();

        prop_assert_eq!(decoded.status_code(), status);
        if !status_forbids_body {
            // 空ボディも .body(vec![]) で明示しているため、デコーダーは Some(vec![]) を返す。
            prop_assert_eq!(decoded.body_bytes(), Some(body_data.as_slice()));
        }
    }
}

// ========================================
// CONNECT トンネルモードの PBT
// ========================================

proptest! {
    /// CONNECT + 2xx (204 を除く) の全ステータスコードでトンネルモードになることを確認
    ///
    /// 204 は除外する: RFC 9112 Section 6.3 の "in order of precedence" により
    /// item 1 (1xx/204/304 はボディなし) が item 2 (CONNECT 2xx はトンネル) より
    /// 優先されるため、CONNECT + 204 は `BodyKind::None` になる。
    #[test]
    fn prop_connect_all_2xx_tunnel(status in 200u16..300u16) {
        prop_assume!(status != 204);
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
        prop_assert_eq!(result.1, BodyKind::ContentLength(len as u64));
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
// トンネルモードの PBT (decode エラー)
// ========================================

proptest! {
    /// CONNECT 2xx (204 を除く) 後に decode() → エラー
    ///
    /// 204 は除外する: RFC 9112 Section 6.3 の "in order of precedence" により
    /// CONNECT + 204 は `BodyKind::None` になり、`decode()` はエラーにならない。
    #[test]
    fn prop_response_decode_tunnel_error(status in 200u16..300) {
        prop_assume!(status != 204);
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

// ========================================
// CONNECT 2xx で Transfer-Encoding / Content-Length が ResponseHead から消える (issue 0045)
// ========================================

proptest! {
    /// CONNECT への 2xx レスポンスでは Transfer-Encoding / Content-Length が
    /// ResponseHead.headers から消去される (RFC 9110 Section 9.3.6 MUST ignore)
    ///
    /// RFC 9112 Section 6.3 の precedence により item 1 (1xx/204/304 はボディなし) が
    /// item 2 (CONNECT 2xx は Tunnel) より優先されるため、204 は範囲から除外する
    /// (CONNECT + 204 は `BodyKind::None`)。
    #[test]
    fn prop_connect_2xx_drops_te_cl_from_head(
        status in prop_oneof![200u16..204, 205u16..300],
        cl in 0u64..1_000_000,
    ) {
        let response = format!(
            "HTTP/1.1 {} OK\r\nTransfer-Encoding: chunked\r\nContent-Length: {}\r\n\r\n",
            status, cl
        );
        let mut decoder = ResponseDecoder::new();
        decoder.set_request_method("CONNECT");
        decoder.feed(response.as_bytes()).unwrap();
        let (head, body_kind) = decoder.decode_headers().unwrap().unwrap();
        prop_assert_eq!(body_kind, BodyKind::Tunnel);
        prop_assert_eq!(head.get_header("Transfer-Encoding"), None);
        prop_assert_eq!(head.get_header("Content-Length"), None);
        prop_assert!(!head.is_chunked());
        prop_assert_eq!(head.content_length().unwrap(), None);
    }
}

proptest! {
    /// CONNECT への 1xx / 3xx / 4xx / 5xx レスポンスでは
    /// Content-Length が ResponseHead.headers に残る (Tunnel に遷移しないため)
    #[test]
    fn prop_connect_non_2xx_keeps_cl_in_head(
        status in prop_oneof![300u16..400, 400u16..500, 500u16..600],
        cl in 0u64..1_000,
    ) {
        let body = "x".repeat(cl as usize);
        let response = format!(
            "HTTP/1.1 {} Some\r\nContent-Length: {}\r\n\r\n{}",
            status, cl, body
        );
        let mut decoder = ResponseDecoder::new();
        decoder.set_request_method("CONNECT");
        decoder.feed(response.as_bytes()).unwrap();
        let (head, body_kind) = decoder.decode_headers().unwrap().unwrap();
        // status_has_body 系のロジックは status_code 単位で判定されるため、ここでは
        // CL がそのまま残っていることだけを検証する。
        let _ = body_kind;
        let cl_str = cl.to_string();
        prop_assert_eq!(head.get_header("Content-Length"), Some(cl_str.as_str()));
    }
}
