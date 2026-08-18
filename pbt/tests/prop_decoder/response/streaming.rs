//! ResponseDecoder のストリーミング / 状態管理関連プロパティテスト
//!
//! Keep-Alive / パイプライン / close-delimited ストリーミング / mark_eof / reset /
//! `feed` と `mut_buf` + `advance_buf` の等価性などを対象にする。

use shiguredo_http11::{BodyKind, HttpHead, Response, ResponseDecoder};

use crate::{body, status_code};

/// ボディを持つ 2xx ステータスコード (204 を除く) を生成する
///
/// 204, 304 はボディなしなので除外 (2xx のうちボディがあるステータスコードのみ)
fn body_status_code(ctx: &mut noprop::TestCaseContext) -> u16 {
    match noprop::sample_usize_in(ctx, 0..2) {
        0 => noprop::sample_usize_in(ctx, 200..=203) as u16,
        _ => noprop::sample_usize_in(ctx, 205..=299) as u16,
    }
}

/// `[a-z]` で構成された長さ `len_range` の文字列を生成する
fn lower_string(
    ctx: &mut noprop::TestCaseContext,
    len_range: std::ops::RangeInclusive<usize>,
) -> String {
    let len = noprop::sample_usize_in(ctx, len_range);
    let mut s = String::with_capacity(len);
    for _ in 0..len {
        s.push(char::from(b'a' + noprop::sample_usize_in(ctx, 0..26) as u8));
    }
    s
}

// ========================================
// remaining / reset 系
// ========================================

#[test]
fn prop_response_decoder_remaining() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let data_len = noprop::sample_usize_in(ctx, 10..100);
        let mut decoder = ResponseDecoder::new();
        let data = "x".repeat(data_len);
        decoder
            .feed(data.as_bytes())
            .expect("レスポンスのデコードは成功するはず (実装バグ)");
        assert_eq!(decoder.remaining().len(), data_len);
        Ok(())
    })?;

    // ジェネレータは valid-by-construction であり、ケース棄却が発生しないことの検証
    assert_eq!(
        runner.stats().rejected_cases,
        0,
        "ジェネレータが valid-by-construction であること\n{runner}"
    );
    Ok(())
}

#[test]
fn prop_response_decoder_reset() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        // 204, 304 はボディなしなので除外 (2xx のうちボディがあるステータスコードのみ)
        let status_code = body_status_code(ctx);
        let mut decoder = ResponseDecoder::new();
        let data = format!(
            "HTTP/1.1 {} OK\r\nContent-Length: 5\r\n\r\nhello",
            status_code
        );
        decoder
            .feed(data.as_bytes())
            .expect("レスポンスのデコードは成功するはず (実装バグ)");
        let _ = decoder
            .decode_headers()
            .expect("結果は存在するはず (実装バグ)")
            .expect("レスポンスのデコードは成功するはず (実装バグ)");
        decoder.reset();
        assert_eq!(decoder.remaining().len(), 0);
        Ok(())
    })?;

    // ジェネレータは valid-by-construction であり、ケース棄却が発生しないことの検証
    assert_eq!(
        runner.stats().rejected_cases,
        0,
        "ジェネレータが valid-by-construction であること\n{runner}"
    );
    Ok(())
}

