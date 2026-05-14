//! ResponseDecoder のストリーミング / 状態管理関連プロパティテスト
//!
//! Keep-Alive / パイプライン / close-delimited ストリーミング / mark_eof / reset /
//! `feed` と `mut_buf` + `advance_buf` の等価性などを対象にする。

use proptest::prelude::*;
use shiguredo_http11::{BodyKind, HttpHead, Response, ResponseDecoder};

use crate::{body, status_code};

// ========================================
// remaining / reset 系
// ========================================

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
    fn prop_response_decoder_reset_request_method(
        // 204, 304 はボディなしなので除外 (2xx のうちボディがあるステータスコードのみ)
        status_code in prop_oneof![200u16..=203, 205u16..=299]
    ) {
        let mut decoder = ResponseDecoder::new();
        decoder.set_request_method("HEAD");
        let data = format!("HTTP/1.1 {} OK\r\nContent-Length: 100\r\n\r\n", status_code);
        decoder.feed(data.as_bytes()).unwrap();
        let (_, body_kind) = decoder.decode_headers().unwrap().unwrap();
        prop_assert_eq!(body_kind, BodyKind::None);
        decoder.reset();
        // reset 後は request_method がクリアされる
        let data2 = format!("HTTP/1.1 {} OK\r\nContent-Length: 5\r\n\r\nhello", status_code);
        decoder.feed(data2.as_bytes()).unwrap();
        let (_, body_kind2) = decoder.decode_headers().unwrap().unwrap();
        prop_assert_eq!(body_kind2, BodyKind::ContentLength(5));
    }
}

proptest! {
    #[test]
    fn prop_head_request_method_cleared_on_decode_headers_complete(
        // 200..=299 のうちボディがあるステータスコード (204 は status_has_body=false で除外)
        status_code in prop_oneof![200u16..=203, 205u16..=299]
    ) {
        // set_request_method("HEAD") + 空ボディレスポンスを decode_headers() で
        // 処理した後、続けて通常のレスポンスを decode_headers() で処理した場合に
        // request_method が Complete 遷移時にクリアされていることを検証する。
        let mut decoder = ResponseDecoder::new();
        decoder.set_request_method("HEAD");
        let data1 = format!("HTTP/1.1 {} OK\r\nContent-Length: 0\r\n\r\n", status_code);
        decoder.feed(data1.as_bytes()).unwrap();
        let (_, body_kind1) = decoder.decode_headers().unwrap().unwrap();
        prop_assert_eq!(body_kind1, BodyKind::None);

        // 次のレスポンスを供給する。Complete 遷移時に request_method がクリア
        // されていれば、Content-Length: 5 が正しく解釈されるはず。
        let data2 = format!("HTTP/1.1 {} OK\r\nContent-Length: 5\r\n\r\nhello", status_code);
        decoder.feed(data2.as_bytes()).unwrap();
        let (_, body_kind2) = decoder.decode_headers().unwrap().unwrap();
        prop_assert_eq!(body_kind2, BodyKind::ContentLength(5));
    }
}

