//! RequestDecoder のプロパティテスト

use shiguredo_http11::{
    BodyKind, BodyProgress, DecoderLimits, Error, HeaderName, HttpHead, Method, Request,
    RequestDecoder,
};

use super::{body, http_method, http_uri};

/// `[A-Za-z]` の 1 文字を生成する
fn alpha_char(ctx: &mut noprop::TestCaseContext) -> char {
    let index = noprop::sample_usize_in(ctx, 0..52);
    if index < 26 {
        char::from(b'A' + index as u8)
    } else {
        char::from(b'a' + (index - 26) as u8)
    }
}

/// `[a-z]` の 1 文字を生成する
fn lower_char(ctx: &mut noprop::TestCaseContext) -> char {
    char::from(b'a' + noprop::sample_usize_in(ctx, 0..26) as u8)
}

/// `[A-Za-z]` で構成された長さ `len_range` の文字列を生成する
fn alpha_string(
    ctx: &mut noprop::TestCaseContext,
    len_range: std::ops::RangeInclusive<usize>,
) -> String {
    let len = noprop::sample_usize_in(ctx, len_range);
    let mut s = String::with_capacity(len);
    for _ in 0..len {
        s.push(alpha_char(ctx));
    }
    s
}

/// `[A-Za-z0-9]` で構成された長さ `len_range` の文字列を生成する
fn alnum_string(
    ctx: &mut noprop::TestCaseContext,
    len_range: std::ops::RangeInclusive<usize>,
) -> String {
    let len = noprop::sample_usize_in(ctx, len_range);
    let mut s = String::with_capacity(len);
    for _ in 0..len {
        match noprop::sample_usize_in(ctx, 0..3) {
            0 => s.push(char::from(b'A' + noprop::sample_usize_in(ctx, 0..26) as u8)),
            1 => s.push(char::from(b'a' + noprop::sample_usize_in(ctx, 0..26) as u8)),
            _ => s.push(char::from(b'0' + noprop::sample_usize_in(ctx, 0..10) as u8)),
        }
    }
    s
}

/// `[a-z]` で構成された長さ `len_range` の文字列を生成する
fn lower_string(
    ctx: &mut noprop::TestCaseContext,
    len_range: std::ops::RangeInclusive<usize>,
) -> String {
    let len = noprop::sample_usize_in(ctx, len_range);
    let mut s = String::with_capacity(len);
    for _ in 0..len {
        s.push(lower_char(ctx));
    }
    s
}

/// `[a-z]{1,16}\.[a-z]{2,4}` 形式のホスト名を生成する
fn hostname(ctx: &mut noprop::TestCaseContext) -> String {
    format!("{}.{}", lower_string(ctx, 1..=16), lower_string(ctx, 2..=4))
}

// ========================================
// リクエスト行のエラー PBT
// ========================================

#[test]
fn prop_request_line_missing_parts_error() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        // バージョンがないリクエスト行はエラー
        let method = http_method(ctx);
        let uri = http_uri(ctx);
        let data = format!("{} {}\r\n\r\n", method, uri);
        let mut decoder = RequestDecoder::new();
        decoder
            .feed(data.as_bytes())
            .expect("リクエストのデコードは成功するはず (実装バグ)");
        assert!(decoder.decode_headers().is_err());
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
fn prop_request_line_empty_error() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        // 空のリクエスト行はエラー
        let header_name = alpha_string(ctx, 1..=16);
        let header_value = alnum_string(ctx, 1..=16);
        let data = format!("\r\n{}: {}\r\n\r\n", header_name, header_value);
        let mut decoder = RequestDecoder::new();
        decoder
            .feed(data.as_bytes())
            .expect("リクエストのデコードは成功するはず (実装バグ)");
        assert!(decoder.decode_headers().is_err());
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
// UTF-8 エラー PBT (リクエスト)
// ========================================

#[test]
fn prop_invalid_utf8_request_line_error() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        // 無効な UTF-8 バイトを含むリクエスト行はエラー
        let method = http_method(ctx);
        let invalid_byte = noprop::sample_usize_in(ctx, 128..=255) as u8;
        let mut data = format!("{} /", method).into_bytes();
        data.push(invalid_byte);
        data.extend(b" HTTP/1.1\r\n\r\n");
        let mut decoder = RequestDecoder::new();
        decoder
            .feed(&data)
            .expect("リクエストのデコードは成功するはず (実装バグ)");
        assert!(decoder.decode_headers().is_err());
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
fn prop_invalid_utf8_header_error() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        // 無効な UTF-8 バイトを含むヘッダーはエラー
        let header_name = alpha_string(ctx, 1..=16);
        let invalid_byte = noprop::sample_usize_in(ctx, 128..=255) as u8;
        let mut data = b"GET / HTTP/1.1\r\nHost: localhost\r\n".to_vec();
        data.extend(header_name.as_bytes());
        data.extend(b": ");
        data.push(invalid_byte);
        data.extend(b"\r\n\r\n");
        let mut decoder = RequestDecoder::new();
        decoder
            .feed(&data)
            .expect("リクエストのデコードは成功するはず (実装バグ)");
        assert!(decoder.decode_headers().is_err());
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
// 部分的なデータ (None を返す) PBT (リクエスト)
// ========================================