#[test]
fn prop_response_decoder_reset_request_method() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        // 204, 304 はボディなしなので除外 (2xx のうちボディがあるステータスコードのみ)
        let status_code = body_status_code(ctx);
        let mut decoder = ResponseDecoder::new();
        decoder.set_request_method("HEAD");
        let data = format!("HTTP/1.1 {} OK\r\nContent-Length: 100\r\n\r\n", status_code);
        decoder
            .feed(data.as_bytes())
            .expect("レスポンスのデコードは成功するはず (実装バグ)");
        let (_, body_kind) = decoder
            .decode_headers()
            .expect("結果は存在するはず (実装バグ)")
            .expect("レスポンスのデコードは成功するはず (実装バグ)");
        assert_eq!(body_kind, BodyKind::None);
        decoder.reset();
        // reset 後は request_method がクリアされる
        let data2 = format!(
            "HTTP/1.1 {} OK\r\nContent-Length: 5\r\n\r\nhello",
            status_code
        );
        decoder
            .feed(data2.as_bytes())
            .expect("レスポンスのデコードは成功するはず (実装バグ)");
        let (_, body_kind2) = decoder
            .decode_headers()
            .expect("結果は存在するはず (実装バグ)")
            .expect("レスポンスのデコードは成功するはず (実装バグ)");
        assert_eq!(body_kind2, BodyKind::ContentLength(5));
        Ok(())
    })?;

    // ジェネレータは valid-by-construction であり、ケース棄却が発生しないことの検証
    assert_eq!(
        runner.stats().rejected_cases,
        0,
        "ジェネレータが valid-by-construction であること\n{runner}"
    );
    Ok(())
}

#[test]
fn prop_head_request_method_cleared_on_decode_headers_complete() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        // 200..=299 のうちボディがあるステータスコード (204 は status_has_body=false で除外)
        let status_code = body_status_code(ctx);
        // set_request_method("HEAD") + 空ボディレスポンスを decode_headers() で
        // 処理した後、続けて通常のレスポンスを decode_headers() で処理した場合に
        // request_method が Complete 遷移時にクリアされていることを検証する。
        let mut decoder = ResponseDecoder::new();
        decoder.set_request_method("HEAD");
        let data1 = format!("HTTP/1.1 {} OK\r\nContent-Length: 0\r\n\r\n", status_code);
        decoder
            .feed(data1.as_bytes())
            .expect("レスポンスのデコードは成功するはず (実装バグ)");
        let (_, body_kind1) = decoder
            .decode_headers()
            .expect("結果は存在するはず (実装バグ)")
            .expect("レスポンスのデコードは成功するはず (実装バグ)");
        assert_eq!(body_kind1, BodyKind::None);

        // 次のレスポンスを供給する。Complete 遷移時に request_method がクリア
        // されていれば、Content-Length: 5 が正しく解釈されるはず。
        let data2 = format!(
            "HTTP/1.1 {} OK\r\nContent-Length: 5\r\n\r\nhello",
            status_code
        );
        decoder
            .feed(data2.as_bytes())
            .expect("レスポンスのデコードは成功するはず (実装バグ)");
        let (_, body_kind2) = decoder
            .decode_headers()
            .expect("結果は存在するはず (実装バグ)")
            .expect("レスポンスのデコードは成功するはず (実装バグ)");
        assert_eq!(body_kind2, BodyKind::ContentLength(5));
        Ok(())
    })?;

    // ジェネレータは valid-by-construction であり、ケース棄却が発生しないことの検証
    assert_eq!(
        runner.stats().rejected_cases,
        0,
        "ジェネレータが valid-by-construction であること\n{runner}"
    );
    Ok(())
}

