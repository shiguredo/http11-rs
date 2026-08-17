//! ResponseDecoder のリミット関連プロパティテスト
//!
//! `DecoderLimits` の各上限 (buffer / header line / headers count / body size) を
//! 超過した場合のエラーパスと、`limits()` ゲッターの動作を対象にする。

use shiguredo_http11::{BodyKind, DecoderLimits, Error, ResponseDecoder};

// ========================================
// デコーダーリミット PBT (レスポンス)
// ========================================

#[test]
fn prop_response_decoder_buffer_overflow() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let data_size = noprop::sample_usize_in(ctx, 1000..2000);
        let limits = DecoderLimits {
            max_buffer_size: 100,
            ..DecoderLimits::default()
        };
        let mut decoder = ResponseDecoder::with_limits(limits);
        let data = "x".repeat(data_size);
        let result = decoder.feed(data.as_bytes());
        let is_buffer_overflow = matches!(result, Err(Error::BufferOverflow { .. }));
        assert!(
            is_buffer_overflow,
            "BufferOverflow を期待したが {:?} だった",
            result
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

#[test]
fn prop_response_decoder_header_line_too_long() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let header_value_len = noprop::sample_usize_in(ctx, 200..500);
        let limits = DecoderLimits {
            max_header_line_size: 100,
            ..DecoderLimits::default()
        };
        let mut decoder = ResponseDecoder::with_limits(limits);
        let header_value = "x".repeat(header_value_len);
        let data = format!("HTTP/1.1 200 OK\r\nX-Long: {}\r\n\r\n", header_value);
        decoder
            .feed(data.as_bytes())
            .expect("レスポンスのデコードは成功するはず (実装バグ)");
        let result = decoder.decode_headers();
        let is_header_line_too_long = matches!(result, Err(Error::HeaderLineTooLong { .. }));
        assert!(
            is_header_line_too_long,
            "HeaderLineTooLong を期待したが {:?} だった",
            result
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

#[test]
fn prop_response_decoder_too_many_headers() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let header_count = noprop::sample_usize_in(ctx, 20..50);
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
        decoder
            .feed(data.as_bytes())
            .expect("レスポンスのデコードは成功するはず (実装バグ)");
        let result = decoder.decode_headers();
        let is_too_many_headers = matches!(result, Err(Error::TooManyHeaders { .. }));
        assert!(
            is_too_many_headers,
            "TooManyHeaders を期待したが {:?} だった",
            result
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

#[test]
fn prop_response_decoder_body_too_large_content_length() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let body_size = noprop::sample_usize_in(ctx, 200..500);
        let limits = DecoderLimits {
            max_body_size: 100,
            ..DecoderLimits::default()
        };
        let mut decoder = ResponseDecoder::with_limits(limits);
        let body = "x".repeat(body_size);
        let data = format!(
            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\n\r\n{}",
            body_size, body
        );
        decoder
            .feed(data.as_bytes())
            .expect("レスポンスのデコードは成功するはず (実装バグ)");
        let result = decoder.decode_headers();
        let is_body_too_large = matches!(result, Err(Error::BodyTooLarge { .. }));
        assert!(
            is_body_too_large,
            "BodyTooLarge を期待したが {:?} だった",
            result
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

#[test]
fn prop_response_decoder_body_too_large_chunked() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let chunk_size = noprop::sample_usize_in(ctx, 200..500);
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
        decoder
            .feed(data.as_bytes())
            .expect("レスポンスのデコードは成功するはず (実装バグ)");
        let (_, _) = decoder
            .decode_headers()
            .expect("結果は存在するはず (実装バグ)")
            .expect("レスポンスのデコードは成功するはず (実装バグ)");
        // チャンクサイズ解析時にボディサイズ制限エラー
        let result = decoder.progress();
        assert!(result.is_err());
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
fn prop_response_decoder_body_too_large_close_delimited() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        // close-delimited ボディでも max_body_size を超えるとエラー
        let body_size = noprop::sample_usize_in(ctx, 200..500);
        let limits = DecoderLimits {
            max_body_size: 100,
            ..DecoderLimits::default()
        };
        let mut decoder = ResponseDecoder::with_limits(limits);
        // Content-Length も Transfer-Encoding もなし = close-delimited
        decoder
            .feed(b"HTTP/1.1 200 OK\r\n\r\n")
            .expect("レスポンスのデコードは成功するはず (実装バグ)");
        let (_, body_kind) = decoder
            .decode_headers()
            .expect("結果は存在するはず (実装バグ)")
            .expect("レスポンスのデコードは成功するはず (実装バグ)");
        assert_eq!(body_kind, BodyKind::CloseDelimited);

        // ボディデータを追加
        let body = vec![b'x'; body_size];
        decoder
            .feed(&body)
            .expect("レスポンスのデコードは成功するはず (実装バグ)");

        // ボディを消費していくと max_body_size 超過でエラー
        let mut consumed = 0;
        while let Some(data) = decoder.peek_body() {
            let len = data.len();
            match decoder.consume_body(len) {
                Ok(_) => consumed += len,
                Err(shiguredo_http11::Error::BodyTooLarge { .. }) => {
                    // max_body_size を超えた時点でエラー
                    assert!(consumed <= 100);
                    return Ok(());
                }
                Err(e) => {
                    return Err(format!("予期しないエラー: {:?}", e).into());
                }
            }
        }
        // ここに到達した場合は問題
        panic!(
            "BodyTooLarge エラーを期待したが {} バイト消費した",
            consumed
        );
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
fn prop_response_decoder_feed_unchecked_no_limit() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let data_size = noprop::sample_usize_in(ctx, 1000..2000);
        let limits = DecoderLimits {
            max_buffer_size: 100,
            ..DecoderLimits::default()
        };
        let mut decoder = ResponseDecoder::with_limits(limits);
        let data = "x".repeat(data_size);
        decoder.feed_unchecked(data.as_bytes());
        assert_eq!(decoder.remaining().len(), data_size);
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
fn prop_response_decoder_limits_getter() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let max_buffer_size = noprop::sample_usize_in(ctx, 100..1000);
        let max_body_size = noprop::sample_u64_in(ctx, 100..1000);
        let limits = DecoderLimits {
            max_buffer_size,
            max_body_size,
            ..DecoderLimits::default()
        };
        let decoder = ResponseDecoder::with_limits(limits.clone());
        assert_eq!(decoder.limits().max_buffer_size, max_buffer_size);
        assert_eq!(decoder.limits().max_body_size, max_body_size);
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
// close-delimited ボディサイズ制限
// ========================================

/// close-delimited のボディサイズ制限チェック
#[test]
fn prop_response_decode_close_delimited_body_too_large() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let body_size = noprop::sample_usize_in(ctx, 128..512);
        let limits = DecoderLimits {
            max_body_size: 64,
            ..DecoderLimits::default()
        };
        let mut decoder = ResponseDecoder::with_limits(limits);
        let body_data = vec![0x41u8; body_size];
        decoder
            .feed(b"HTTP/1.1 200 OK\r\n\r\n")
            .expect("レスポンスのデコードは成功するはず (実装バグ)");
        decoder
            .feed(&body_data)
            .expect("レスポンスのデコードは成功するはず (実装バグ)");

        let result = decoder.decode();
        let is_body_too_large = matches!(result, Err(Error::BodyTooLarge { .. }));
        assert!(
            is_body_too_large,
            "BodyTooLarge を期待したが {:?} だった",
            result
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