#[test]
fn prop_incomplete_request_line() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        // CRLF がないリクエスト行は None
        let method = http_method(ctx);
        let uri = http_uri(ctx);
        let data = format!("{} {} HTTP/1.1", method, uri);
        let mut decoder = RequestDecoder::new();
        decoder
            .feed(data.as_bytes())
            .expect("リクエストのデコードは成功するはず (実装バグ)");
        assert!(
            decoder
                .decode_headers()
                .expect("リクエストのデコードは成功するはず (実装バグ)")
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

#[test]
fn prop_incomplete_headers() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        // ヘッダー終端 CRLF がない場合は None
        let header_name = alpha_string(ctx, 1..=16);
        let header_value = alnum_string(ctx, 1..=16);
        let data = format!(
            "GET / HTTP/1.1\r\nHost: localhost\r\n{}: {}",
            header_name, header_value
        );
        let mut decoder = RequestDecoder::new();
        decoder
            .feed(data.as_bytes())
            .expect("リクエストのデコードは成功するはず (実装バグ)");
        assert!(
            decoder
                .decode_headers()
                .expect("リクエストのデコードは成功するはず (実装バグ)")
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

#[test]
fn prop_incomplete_body() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        // 不完全なボディは peek_body で部分データを返す
        let body_length = noprop::sample_usize_in(ctx, 10..100);
        let partial_length = noprop::sample_usize_in(ctx, 1..10);
        let full_body = "x".repeat(body_length);
        let partial_body = &full_body[..partial_length];
        let data = format!(
            "GET / HTTP/1.1\r\nHost: localhost\r\nContent-Length: {}\r\n\r\n{}",
            body_length, partial_body
        );
        let mut decoder = RequestDecoder::new();
        decoder
            .feed(data.as_bytes())
            .expect("リクエストのデコードは成功するはず (実装バグ)");
        let (_, body_kind) = decoder
            .decode_headers()
            .expect("結果は存在するはず (実装バグ)")
            .expect("リクエストのデコードは成功するはず (実装バグ)");
        assert_eq!(body_kind, BodyKind::ContentLength(body_length as u64));
        let peeked = decoder
            .peek_body()
            .expect("リクエストのデコードは成功するはず (実装バグ)");
        assert_eq!(peeked, partial_body.as_bytes());
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
// デコーダーリミット PBT (リクエスト)
// ========================================

#[test]
fn prop_request_decoder_buffer_overflow() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let data_size = noprop::sample_usize_in(ctx, 1000..2000);
        let limits = DecoderLimits {
            max_buffer_size: 100,
            ..DecoderLimits::default()
        };
        let mut decoder = RequestDecoder::with_limits(limits);
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
fn prop_request_decoder_exact_buffer_limit() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);
    let exact_seen = std::cell::Cell::new(0usize);
    let overflow_seen = std::cell::Cell::new(0usize);

    runner.run(256, |ctx| {
        let extra_bytes = noprop::sample_usize_in(ctx, 0..10);
        let limits = DecoderLimits {
            max_buffer_size: 100,
            ..DecoderLimits::default()
        };
        let mut decoder = RequestDecoder::with_limits(limits);
        let data = "x".repeat(100 + extra_bytes);
        if extra_bytes == 0 {
            // ちょうど上限のケースが一度も実行されないと境界値の検証が空振りする
            exact_seen.set(exact_seen.get() + 1);
            assert!(decoder.feed(data.as_bytes()).is_ok());
        } else {
            overflow_seen.set(overflow_seen.get() + 1);
            let result = decoder.feed(data.as_bytes());
            let is_buffer_overflow = matches!(result, Err(Error::BufferOverflow { .. }));
            assert!(
                is_buffer_overflow,
                "BufferOverflow を期待したが {:?} だった",
                result
            );
        }
        Ok(())
    })?;

    // p 推定値: extra_bytes ~ U(0..10) で extra_bytes == 0 の確率は 1/10。
    // 256 ケースでの miss 確率は (9/10)^256 ≈ 2e-12。
    assert!(
        exact_seen.get() > 0,
        "ちょうど上限のケースが一度も検証されなかった\n{runner}"
    );
    assert!(
        overflow_seen.get() > 0,
        "上限超過のケースが一度も検証されなかった\n{runner}"
    );

    // ジェネレータは valid-by-construction であり、ケース棄却が発生しないことの検証
    assert_eq!(
        runner.stats().rejected_cases,
        0,
        "ジェネレータが valid-by-construction であること\n{runner}"
    );
    Ok(())
}

#[test]
fn prop_request_decoder_header_line_too_long() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let header_value_len = noprop::sample_usize_in(ctx, 200..500);
        let limits = DecoderLimits {
            max_header_line_size: 100,
            ..DecoderLimits::default()
        };
        let mut decoder = RequestDecoder::with_limits(limits);
        let header_value = "x".repeat(header_value_len);
        let data = format!(
            "GET / HTTP/1.1\r\nHost: localhost\r\nX-Long: {}\r\n\r\n",
            header_value
        );
        decoder
            .feed(data.as_bytes())
            .expect("リクエストのデコードは成功するはず (実装バグ)");
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
fn prop_request_decoder_too_many_headers() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let header_count = noprop::sample_usize_in(ctx, 20..50);
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
        decoder
            .feed(data.as_bytes())
            .expect("リクエストのデコードは成功するはず (実装バグ)");
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
fn prop_request_decoder_exact_header_count() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);
    let exact_seen = std::cell::Cell::new(0usize);
    let overflow_seen = std::cell::Cell::new(0usize);

    runner.run(256, |ctx| {
        let extra_headers = noprop::sample_usize_in(ctx, 0..5);
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
        decoder
            .feed(data.as_bytes())
            .expect("リクエストのデコードは成功するはず (実装バグ)");
        if extra_headers == 0 {
            // Host + 9 headers = 10 (ちょうど max_count)
            // ちょうど上限のケースが一度も実行されないと境界値の検証が空振りする
            exact_seen.set(exact_seen.get() + 1);
            assert!(decoder.decode_headers().is_ok());
        } else {
            overflow_seen.set(overflow_seen.get() + 1);
            let result = decoder.decode_headers();
            let is_too_many_headers = matches!(result, Err(Error::TooManyHeaders { .. }));
            assert!(
                is_too_many_headers,
                "TooManyHeaders を期待したが {:?} だった",
                result
            );
        }
        Ok(())
    })?;

    // p 推定値: extra_headers ~ U(0..5) で extra_headers == 0 の確率は 1/5。
    // 256 ケースでの miss 確率は (4/5)^256 ≈ 4e-25。
    assert!(
        exact_seen.get() > 0,
        "ちょうど上限のケースが一度も検証されなかった\n{runner}"
    );
    assert!(
        overflow_seen.get() > 0,
        "上限超過のケースが一度も検証されなかった\n{runner}"
    );

    // ジェネレータは valid-by-construction であり、ケース棄却が発生しないことの検証
    assert_eq!(
        runner.stats().rejected_cases,
        0,
        "ジェネレータが valid-by-construction であること\n{runner}"
    );
    Ok(())
}