#[test]
fn prop_head_request_method_cleared_on_decode_complete() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        // 200..=299 のうちボディがあるステータスコード (204 は status_has_body=false で除外)
        let status_code = body_status_code(ctx);
        // set_request_method("HEAD") + 空ボディレスポンスを decode() で処理した後、
        // 続けて通常のレスポンスを decode() で処理した場合に request_method が
        // decode() 完了時にクリアされていることを検証する。
        let mut decoder = ResponseDecoder::new();
        decoder.set_request_method("HEAD");
        let data1 = format!("HTTP/1.1 {} OK\r\nContent-Length: 0\r\n\r\n", status_code);
        decoder
            .feed(data1.as_bytes())
            .expect("レスポンスのデコードは成功するはず (実装バグ)");
        let resp1 = decoder
            .decode()
            .expect("結果は存在するはず (実装バグ)")
            .expect("レスポンスのデコードは成功するはず (実装バグ)");
        assert_eq!(resp1.status_code(), status_code);
        // HEAD レスポンスはボディなし扱い
        assert!(resp1.body_bytes().is_none());

        // 次のレスポンスを供給する。decode() 完了時に request_method がクリア
        // されていれば、Content-Length: 5 が正しく解釈されてボディが取れるはず。
        let data2 = format!(
            "HTTP/1.1 {} OK\r\nContent-Length: 5\r\n\r\nhello",
            status_code
        );
        decoder
            .feed(data2.as_bytes())
            .expect("レスポンスのデコードは成功するはず (実装バグ)");
        let resp2 = decoder
            .decode()
            .expect("結果は存在するはず (実装バグ)")
            .expect("レスポンスのデコードは成功するはず (実装バグ)");
        assert_eq!(resp2.body_bytes(), Some(&b"hello"[..]));
        Ok(())
    })?;

    // ジェネレータは valid-by-construction であり、ケース棄却が発生しないことの検証
    assert_eq!(
        runner.stats().rejected_cases,
        0,
        "ジェネレータが valid-by-construction であること\n{runner}"
    );
    Ok(())
}

// ========================================
// 複数レスポンス PBT
// ========================================

#[test]
fn prop_multiple_responses_same_decoder() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let code_count = noprop::sample_usize_in(ctx, 2..5);
        let mut status_codes = Vec::new();
        for _ in 0..code_count {
            status_codes.push(status_code(ctx));
        }
        let mut decoder = ResponseDecoder::new();

        for code in &status_codes {
            // body == None だと status_has_body 系コード (200/300/...) で
            // close-delimited になり decode() が EOF 待ちになるため、
            // 明示的に空ボディを指定して Content-Length: 0 を確保する。
            // 1xx/204/304 では status_has_body=false により Content-Length は付かないが、
            // body=Some(vec![]) でもエンコーダーは body バイトを出力しないため問題ない。
            let response = Response::new(*code, "OK")
                .expect("レスポンスのデコードは成功するはず (実装バグ)")
                .body(Vec::new());
            let encoded = response
                .encode()
                .expect("レスポンスのデコードは成功するはず (実装バグ)");
            decoder
                .feed(&encoded)
                .expect("レスポンスのデコードは成功するはず (実装バグ)");
            let decoded = decoder
                .decode()
                .expect("結果は存在するはず (実装バグ)")
                .expect("レスポンスのデコードは成功するはず (実装バグ)");
            assert_eq!(decoded.status_code(), *code);
            decoder.reset();
        }
        Ok(())
    })?;

    // ジェネレータは valid-by-construction であり、ケース棄却が発生しないことの検証
    assert_eq!(
        runner.stats().rejected_cases,
        0,
        "ジェネレータが valid-by-construction であること\n{runner}"
    );
    Ok(())
}

// ========================================
// ストリーミング API の PBT (レスポンス)
// ========================================