proptest! {
    #[test]
    fn prop_head_request_method_cleared_on_decode_complete(
        // 200..=299 のうちボディがあるステータスコード (204 は status_has_body=false で除外)
        status_code in prop_oneof![200u16..=203, 205u16..=299]
    ) {
        // set_request_method("HEAD") + 空ボディレスポンスを decode() で処理した後、
        // 続けて通常のレスポンスを decode() で処理した場合に request_method が
        // decode() 完了時にクリアされていることを検証する。
        let mut decoder = ResponseDecoder::new();
        decoder.set_request_method("HEAD");
        let data1 = format!("HTTP/1.1 {} OK\r\nContent-Length: 0\r\n\r\n", status_code);
        decoder.feed(data1.as_bytes()).unwrap();
        let resp1 = decoder.decode().unwrap().unwrap();
        prop_assert_eq!(resp1.status_code(), status_code);
        // HEAD レスポンスはボディなし扱い
        prop_assert!(resp1.body_bytes().is_none());

        // 次のレスポンスを供給する。decode() 完了時に request_method がクリア
        // されていれば、Content-Length: 5 が正しく解釈されてボディが取れるはず。
        let data2 = format!("HTTP/1.1 {} OK\r\nContent-Length: 5\r\n\r\nhello", status_code);
        decoder.feed(data2.as_bytes()).unwrap();
        let resp2 = decoder.decode().unwrap().unwrap();
        prop_assert_eq!(resp2.body_bytes(), Some(&b"hello"[..]));
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
            // body == None だと status_has_body 系コード (200/300/...) で
            // close-delimited になり decode() が EOF 待ちになるため、
            // 明示的に空ボディを指定して Content-Length: 0 を確保する。
            // 1xx/204/304 では status_has_body=false により Content-Length は付かないが、
            // body=Some(vec![]) でもエンコーダーは body バイトを出力しないため問題ない。
            let response = Response::new(*code, "OK").unwrap().body(Vec::new());
            let encoded = response.encode().unwrap();
            decoder.feed(&encoded).unwrap();
            let decoded = decoder.decode().unwrap().unwrap();
            prop_assert_eq!(decoded.status_code(), *code);
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
        prop_assert_eq!(head.status_code(), status_code);
        // RFC 9112 Section 6.3: 1xx, 204, 304 はボディなし
        if (100..200).contains(&status_code) || status_code == 204 || status_code == 304 {
            prop_assert_eq!(body_kind, BodyKind::None);
        } else {
            prop_assert_eq!(body_kind, BodyKind::ContentLength(body_len as u64));
        }
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
        // body == None だと status_has_body 系コードで close-delimited になるため、
        // 明示的に空ボディを指定する。
        let mut all_data = Vec::new();
        for code in &status_codes {
            let response = Response::new(*code, "OK").unwrap().body(Vec::new());
            all_data.extend(response.encode().unwrap());
        }
        decoder.feed(&all_data).unwrap();

        // decode() を連続して呼ぶ（reset() なし）
        for code in &status_codes {
            let response = decoder.decode().unwrap().unwrap();
            prop_assert_eq!(response.status_code(), *code);
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
            prop_assert_eq!(head.status_code(), base_status + i as u16);
        }

        // 次のメッセージがなければ Ok(None)
        prop_assert!(decoder.decode_headers().unwrap().is_none());
    }
}

// ========================================
// close-delimited ボディ + mark_eof
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
        prop_assert_eq!(response.status_code(), 200);
        prop_assert_eq!(response.body_bytes(), Some(body_data.as_slice()));
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
        prop_assert_eq!(response.body_bytes(), Some(expected_body.as_slice()));
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
        prop_assert_eq!(response.body_bytes(), Some(body_data.as_slice()));
    }
}

// ========================================
// 直接書き込み API (mut_buf / advance_buf / available_buf) のプロパティ
// ========================================

/// HTTP メッセージのバイト列を任意のチャンク境界で分割する Strategy
fn message_with_chunks() -> impl Strategy<Value = (Vec<u8>, Vec<usize>)> {
    body().prop_flat_map(|body_data| {
        let headers = format!(
            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\n\r\n",
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
            let mut decoder = ResponseDecoder::new();
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
            let mut decoder = ResponseDecoder::new();
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

        let by_feed = by_feed.expect("feed 経路で response が得られなかった");
        let by_mut_buf = by_mut_buf.expect("mut_buf 経路で response が得られなかった");
        prop_assert_eq!(by_feed.status_code(), by_mut_buf.status_code());
        prop_assert_eq!(by_feed.reason_phrase(), by_mut_buf.reason_phrase());
        prop_assert_eq!(HttpHead::headers(&by_feed), HttpHead::headers(&by_mut_buf));
        prop_assert_eq!(by_feed.body_bytes(), by_mut_buf.body_bytes());
    }
}

proptest! {
    /// `mut_buf(len)` の戻りスライス長は常に `len`
    #[test]
    fn prop_mut_buf_returns_exact_length(len in 0usize..4096) {
        let mut decoder = ResponseDecoder::new();
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
        let mut decoder = ResponseDecoder::new();
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
        let mut decoder = ResponseDecoder::new();
        if !prefix.is_empty() {
            decoder.feed(&prefix).unwrap();
        }
        let before = decoder.remaining().to_vec();
        let _ = decoder.mut_buf(write_len).unwrap();
        decoder.advance_buf(0);
        prop_assert_eq!(decoder.remaining(), before.as_slice());
    }
}