#[test]
fn prop_request_decoder_body_too_large_content_length() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let body_size = noprop::sample_usize_in(ctx, 200..500);
        let limits = DecoderLimits {
            max_body_size: 100,
            ..DecoderLimits::default()
        };
        let mut decoder = RequestDecoder::with_limits(limits);
        let body = "x".repeat(body_size);
        let data = format!(
            "POST / HTTP/1.1\r\nHost: localhost\r\nContent-Length: {}\r\n\r\n{}",
            body_size, body
        );
        decoder
            .feed(data.as_bytes())
            .expect("リクエストのデコードは成功するはず (実装バグ)");
        let result = decoder.decode_headers();
        // Content-Length が制限を超えているのでエラー
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
fn prop_request_decoder_exact_body_size() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);
    let exact_seen = std::cell::Cell::new(0usize);
    let overflow_seen = std::cell::Cell::new(0usize);

    runner.run(256, |ctx| {
        let extra_bytes = noprop::sample_u64_in(ctx, 0..10);
        let max_size = 100;
        let limits = DecoderLimits {
            max_body_size: max_size,
            ..DecoderLimits::default()
        };
        let mut decoder = RequestDecoder::with_limits(limits);
        let body_size = max_size + extra_bytes;
        let body = "x".repeat(body_size as usize);
        let data = format!(
            "POST / HTTP/1.1\r\nHost: localhost\r\nContent-Length: {}\r\n\r\n{}",
            body_size, body
        );
        decoder
            .feed(data.as_bytes())
            .expect("リクエストのデコードは成功するはず (実装バグ)");
        if extra_bytes == 0 {
            // ちょうど上限のケースが一度も実行されないと境界値の検証が空振りする
            exact_seen.set(exact_seen.get() + 1);
            assert!(decoder.decode_headers().is_ok());
        } else {
            overflow_seen.set(overflow_seen.get() + 1);
            assert!(decoder.decode_headers().is_err());
        }
        Ok(())
    })?;

    // p 推定値: extra_bytes ~ U(0..10) で extra_bytes == 0 の確率は 1/10。
    // 256 ケースでの miss 確率は (9/10)^256 ≈ 2e-12。
    assert!(
        exact_seen.get() > 0,
        "ちょうど上限のケースが一度も検証されなかった\n{runner}"
    );
    assert!(
        overflow_seen.get() > 0,
        "上限超過のケースが一度も検証されなかった\n{runner}"
    );

    // ジェネレータは valid-by-construction であり、ケース棄却が発生しないことの検証
    assert_eq!(
        runner.stats().rejected_cases,
        0,
        "ジェネレータが valid-by-construction であること\n{runner}"
    );
    Ok(())
}