#[test]
fn prop_streaming_decode_response() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);
    let no_body_seen = std::cell::Cell::new(0usize);
    let body_seen = std::cell::Cell::new(0usize);

    runner.run(256, |ctx| {
        let status_code = status_code(ctx);
        let body_content = lower_string(ctx, 1..=100);
        let mut decoder = ResponseDecoder::new();
        let body_len = body_content.len();
        let data = format!(
            "HTTP/1.1 {} OK\r\nContent-Length: {}\r\n\r\n{}",
            status_code, body_len, body_content
        );
        decoder
            .feed(data.as_bytes())
            .expect("レスポンスのデコードは成功するはず (実装バグ)");
        let (head, body_kind) = decoder
            .decode_headers()
            .expect("結果は存在するはず (実装バグ)")
            .expect("レスポンスのデコードは成功するはず (実装バグ)");
        assert_eq!(head.status_code(), status_code);
        // RFC 9112 Section 6.3: 1xx, 204, 304 はボディなし
        if (100..200).contains(&status_code) || status_code == 204 || status_code == 304 {
            // 1xx/204/304 側の分岐が一度も実行されないと BodyKind::None の検証が空振りする
            no_body_seen.set(no_body_seen.get() + 1);
            assert_eq!(body_kind, BodyKind::None);
        } else {
            body_seen.set(body_seen.get() + 1);
            assert_eq!(body_kind, BodyKind::ContentLength(body_len as u64));
        }
        Ok(())
    })?;

    // p 推定値: status_code() は全 82 値のうち 1xx/204/304 が 4 値なので
    // ボディなし側の確率は 4/82 ≈ 0.049。256 ケースでの miss 確率は
    // (1 - 4/82)^256 ≈ 3e-6。
    assert!(
        no_body_seen.get() > 0,
        "1xx/204/304 (ボディなし) 側が一度も検証されなかった\n{runner}"
    );
    assert!(
        body_seen.get() > 0,
        "ボディあり側が一度も検証されなかった\n{runner}"
    );

    // ジェネレータは valid-by-construction であり、ケース棄却が発生しないことの検証
    assert_eq!(
        runner.stats().rejected_cases,
        0,
        "ジェネレータが valid-by-construction であること\n{runner}"
    );
    Ok(())
}

// ========================================
// decode() API の連続デコードテスト (Keep-Alive) PBT (レスポンス)
// ========================================

#[test]
fn prop_decode_multiple_responses_keep_alive() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let code_count = noprop::sample_usize_in(ctx, 2..5);
        let mut status_codes = Vec::new();
        for _ in 0..code_count {
            status_codes.push(status_code(ctx));
        }
        let mut decoder = ResponseDecoder::new();

        // 全レスポンスを一度にバッファに入れる
        // body == None だと status_has_body 系コードで close-delimited になるため、
        // 明示的に空ボディを指定する。
        let mut all_data = Vec::new();
        for code in &status_codes {
            let response = Response::new(*code, "OK")
                .expect("レスポンスのデコードは成功するはず (実装バグ)")
                .body(Vec::new());
            all_data.extend(
                response
                    .encode()
                    .expect("レスポンスのデコードは成功するはず (実装バグ)"),
            );
        }
        decoder
            .feed(&all_data)
            .expect("レスポンスのデコードは成功するはず (実装バグ)");

        // decode() を連続して呼ぶ（reset() なし）
        for code in &status_codes {
            let response = decoder
                .decode()
                .expect("結果は存在するはず (実装バグ)")
                .expect("レスポンスのデコードは成功するはず (実装バグ)");
            assert_eq!(response.status_code(), *code);
        }
        Ok(())
    })?;

    // ジェネレータは valid-by-construction であり、ケース棄却が発生しないことの検証
    assert_eq!(
        runner.stats().rejected_cases,
        0,
        "ジェネレータが valid-by-construction であること\n{runner}"
    );
    Ok(())
}

// ========================================
// decode_headers の Complete → StartLine 遷移 PBT (レスポンス)
// ========================================

#[test]
fn prop_response_decode_headers_multiple_no_body_messages() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        // 複数のボディなしレスポンスを decode_headers で連続処理
        let count = noprop::sample_usize_in(ctx, 2..5);
        let base_status = noprop::sample_usize_in(ctx, 200..400) as u16;
        let mut decoder = ResponseDecoder::new();
        for i in 0..count {
            let status = base_status + i as u16;
            let data = format!("HTTP/1.1 {} OK\r\nContent-Length: 0\r\n\r\n", status);
            decoder
                .feed(data.as_bytes())
                .expect("レスポンスのデコードは成功するはず (実装バグ)");
        }

        for i in 0..count {
            let (head, _) = decoder
                .decode_headers()
                .expect("結果は存在するはず (実装バグ)")
                .expect("レスポンスのデコードは成功するはず (実装バグ)");
            assert_eq!(head.status_code(), base_status + i as u16);
        }

        // 次のメッセージがなければ Ok(None)
        assert!(
            decoder
                .decode_headers()
                .expect("レスポンスのデコードは成功するはず (実装バグ)")
                .is_none()
        );
        Ok(())
    })?;

    // ジェネレータは valid-by-construction であり、ケース棄却が発生しないことの検証
    assert_eq!(
        runner.stats().rejected_cases,
        0,
        "ジェネレータが valid-by-construction であること\n{runner}"
    );
    Ok(())
}

// ========================================
// close-delimited ボディ + mark_eof
// ========================================

/// close-delimited ボディの decode() + mark_eof() ラウンドトリップ
#[test]
fn prop_response_decode_close_delimited_with_mark_eof() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let body_len = noprop::sample_usize_in(ctx, 1..256);
        let body_data = noprop::sample_bytes_vec(ctx, body_len);
        let mut decoder = ResponseDecoder::new();
        decoder
            .feed(b"HTTP/1.1 200 OK\r\n\r\n")
            .expect("レスポンスのデコードは成功するはず (実装バグ)");
        decoder
            .feed(&body_data)
            .expect("レスポンスのデコードは成功するはず (実装バグ)");

        // mark_eof() 前は None
        let result = decoder
            .decode()
            .expect("レスポンスのデコードは成功するはず (実装バグ)");
        assert!(result.is_none());

        // mark_eof() 後に decode() で取得可能
        decoder.mark_eof();
        let response = decoder
            .decode()
            .expect("結果は存在するはず (実装バグ)")
            .expect("レスポンスのデコードは成功するはず (実装バグ)");
        assert_eq!(response.status_code(), 200);
        assert_eq!(response.body_bytes(), Some(body_data.as_slice()));
        Ok(())
    })?;

    // ジェネレータは valid-by-construction であり、ケース棄却が発生しないことの検証
    assert_eq!(
        runner.stats().rejected_cases,
        0,
        "ジェネレータが valid-by-construction であること\n{runner}"
    );
    Ok(())
}

/// mark_eof() 前の close-delimited は常に None を返す
#[test]
fn prop_response_decode_close_delimited_returns_none_before_eof() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let body_len = noprop::sample_usize_in(ctx, 0..=256);
        let body_data = noprop::sample_bytes_vec(ctx, body_len);
        let mut decoder = ResponseDecoder::new();
        decoder
            .feed(b"HTTP/1.1 200 OK\r\n\r\n")
            .expect("レスポンスのデコードは成功するはず (実装バグ)");
        decoder
            .feed(&body_data)
            .expect("レスポンスのデコードは成功するはず (実装バグ)");

        // mark_eof() を呼ばずに decode() → None
        let result = decoder
            .decode()
            .expect("レスポンスのデコードは成功するはず (実装バグ)");
        assert!(result.is_none());

        // 追加データを feed しても None
        decoder
            .feed(b"more data")
            .expect("レスポンスのデコードは成功するはず (実装バグ)");
        let result = decoder
            .decode()
            .expect("レスポンスのデコードは成功するはず (実装バグ)");
        assert!(result.is_none());
        Ok(())
    })?;

    // ジェネレータは valid-by-construction であり、ケース棄却が発生しないことの検証
    assert_eq!(
        runner.stats().rejected_cases,
        0,
        "ジェネレータが valid-by-construction であること\n{runner}"
    );
    Ok(())
}

/// is_close_delimited() の状態確認
#[test]
fn prop_response_is_close_delimited() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let body_len = noprop::sample_usize_in(ctx, 0..=64);
        let body_data = noprop::sample_bytes_vec(ctx, body_len);
        let mut decoder = ResponseDecoder::new();
        decoder
            .feed(b"HTTP/1.1 200 OK\r\n\r\n")
            .expect("レスポンスのデコードは成功するはず (実装バグ)");
        decoder
            .feed(&body_data)
            .expect("レスポンスのデコードは成功するはず (実装バグ)");

        let (_, body_kind) = decoder
            .decode_headers()
            .expect("結果は存在するはず (実装バグ)")
            .expect("レスポンスのデコードは成功するはず (実装バグ)");
        assert_eq!(body_kind, BodyKind::CloseDelimited);
        assert!(decoder.is_close_delimited());

        // mark_eof() 後は false
        decoder.mark_eof();
        assert!(!decoder.is_close_delimited());
        Ok(())
    })?;

    // ジェネレータは valid-by-construction であり、ケース棄却が発生しないことの検証
    assert_eq!(
        runner.stats().rejected_cases,
        0,
        "ジェネレータが valid-by-construction であること\n{runner}"
    );
    Ok(())
}