#[test]
fn prop_request_decoder_body_too_large_chunked() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let chunk_size = noprop::sample_usize_in(ctx, 200..500);
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
        decoder.feed(data.as_bytes()).expect("リクエストのデコードは成功するはず (実装バグ)");
        let (_, _) = decoder.decode_headers().expect("結果は存在するはず (実装バグ)").expect("リクエストのデコードは成功するはず (実装バグ)");
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
fn prop_request_decoder_feed_unchecked_no_limit() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let data_size = noprop::sample_usize_in(ctx, 1000..2000);
        let limits = DecoderLimits {
            max_buffer_size: 100,
            ..DecoderLimits::default()
        };
        let mut decoder = RequestDecoder::with_limits(limits);
        let data = "x".repeat(data_size);
        // feed_unchecked はバッファ制限をチェックしない
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
fn prop_request_decoder_limits_getter() -> noprop::TestResult {
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
        let decoder = RequestDecoder::with_limits(limits.clone());
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
// ヘッダー個数・サイズ上限の PBT
// ========================================

// 同名ヘッダー (Cookie) を上限超で送ると TooManyHeaders になる
#[test]
fn prop_request_decoder_duplicate_cookie_counted() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let header_count = noprop::sample_usize_in(ctx, 20..50);
        let limits = DecoderLimits {
            max_headers_count: 10,
            ..DecoderLimits::default()
        };
        let mut decoder = RequestDecoder::with_limits(limits);
        let headers = (0..header_count)
            .map(|i| format!("Cookie: c{}=v", i))
            .collect::<Vec<_>>()
            .join("\r\n");
        let data = format!("GET / HTTP/1.1\r\nHost: localhost\r\n{}\r\n\r\n", headers);
        decoder
            .feed(data.as_bytes())
            .expect("リクエストのデコードは成功するはず (実装バグ)");
        let result = decoder.decode_headers();
        let is_too_many = matches!(result, Err(Error::TooManyHeaders { .. }));
        assert!(
            is_too_many,
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

// 空値ヘッダー (`x0:` 相当) を上限超で送ると TooManyHeaders になる
#[test]
fn prop_request_decoder_empty_value_header_flood_counted() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let header_count = noprop::sample_usize_in(ctx, 20..50);
        let limits = DecoderLimits {
            max_headers_count: 10,
            ..DecoderLimits::default()
        };
        let mut decoder = RequestDecoder::with_limits(limits);
        let headers = (0..header_count)
            .map(|i| format!("x{}:", i))
            .collect::<Vec<_>>()
            .join("\r\n");
        let data = format!("GET / HTTP/1.1\r\nHost: localhost\r\n{}\r\n\r\n", headers);
        decoder
            .feed(data.as_bytes())
            .expect("リクエストのデコードは成功するはず (実装バグ)");
        let result = decoder.decode_headers();
        let is_too_many = matches!(result, Err(Error::TooManyHeaders { .. }));
        assert!(
            is_too_many,
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

// デコード成功時、ヘッダー数は max_headers_count 以下、各ヘッダーの name+value は
// max_header_line_size 以下に収まる
#[test]
fn prop_request_decoded_header_memory_bounded() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let header_count = noprop::sample_usize_in(ctx, 1..30);
        let value_len = noprop::sample_usize_in(ctx, 0..40);
        let max_headers = 50;
        let max_line = 128;
        let limits = DecoderLimits {
            max_headers_count: max_headers,
            max_header_line_size: max_line,
            ..DecoderLimits::default()
        };
        let mut decoder = RequestDecoder::with_limits(limits);
        let value = "v".repeat(value_len);
        let headers = (0..header_count)
            .map(|i| format!("X-H{}: {}", i, value))
            .collect::<Vec<_>>()
            .join("\r\n");
        let data = format!("GET / HTTP/1.1\r\nHost: localhost\r\n{}\r\n\r\n", headers);
        decoder
            .feed(data.as_bytes())
            .expect("リクエストのデコードは成功するはず (実装バグ)");
        let (head, _) = decoder
            .decode_headers()
            .expect("結果は存在するはず (実装バグ)")
            .expect("リクエストのデコードは成功するはず (実装バグ)");
        // 保持ヘッダー数は上限以下 (Host + header_count <= max_headers)
        assert!(head.headers().len() <= max_headers);
        // 各ヘッダーの name+value バイト数は行サイズ上限以下
        for (name, value) in head.headers() {
            assert!(name.as_str().len() + value.len() <= max_line);
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
// 複数リクエスト PBT
// ========================================

#[test]
fn prop_multiple_requests_same_decoder() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let method_count = noprop::sample_usize_in(ctx, 2..5);
        let mut methods = Vec::new();
        for _ in 0..method_count {
            methods.push(http_method(ctx));
        }
        let uri_count = noprop::sample_usize_in(ctx, 2..5);
        let mut uris = Vec::new();
        for _ in 0..uri_count {
            uris.push(http_uri(ctx));
        }
        let count = methods.len().min(uris.len());
        let mut decoder = RequestDecoder::new();

        for i in 0..count {
            let mut request = Request::new(methods[i].clone(), &uris[i])
                .expect("リクエストのデコードは成功するはず (実装バグ)");
            request
                .add_header(HeaderName::from_static(b"Host"), "localhost")
                .expect("リクエストのデコードは成功するはず (実装バグ)");
            let encoded = request
                .encode()
                .expect("リクエストのデコードは成功するはず (実装バグ)");
            decoder
                .feed(&encoded)
                .expect("リクエストのデコードは成功するはず (実装バグ)");
            let decoded = decoder
                .decode()
                .expect("結果は存在するはず (実装バグ)")
                .expect("リクエストのデコードは成功するはず (実装バグ)");
            assert_eq!(decoded.method(), methods[i].as_str());
            assert_eq!(decoded.uri(), uris[i].as_str());
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

#[test]
fn prop_decoder_reuse_after_error() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let valid_method = http_method(ctx);
        let valid_uri = http_uri(ctx);
        let mut decoder = RequestDecoder::new();
        // 不正なリクエストでエラー
        decoder
            .feed(b"INVALID\r\n\r\n")
            .expect("リクエストのデコードは成功するはず (実装バグ)");
        let _ = decoder.decode_headers();
        decoder.reset();
        // リセット後は正常に動作
        let mut request = Request::new(valid_method.clone(), &valid_uri)
            .expect("リクエストのデコードは成功するはず (実装バグ)");
        request
            .add_header(HeaderName::from_static(b"Host"), "localhost")
            .expect("リクエストのデコードは成功するはず (実装バグ)");
        decoder
            .feed(
                &request
                    .encode()
                    .expect("リクエストのデコードは成功するはず (実装バグ)"),
            )
            .expect("リクエストのデコードは成功するはず (実装バグ)");
        let decoded = decoder
            .decode()
            .expect("結果は存在するはず (実装バグ)")
            .expect("リクエストのデコードは成功するはず (実装バグ)");
        assert_eq!(decoded.method(), valid_method.as_str());
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
// ストリーミング API の PBT (リクエスト)
// ========================================

#[test]
fn prop_streaming_decode_request() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let method = http_method(ctx);
        let uri = http_uri(ctx);
        let header_count = noprop::sample_usize_in(ctx, 0..5);
        let mut decoder = RequestDecoder::new();
        let headers = (0..header_count)
            .map(|i| format!("X-Header{}: value{}", i, i))
            .collect::<Vec<_>>()
            .join("\r\n");
        let data = if header_count > 0 {
            format!(
                "{} {} HTTP/1.1\r\nHost: localhost\r\n{}\r\n\r\n",
                method, uri, headers
            )
        } else {
            format!("{} {} HTTP/1.1\r\nHost: localhost\r\n\r\n", method, uri)
        };
        decoder
            .feed(data.as_bytes())
            .expect("リクエストのデコードは成功するはず (実装バグ)");
        let (head, body_kind) = decoder
            .decode_headers()
            .expect("結果は存在するはず (実装バグ)")
            .expect("リクエストのデコードは成功するはず (実装バグ)");
        assert_eq!(head.method(), method.as_str());
        assert_eq!(head.uri(), uri);
        assert_eq!(body_kind, BodyKind::None);
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
fn prop_streaming_decode_request_with_body() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let method = match noprop::sample_usize_in(ctx, 0..2) {
            0 => "POST",
            _ => "PUT",
        };
        let body_content = lower_string(ctx, 1..=100);
        let mut decoder = RequestDecoder::new();
        let body_len = body_content.len();
        let data = format!(
            "{} / HTTP/1.1\r\nHost: localhost\r\nContent-Length: {}\r\n\r\n{}",
            method, body_len, body_content
        );
        decoder
            .feed(data.as_bytes())
            .expect("リクエストのデコードは成功するはず (実装バグ)");
        let (head, body_kind) = decoder
            .decode_headers()
            .expect("結果は存在するはず (実装バグ)")
            .expect("リクエストのデコードは成功するはず (実装バグ)");
        assert_eq!(head.method(), method);
        assert_eq!(body_kind, BodyKind::ContentLength(body_len as u64));

        let mut body = Vec::new();
        while let Some(data) = decoder.peek_body() {
            body.extend_from_slice(data);
            let len = data.len();
            if let BodyProgress::Complete { .. } = decoder
                .consume_body(len)
                .expect("リクエストのデコードは成功するはず (実装バグ)")
            {
                break;
            }
        }
        assert_eq!(body, body_content.as_bytes());
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
// Request ラウンドトリップ PBT
// ========================================

#[test]
fn prop_request_roundtrip() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);
    let empty_seen = std::cell::Cell::new(0usize);
    let non_empty_seen = std::cell::Cell::new(0usize);

    runner.run(256, |ctx| {
        let method = http_method(ctx);
        let uri = http_uri(ctx);
        // body() は長さ 0..256 の一様なボディを返すため空ボディの出現確率が低く、
        // ボディなし (None) 側の分岐検証が空振りする可能性がある。ここでは
        // 空ボディを第一級ブランチとして確率 1/2 で生成する。
        let body_data = if noprop::sample_ratio(ctx, noprop::Ratio::one_nth(2)) {
            Vec::new()
        } else {
            let len = noprop::sample_usize_in(ctx, 1..=256);
            noprop::sample_bytes_vec(ctx, len)
        };
        let mut request = Request::new(method.clone(), &uri)
            .expect("リクエストのデコードは成功するはず (実装バグ)")
            .header(HeaderName::from_static(b"Host"), "example.com")
            .expect("リクエストのデコードは成功するはず (実装バグ)");

        if !body_data.is_empty() {
            request = request.body(body_data.clone());
        }

        let encoded = request
            .encode()
            .expect("リクエストのデコードは成功するはず (実装バグ)");
        let mut decoder = RequestDecoder::new();
        decoder
            .feed(&encoded)
            .expect("リクエストのデコードは成功するはず (実装バグ)");
        let decoded = decoder
            .decode()
            .expect("結果は存在するはず (実装バグ)")
            .expect("リクエストのデコードは成功するはず (実装バグ)");

        assert_eq!(decoded.method(), method.as_str());
        assert_eq!(decoded.uri(), uri.as_str());
        // body_data が空のときは .body() を呼んでいないので request.body == None。
        // エンコード時に Content-Length も付かないため、デコーダーは body == None を返す。
        if body_data.is_empty() {
            empty_seen.set(empty_seen.get() + 1);
            assert_eq!(decoded.body_bytes(), None);
        } else {
            non_empty_seen.set(non_empty_seen.get() + 1);
            assert_eq!(decoded.body_bytes(), Some(body_data.as_slice()));
        }
        Ok(())
    })?;

    // p 推定値: 空ボディも非空ボディも確率 1/2 で生成される。
    // 256 ケースでの miss 確率は (1/2)^256 ≈ 0。
    assert!(
        empty_seen.get() > 0,
        "ボディなし (None) 側が一度も検証されなかった\n{runner}"
    );
    assert!(
        non_empty_seen.get() > 0,
        "ボディあり (Some) 側が一度も検証されなかった\n{runner}"
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
// decode_headers を2回呼んだ場合の挙動 PBT (リクエスト)
// ========================================

#[test]
fn prop_decode_headers_twice_returns_none() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        // ボディなしメッセージの場合、2 回目の decode_headers は Ok(None)
        let uri = lower_string(ctx, 1..=10);
        let data = format!("GET /{} HTTP/1.1\r\nHost: localhost\r\n\r\n", uri);
        let mut decoder = RequestDecoder::new();
        decoder
            .feed(data.as_bytes())
            .expect("リクエストのデコードは成功するはず (実装バグ)");
        let _ = decoder
            .decode_headers()
            .expect("結果は存在するはず (実装バグ)")
            .expect("リクエストのデコードは成功するはず (実装バグ)");
        // 2回目は次のメッセージがないので Ok(None)
        assert!(
            decoder
                .decode_headers()
                .expect("リクエストのデコードは成功するはず (実装バグ)")
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
// consume_body を decode_headers 前に呼ぶとエラー PBT (リクエスト)
// ========================================

#[test]
fn prop_consume_body_before_decode_headers_error() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let method = http_method(ctx);
        let mut decoder = RequestDecoder::new();
        let data = format!("{} / HTTP/1.1\r\nHost: localhost\r\n\r\n", method);
        decoder
            .feed(data.as_bytes())
            .expect("リクエストのデコードは成功するはず (実装バグ)");
        assert!(decoder.progress().is_err());
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
// decode() API の連続デコードテスト (Keep-Alive) PBT (リクエスト)
// ========================================

#[test]
fn prop_decode_multiple_requests_keep_alive() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let method_count = noprop::sample_usize_in(ctx, 2..5);
        let mut methods = Vec::new();
        for _ in 0..method_count {
            methods.push(http_method(ctx));
        }
        let uri_count = noprop::sample_usize_in(ctx, 2..5);
        let mut uris = Vec::new();
        for _ in 0..uri_count {
            uris.push(http_uri(ctx));
        }
        let count = methods.len().min(uris.len());
        let mut decoder = RequestDecoder::new();

        // 全リクエストを一度にバッファに入れる
        let mut all_data = Vec::new();
        for i in 0..count {
            let mut request = Request::new(methods[i].clone(), &uris[i])
                .expect("リクエストのデコードは成功するはず (実装バグ)");
            request
                .add_header(HeaderName::from_static(b"Host"), "localhost")
                .expect("リクエストのデコードは成功するはず (実装バグ)");
            all_data.extend(
                request
                    .encode()
                    .expect("リクエストのデコードは成功するはず (実装バグ)"),
            );
        }
        decoder
            .feed(&all_data)
            .expect("リクエストのデコードは成功するはず (実装バグ)");

        // decode() を連続して呼ぶ（reset() なし）
        for i in 0..count {
            let request = decoder
                .decode()
                .expect("結果は存在するはず (実装バグ)")
                .expect("リクエストのデコードは成功するはず (実装バグ)");
            assert_eq!(request.method(), methods[i].as_str());
            assert_eq!(request.uri(), uris[i].as_str());
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

#[test]
fn prop_decode_multiple_requests_with_body_keep_alive() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let request_count = noprop::sample_usize_in(ctx, 2..4);
        let mut bodies = Vec::new();
        for _ in 0..request_count {
            let body_len = noprop::sample_usize_in(ctx, 0..=64);
            bodies.push(noprop::sample_bytes_vec(ctx, body_len));
        }
        let mut decoder = RequestDecoder::new();

        // 全リクエストを一度にバッファに入れる
        let mut all_data = Vec::new();
        for body_data in &bodies {
            let mut request = Request::new(Method::POST, "/")
                .expect("リクエストのデコードは成功するはず (実装バグ)");
            request
                .add_header(HeaderName::from_static(b"Host"), "localhost")
                .expect("リクエストのデコードは成功するはず (実装バグ)");
            let request = request.body(body_data.clone());
            all_data.extend(
                request
                    .encode()
                    .expect("リクエストのデコードは成功するはず (実装バグ)"),
            );
        }
        decoder
            .feed(&all_data)
            .expect("リクエストのデコードは成功するはず (実装バグ)");

        // decode() を連続して呼ぶ（reset() なし）
        // body == Some(empty) でもエンコード時に Content-Length: 0 が付くため、
        // デコーダー側も Some(empty) を返す。
        for body_data in &bodies {
            let request = decoder
                .decode()
                .expect("結果は存在するはず (実装バグ)")
                .expect("リクエストのデコードは成功するはず (実装バグ)");
            assert_eq!(request.body_bytes(), Some(body_data.as_slice()));
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
// decode_headers の Complete → StartLine 遷移 PBT (リクエスト)
// ========================================

#[test]
fn prop_decode_headers_multiple_no_body_messages() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        // 複数のボディなしメッセージを decode_headers で連続処理
        let count = noprop::sample_usize_in(ctx, 2..5);
        let mut decoder = RequestDecoder::new();
        for i in 0..count {
            let data = format!("GET /{} HTTP/1.1\r\nHost: localhost\r\n\r\n", i);
            decoder
                .feed(data.as_bytes())
                .expect("リクエストのデコードは成功するはず (実装バグ)");
        }

        for i in 0..count {
            let (head, body_kind) = decoder
                .decode_headers()
                .expect("結果は存在するはず (実装バグ)")
                .expect("リクエストのデコードは成功するはず (実装バグ)");
            assert_eq!(head.uri(), format!("/{}", i));
            assert!(matches!(body_kind, BodyKind::None));
        }

        // 次のメッセージがなければ Ok(None)
        assert!(
            decoder
                .decode_headers()
                .expect("リクエストのデコードは成功するはず (実装バグ)")
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
// 小文字メソッドの PBT
// ========================================

/// RFC 9110 Section 9: method = token
/// 小文字メソッドも token として有効なので受理される
#[test]
fn prop_lowercase_method_accepted() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let method = match noprop::sample_usize_in(ctx, 0..7) {
            0 => "get",
            1 => "post",
            2 => "put",
            3 => "delete",
            4 => "Get",
            5 => "Post",
            _ => "gET",
        };
        let mut decoder = RequestDecoder::new();
        let data = format!("{} / HTTP/1.1\r\nHost: localhost\r\n\r\n", method);
        decoder
            .feed(data.as_bytes())
            .expect("リクエストのデコードは成功するはず (実装バグ)");
        let result = decoder.decode_headers();
        assert!(
            result.is_ok(),
            "トークン形式のメソッド '{}' は受理されるべきだが {:?} だった",
            method,
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

/// 大文字メソッドが許可されることを確認
#[test]
fn prop_uppercase_method_allowed() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let method = match noprop::sample_usize_in(ctx, 0..7) {
            0 => "GET",
            1 => "POST",
            2 => "PUT",
            3 => "DELETE",
            4 => "HEAD",
            5 => "OPTIONS",
            _ => "PATCH",
        };
        let mut decoder = RequestDecoder::new();
        let data = format!("{} / HTTP/1.1\r\nHost: localhost\r\n\r\n", method);
        decoder
            .feed(data.as_bytes())
            .expect("リクエストのデコードは成功するはず (実装バグ)");
        let result = decoder.decode_headers();
        assert!(
            result.is_ok(),
            "大文字メソッド '{}' は成功すべきだが {:?} だった",
            method,
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

/// アンダースコアとハイフンを含むメソッドが許可されることを確認
#[test]
fn prop_method_with_underscore_hyphen_allowed() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let method = match noprop::sample_usize_in(ctx, 0..5) {
            0 => "X-CUSTOM",
            1 => "MY_METHOD",
            2 => "GET_PARAMETER",
            3 => "SET_PARAMETER",
            _ => "X-MY-METHOD",
        };
        let mut decoder = RequestDecoder::new();
        let data = format!("{} / HTTP/1.1\r\nHost: localhost\r\n\r\n", method);
        decoder
            .feed(data.as_bytes())
            .expect("リクエストのデコードは成功するはず (実装バグ)");
        let result = decoder.decode_headers();
        assert!(
            result.is_ok(),
            "アンダースコア/ハイフン入りメソッド '{}' は成功すべきだが {:?} だった",
            method,
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

// ========================================
// ストリーミング API 混在エラーの PBT
// ========================================

/// decode_headers() → body phase → decode() エラー
#[test]
fn prop_request_decode_mixed_api_error() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let body_len = noprop::sample_usize_in(ctx, 1..64);
        let body_data = noprop::sample_bytes_vec(ctx, body_len);
        let headers = format!(
            "POST / HTTP/1.1\r\nHost: localhost\r\nContent-Length: {}\r\n\r\n",
            body_data.len()
        );
        let mut full = headers.into_bytes();
        full.extend_from_slice(&body_data);

        let mut decoder = RequestDecoder::new();
        decoder
            .feed(&full)
            .expect("リクエストのデコードは成功するはず (実装バグ)");

        // ストリーミング API で decode_headers() を呼ぶ
        let (_, body_kind) = decoder
            .decode_headers()
            .expect("結果は存在するはず (実装バグ)")
            .expect("リクエストのデコードは成功するはず (実装バグ)");
        assert_eq!(body_kind, BodyKind::ContentLength(body_data.len() as u64));

        // decode() を呼ぶとエラー (ストリーミング API と混在)
        let result = decoder.decode();
        assert!(result.is_err());
        if let Err(Error::InvalidData(msg)) = result {
            assert!(msg.contains("mixed"));
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
// request-target 形式のテスト (RFC 9112 Section 3.2)
// ========================================

/// OPTIONS * (asterisk-form)
#[test]
fn prop_request_asterisk_form() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let version = match noprop::sample_usize_in(ctx, 0..4) {
            0 => "HTTP/1.1",
            1 => "HTTP/1.0",
            2 => "RTSP/1.0",
            _ => "RTSP/2.0",
        };
        let data = format!("OPTIONS * {}\r\nHost: localhost\r\n\r\n", version);
        let mut decoder = RequestDecoder::new();
        decoder
            .feed(data.as_bytes())
            .expect("リクエストのデコードは成功するはず (実装バグ)");
        let result = decoder
            .decode_headers()
            .expect("リクエストのデコードは成功するはず (実装バグ)");
        assert!(result.is_some());
        let (head, _) = result.expect("リクエストのデコードは成功するはず (実装バグ)");
        assert_eq!(head.method(), "OPTIONS");
        assert_eq!(head.uri(), "*");
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

/// CONNECT host:port (authority-form)
#[test]
fn prop_request_authority_form() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let host = hostname(ctx);
        let port = noprop::sample_usize_in(ctx, 1..=65535) as u16;
        let target = format!("{}:{}", host, port);
        let data = format!("CONNECT {} HTTP/1.1\r\nHost: {}\r\n\r\n", target, target);
        let mut decoder = RequestDecoder::new();
        decoder
            .feed(data.as_bytes())
            .expect("リクエストのデコードは成功するはず (実装バグ)");
        let result = decoder
            .decode_headers()
            .expect("リクエストのデコードは成功するはず (実装バグ)");
        assert!(result.is_some());
        let (head, _) = result.expect("リクエストのデコードは成功するはず (実装バグ)");
        assert_eq!(head.method(), "CONNECT");
        assert_eq!(head.uri(), target.as_str());
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

/// absolute-form (http://host/path)
#[test]
fn prop_request_absolute_form() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let method = match noprop::sample_usize_in(ctx, 0..3) {
            0 => "GET",
            1 => "POST",
            _ => "PUT",
        };
        let host = hostname(ctx);
        let path = format!("/{}", lower_string(ctx, 1..=16));
        let target = format!("http://{}{}", host, path);
        let data = format!("{} {} HTTP/1.1\r\nHost: {}\r\n\r\n", method, target, host);
        let mut decoder = RequestDecoder::new();
        decoder
            .feed(data.as_bytes())
            .expect("リクエストのデコードは成功するはず (実装バグ)");
        let result = decoder
            .decode_headers()
            .expect("リクエストのデコードは成功するはず (実装バグ)");
        assert!(result.is_some());
        let (head, _) = result.expect("リクエストのデコードは成功するはず (実装バグ)");
        assert_eq!(head.method(), method);
        assert_eq!(head.uri(), target.as_str());
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

/// origin-form にクエリパラメータを含む URI
#[test]
fn prop_request_origin_form_with_query() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let path_len = noprop::sample_usize_in(ctx, 1..=16);
        let mut path = String::with_capacity(path_len + 1);
        path.push('/');
        for _ in 0..path_len {
            match noprop::sample_usize_in(ctx, 0..3) {
                0 => path.push(char::from(b'A' + noprop::sample_usize_in(ctx, 0..26) as u8)),
                1 => path.push(char::from(b'a' + noprop::sample_usize_in(ctx, 0..26) as u8)),
                _ => path.push(char::from(b'0' + noprop::sample_usize_in(ctx, 0..10) as u8)),
            }
        }
        let key = alpha_string(ctx, 1..=8);
        let value = alnum_string(ctx, 1..=8);
        let uri = format!("{}?{}={}", path, key, value);
        let data = format!("GET {} HTTP/1.1\r\nHost: localhost\r\n\r\n", uri);
        let mut decoder = RequestDecoder::new();
        decoder
            .feed(data.as_bytes())
            .expect("リクエストのデコードは成功するはず (実装バグ)");
        let result = decoder
            .decode_headers()
            .expect("リクエストのデコードは成功するはず (実装バグ)");
        assert!(result.is_some());
        let (head, _) = result.expect("リクエストのデコードは成功するはず (実装バグ)");
        assert_eq!(head.uri(), uri.as_str());
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

/// origin-form にパーセントエンコーディングを含むパス
#[test]
fn prop_request_origin_form_with_percent_encoding() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let prefix = format!("/{}", alpha_string(ctx, 1..=8));
        let hex1 = match noprop::sample_usize_in(ctx, 0..5) {
            0 => "2F",
            1 => "20",
            2 => "3D",
            3 => "3F",
            _ => "41",
        };
        let suffix = alpha_string(ctx, 1..=8);
        let uri = format!("{}%{}{}", prefix, hex1, suffix);
        let data = format!("GET {} HTTP/1.1\r\nHost: localhost\r\n\r\n", uri);
        let mut decoder = RequestDecoder::new();
        decoder
            .feed(data.as_bytes())
            .expect("リクエストのデコードは成功するはず (実装バグ)");
        let result = decoder
            .decode_headers()
            .expect("リクエストのデコードは成功するはず (実装バグ)");
        assert!(result.is_some());
        let (head, _) = result.expect("リクエストのデコードは成功するはず (実装バグ)");
        assert_eq!(head.uri(), uri.as_str());
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

/// CONNECT 以外のメソッドで authority-form はエラー
#[test]
fn prop_request_non_connect_authority_form_error() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let method = match noprop::sample_usize_in(ctx, 0..4) {
            0 => "GET",
            1 => "POST",
            2 => "PUT",
            _ => "DELETE",
        };
        let host = hostname(ctx);
        let port = noprop::sample_usize_in(ctx, 1..=65535) as u16;
        let target = format!("{}:{}", host, port);
        let data = format!("{} {} HTTP/1.1\r\nHost: localhost\r\n\r\n", method, target);
        let mut decoder = RequestDecoder::new();
        decoder
            .feed(data.as_bytes())
            .expect("リクエストのデコードは成功するはず (実装バグ)");
        let result = decoder.decode_headers();
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

/// GET/POST 等で asterisk-form はエラー
#[test]
fn prop_request_non_options_asterisk_form_error() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let method = match noprop::sample_usize_in(ctx, 0..5) {
            0 => "GET",
            1 => "POST",
            2 => "PUT",
            3 => "DELETE",
            _ => "PATCH",
        };
        let data = format!("{} * HTTP/1.1\r\nHost: localhost\r\n\r\n", method);
        let mut decoder = RequestDecoder::new();
        decoder
            .feed(data.as_bytes())
            .expect("リクエストのデコードは成功するはず (実装バグ)");
        let result = decoder.decode_headers();
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

// ========================================
// 直接書き込み API (mut_buf / advance_buf / available_buf) のプロパティ
// ========================================

/// HTTP メッセージのバイト列を任意のチャンク境界で分割する Strategy
fn message_with_chunks(ctx: &mut noprop::TestCaseContext) -> (Vec<u8>, Vec<usize>) {
    let body_data = body(ctx);
    let headers = format!(
        "POST / HTTP/1.1\r\nHost: example.com\r\nContent-Length: {}\r\n\r\n",
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
            let mut decoder = RequestDecoder::new();
            let mut offset = 0usize;
            for &size in &chunk_sizes {
                if offset >= full.len() {
                    break;
                }
                let end = (offset + size).min(full.len());
                decoder
                    .feed(&full[offset..end])
                    .expect("リクエストのデコードは成功するはず (実装バグ)");
                offset = end;
            }
            if offset < full.len() {
                decoder
                    .feed(&full[offset..])
                    .expect("リクエストのデコードは成功するはず (実装バグ)");
            }
            decoder
                .decode()
                .expect("リクエストのデコードは成功するはず (実装バグ)")
        };

        let by_mut_buf = {
            let mut decoder = RequestDecoder::new();
            let mut offset = 0usize;
            for &size in &chunk_sizes {
                if offset >= full.len() {
                    break;
                }
                let end = (offset + size).min(full.len());
                let len = end - offset;
                let dst = decoder
                    .mut_buf(len)
                    .expect("リクエストのデコードは成功するはず (実装バグ)");
                dst.copy_from_slice(&full[offset..end]);
                decoder.advance_buf(len);
                offset = end;
            }
            if offset < full.len() {
                let len = full.len() - offset;
                let dst = decoder
                    .mut_buf(len)
                    .expect("リクエストのデコードは成功するはず (実装バグ)");
                dst.copy_from_slice(&full[offset..]);
                decoder.advance_buf(len);
            }
            decoder
                .decode()
                .expect("リクエストのデコードは成功するはず (実装バグ)")
        };

        let by_feed = by_feed.expect("feed 経路で request が得られなかった");
        let by_mut_buf = by_mut_buf.expect("mut_buf 経路で request が得られなかった");
        assert_eq!(by_feed.method(), by_mut_buf.method());
        assert_eq!(by_feed.uri(), by_mut_buf.uri());
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
        let mut decoder = RequestDecoder::new();
        let buf = decoder
            .mut_buf(len)
            .expect("リクエストのデコードは成功するはず (実装バグ)");
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
        let mut decoder = RequestDecoder::new();
        if !prefix.is_empty() {
            decoder
                .feed(&prefix)
                .expect("リクエストのデコードは成功するはず (実装バグ)");
        }
        let before = decoder.remaining().len();
        let buf = decoder
            .mut_buf(write_len)
            .expect("リクエストのデコードは成功するはず (実装バグ)");
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
        let mut decoder = RequestDecoder::new();
        if !prefix.is_empty() {
            decoder
                .feed(&prefix)
                .expect("リクエストのデコードは成功するはず (実装バグ)");
        }
        let before = decoder.remaining().to_vec();
        let _ = decoder
            .mut_buf(write_len)
            .expect("リクエストのデコードは成功するはず (実装バグ)");
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

// ========================================
// HTTP/1.1 以外で Transfer-Encoding 受理を拒否
// ========================================

/// HTTP/1.1 完全一致以外のリクエストで `Transfer-Encoding: chunked` は reject される
#[test]
fn prop_request_te_rejected_for_non_http11() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let version = match noprop::sample_usize_in(ctx, 0..7) {
            0 => "HTTP/0.9",
            1 => "HTTP/1.0",
            2 => "HTTP/2.0",
            3 => "HTTP/3.0",
            4 => "RTSP/1.0",
            5 => "RTSP/2.0",
            _ => "FOO/1.0",
        };
        let data = format!(
            "POST / {}\r\nHost: example.com\r\nTransfer-Encoding: chunked\r\n\r\n",
            version
        );
        let mut decoder = RequestDecoder::new();
        decoder
            .feed(data.as_bytes())
            .expect("リクエストのデコードは成功するはず (実装バグ)");
        let result = decoder.decode_headers();
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

/// HTTP/1.1 のリクエストで `Transfer-Encoding: chunked` は引き続き受理される
#[test]
fn prop_request_te_accepted_for_http11() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        // 元の proptest 版ではダミーの入力変数でプロパティとして駆動していたため、
        // 同様にダミーを 1 回サンプリングしてケース数を担保する。
        let _dummy = noprop::sample_bool(ctx);
        let data = b"POST / HTTP/1.1\r\nHost: example.com\r\nTransfer-Encoding: chunked\r\n\r\n";
        let mut decoder = RequestDecoder::new();
        decoder
            .feed(data)
            .expect("リクエストのデコードは成功するはず (実装バグ)");
        let (_, body_kind) = decoder
            .decode_headers()
            .expect("結果は存在するはず (実装バグ)")
            .expect("リクエストのデコードは成功するはず (実装バグ)");
        assert!(matches!(body_kind, BodyKind::Chunked));
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