/// トンネルモード後の take_remaining()
#[test]
fn prop_response_take_remaining_tunnel() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let extra_len = noprop::sample_usize_in(ctx, 1..128);
        let extra_data = noprop::sample_bytes_vec(ctx, extra_len);
        let mut decoder = ResponseDecoder::new();
        decoder.set_request_method("CONNECT");

        let mut response = b"HTTP/1.1 200 OK\r\n\r\n".to_vec();
        response.extend_from_slice(&extra_data);
        decoder
            .feed(&response)
            .expect("レスポンスのデコードは成功するはず (実装バグ)");

        let (_, body_kind) = decoder
            .decode_headers()
            .expect("結果は存在するはず (実装バグ)")
            .expect("レスポンスのデコードは成功するはず (実装バグ)");
        assert_eq!(body_kind, BodyKind::Tunnel);
        assert!(decoder.is_tunnel());

        let remaining = decoder.take_remaining();
        assert_eq!(&remaining, &extra_data);
        Ok(())
    })?;

    // ジェネレータは valid-by-construction であり、ケース棄却が発生しないことの検証
    assert_eq!(
        runner.stats().rejected_cases,
        0,
        "ジェネレータが valid-by-construction であること\n{runner}"
    );
    Ok(())
}

// ========================================
// close-delimited 段階的フィードの PBT
// ========================================

/// close-delimited を段階的に feed + mark_eof
#[test]
fn prop_response_decode_close_delimited_incremental() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let chunk_count = noprop::sample_usize_in(ctx, 2..5);
        let mut chunks = Vec::new();
        for _ in 0..chunk_count {
            let chunk_len = noprop::sample_usize_in(ctx, 1..64);
            chunks.push(noprop::sample_bytes_vec(ctx, chunk_len));
        }
        let mut decoder = ResponseDecoder::new();
        decoder
            .feed(b"HTTP/1.1 200 OK\r\n\r\n")
            .expect("レスポンスのデコードは成功するはず (実装バグ)");

        // ヘッダーだけで decode → None
        let result = decoder
            .decode()
            .expect("レスポンスのデコードは成功するはず (実装バグ)");
        assert!(result.is_none());

        // 各チャンクを feed して decode (すべて None)
        for chunk in &chunks {
            decoder
                .feed(chunk)
                .expect("レスポンスのデコードは成功するはず (実装バグ)");
            let result = decoder
                .decode()
                .expect("レスポンスのデコードは成功するはず (実装バグ)");
            assert!(result.is_none());
        }

        // mark_eof() 後に decode() で取得
        decoder.mark_eof();
        let response = decoder
            .decode()
            .expect("結果は存在するはず (実装バグ)")
            .expect("レスポンスのデコードは成功するはず (実装バグ)");

        let expected_body: Vec<u8> = chunks.into_iter().flatten().collect();
        assert_eq!(response.body_bytes(), Some(expected_body.as_slice()));
        Ok(())
    })?;

    // ジェネレータは valid-by-construction であり、ケース棄却が発生しないことの検証
    assert_eq!(
        runner.stats().rejected_cases,
        0,
        "ジェネレータが valid-by-construction であること\n{runner}"
    );
    Ok(())
}

/// close-delimited 以外で mark_eof は無視
#[test]
fn prop_response_mark_eof_non_close_delimited() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let body_len = noprop::sample_usize_in(ctx, 1..64);
        let body_data = noprop::sample_bytes_vec(ctx, body_len);
        let headers = format!(
            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\n\r\n",
            body_data.len()
        );
        let mut full = headers.into_bytes();
        full.extend_from_slice(&body_data);

        let mut decoder = ResponseDecoder::new();
        decoder
            .feed(&full)
            .expect("レスポンスのデコードは成功するはず (実装バグ)");

        // mark_eof は Content-Length ボディには影響しない
        decoder.mark_eof();
        assert!(!decoder.is_close_delimited());

        let response = decoder
            .decode()
            .expect("結果は存在するはず (実装バグ)")
            .expect("レスポンスのデコードは成功するはず (実装バグ)");
        assert_eq!(response.body_bytes(), Some(body_data.as_slice()));
        Ok(())
    })?;

    // ジェネレータは valid-by-construction であり、ケース棄却が発生しないことの検証
    assert_eq!(
        runner.stats().rejected_cases,
        0,
        "ジェネレータが valid-by-construction であること\n{runner}"
    );
    Ok(())
}

// ========================================
// 直接書き込み API (mut_buf / advance_buf / available_buf) のプロパティ
// ========================================

/// HTTP メッセージのバイト列を任意のチャンク境界で分割するサイズ列を生成する
fn message_with_chunks(ctx: &mut noprop::TestCaseContext) -> (Vec<u8>, Vec<usize>) {
    let body_data = body(ctx);
    let headers = format!(
        "HTTP/1.1 200 OK\r\nContent-Length: {}\r\n\r\n",
        body_data.len()
    );
    let mut full = headers.into_bytes();
    full.extend_from_slice(&body_data);
    let len = full.len();
    let chunks = if len == 0 {
        Vec::new()
    } else {
        let chunk_count = noprop::sample_usize_in(ctx, 0..=8);
        let mut chunks = Vec::new();
        for _ in 0..chunk_count {
            chunks.push(noprop::sample_usize_in(ctx, 1..=len.max(1)));
        }
        chunks
    };
    (full, chunks)
}

/// `feed` と `mut_buf` + `advance_buf` で同じ結果になることを確認
#[test]
fn prop_feed_mut_buf_equivalence() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let (full, chunk_sizes) = message_with_chunks(ctx);
        let by_feed = {
            let mut decoder = ResponseDecoder::new();
            let mut offset = 0usize;
            for &size in &chunk_sizes {
                if offset >= full.len() {
                    break;
                }
                let end = (offset + size).min(full.len());
                decoder
                    .feed(&full[offset..end])
                    .expect("レスポンスのデコードは成功するはず (実装バグ)");
                offset = end;
            }
            if offset < full.len() {
                decoder
                    .feed(&full[offset..])
                    .expect("レスポンスのデコードは成功するはず (実装バグ)");
            }
            decoder
                .decode()
                .expect("レスポンスのデコードは成功するはず (実装バグ)")
        };

        let by_mut_buf = {
            let mut decoder = ResponseDecoder::new();
            let mut offset = 0usize;
            for &size in &chunk_sizes {
                if offset >= full.len() {
                    break;
                }
                let end = (offset + size).min(full.len());
                let len = end - offset;
                let dst = decoder
                    .mut_buf(len)
                    .expect("レスポンスのデコードは成功するはず (実装バグ)");
                dst.copy_from_slice(&full[offset..end]);
                decoder.advance_buf(len);
                offset = end;
            }
            if offset < full.len() {
                let len = full.len() - offset;
                let dst = decoder
                    .mut_buf(len)
                    .expect("レスポンスのデコードは成功するはず (実装バグ)");
                dst.copy_from_slice(&full[offset..]);
                decoder.advance_buf(len);
            }
            decoder
                .decode()
                .expect("レスポンスのデコードは成功するはず (実装バグ)")
        };

        let by_feed = by_feed.expect("feed 経路で response が得られなかった");
        let by_mut_buf = by_mut_buf.expect("mut_buf 経路で response が得られなかった");
        assert_eq!(by_feed.status_code(), by_mut_buf.status_code());
        assert_eq!(by_feed.reason_phrase(), by_mut_buf.reason_phrase());
        assert_eq!(HttpHead::headers(&by_feed), HttpHead::headers(&by_mut_buf));
        assert_eq!(by_feed.body_bytes(), by_mut_buf.body_bytes());
        Ok(())
    })?;

    // ジェネレータは valid-by-construction であり、ケース棄却が発生しないことの検証
    assert_eq!(
        runner.stats().rejected_cases,
        0,
        "ジェネレータが valid-by-construction であること\n{runner}"
    );
    Ok(())
}

/// `mut_buf(len)` の戻りスライス長は常に `len`
#[test]
fn prop_mut_buf_returns_exact_length() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let len = noprop::sample_usize_in(ctx, 0..4096);
        let mut decoder = ResponseDecoder::new();
        let buf = decoder
            .mut_buf(len)
            .expect("レスポンスのデコードは成功するはず (実装バグ)");
        assert_eq!(buf.len(), len);
        decoder.advance_buf(0);
        Ok(())
    })?;

    // ジェネレータは valid-by-construction であり、ケース棄却が発生しないことの検証
    assert_eq!(
        runner.stats().rejected_cases,
        0,
        "ジェネレータが valid-by-construction であること\n{runner}"
    );
    Ok(())
}

/// `advance_buf(n)` 後の `remaining().len()` は (前回の remaining) + n になる
#[test]
fn prop_advance_buf_grows_remaining() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let prefix_len = noprop::sample_usize_in(ctx, 0..=64);
        let prefix = noprop::sample_bytes_vec(ctx, prefix_len);
        let write_len = noprop::sample_usize_in(ctx, 0..256);
        let advance = noprop::sample_usize_in(ctx, 0..256);
        let advance = advance.min(write_len);
        let mut decoder = ResponseDecoder::new();
        if !prefix.is_empty() {
            decoder
                .feed(&prefix)
                .expect("レスポンスのデコードは成功するはず (実装バグ)");
        }
        let before = decoder.remaining().len();
        let buf = decoder
            .mut_buf(write_len)
            .expect("レスポンスのデコードは成功するはず (実装バグ)");
        for (i, slot) in buf.iter_mut().enumerate() {
            *slot = (i & 0xff) as u8;
        }
        decoder.advance_buf(advance);
        assert_eq!(decoder.remaining().len(), before + advance);
        Ok(())
    })?;

    // ジェネレータは valid-by-construction であり、ケース棄却が発生しないことの検証
    assert_eq!(
        runner.stats().rejected_cases,
        0,
        "ジェネレータが valid-by-construction であること\n{runner}"
    );
    Ok(())
}

/// `mut_buf` 後 `advance_buf(0)` で `remaining()` が `mut_buf` 前と同じになる
#[test]
fn prop_advance_zero_is_identity() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let prefix_len = noprop::sample_usize_in(ctx, 0..=64);
        let prefix = noprop::sample_bytes_vec(ctx, prefix_len);
        let write_len = noprop::sample_usize_in(ctx, 0..256);
        let mut decoder = ResponseDecoder::new();
        if !prefix.is_empty() {
            decoder
                .feed(&prefix)
                .expect("レスポンスのデコードは成功するはず (実装バグ)");
        }
        let before = decoder.remaining().to_vec();
        let _ = decoder
            .mut_buf(write_len)
            .expect("レスポンスのデコードは成功するはず (実装バグ)");
        decoder.advance_buf(0);
        assert_eq!(decoder.remaining(), before.as_slice());
        Ok(())
    })?;

    // ジェネレータは valid-by-construction であり、ケース棄却が発生しないことの検証
    assert_eq!(
        runner.stats().rejected_cases,
        0,
        "ジェネレータが valid-by-construction であること\n{runner}"
    );
    Ok(())
}
