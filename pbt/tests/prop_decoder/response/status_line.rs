//! ResponseDecoder のステータス行関連プロパティテスト

use shiguredo_http11::{BodyKind, ResponseDecoder, StatusClass};

/// `[a-zA-Z]` で構成された長さ `len_range` の文字列を生成する
fn alpha_string(
    ctx: &mut noprop::TestCaseContext,
    len_range: std::ops::RangeInclusive<usize>,
) -> String {
    let len = noprop::sample_usize_in(ctx, len_range);
    let mut s = String::with_capacity(len);
    for _ in 0..len {
        let index = noprop::sample_usize_in(ctx, 0..52);
        let c = if index < 26 {
            char::from(b'A' + index as u8)
        } else {
            char::from(b'a' + (index - 26) as u8)
        };
        s.push(c);
    }
    s
}

// ========================================
// ステータス行のエラー PBT
// ========================================

#[test]
fn prop_status_line_missing_parts_error() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        // ステータスコードがないステータス行はエラー
        let version = match noprop::sample_usize_in(ctx, 0..4) {
            0 => "HTTP/1.0",
            1 => "HTTP/1.1",
            2 => "RTSP/1.0",
            _ => "RTSP/2.0",
        };
        let data = format!("{}\r\n\r\n", version);
        let mut decoder = ResponseDecoder::new();
        decoder
            .feed(data.as_bytes())
            .expect("レスポンスのデコードは成功するはず (実装バグ)");
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
fn prop_status_code_invalid_error() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        // 数字でないステータスコードはエラー
        let invalid_code = alpha_string(ctx, 1..=5);
        let data = format!("HTTP/1.1 {} OK\r\n\r\n", invalid_code);
        let mut decoder = ResponseDecoder::new();
        decoder
            .feed(data.as_bytes())
            .expect("レスポンスのデコードは成功するはず (実装バグ)");
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
fn prop_status_line_no_reason_phrase_ok() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        // reason phrase なしは OK
        let status_code = noprop::sample_usize_in(ctx, 200..600) as u16;
        let data = format!("HTTP/1.1 {}\r\n\r\n", status_code);
        let mut decoder = ResponseDecoder::new();
        decoder
            .feed(data.as_bytes())
            .expect("レスポンスのデコードは成功するはず (実装バグ)");
        let (head, _) = decoder
            .decode_headers()
            .expect("結果は存在するはず (実装バグ)")
            .expect("レスポンスのデコードは成功するはず (実装バグ)");
        assert_eq!(head.status_code(), status_code);
        assert_eq!(head.reason_phrase(), "");
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
// HEAD リクエストへのレスポンス PBT
// ========================================

#[test]
fn prop_head_response_with_content_length() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        // HEAD レスポンスは Content-Length があってもボディなし
        let content_length = noprop::sample_usize_in(ctx, 1..10000);
        let data = format!(
            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\n\r\n",
            content_length
        );
        let mut decoder = ResponseDecoder::new();
        decoder.set_request_method("HEAD");
        decoder
            .feed(data.as_bytes())
            .expect("レスポンスのデコードは成功するはず (実装バグ)");
        let (_, body_kind) = decoder
            .decode_headers()
            .expect("結果は存在するはず (実装バグ)")
            .expect("レスポンスのデコードは成功するはず (実装バグ)");
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
fn prop_head_response_with_transfer_encoding() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        // HEAD レスポンスは Transfer-Encoding があってもボディなし
        let status_code = noprop::sample_usize_in(ctx, 200..400) as u16;
        let data = format!(
            "HTTP/1.1 {} OK\r\nTransfer-Encoding: chunked\r\n\r\n",
            status_code
        );
        let mut decoder = ResponseDecoder::new();
        decoder.set_request_method("HEAD");
        decoder
            .feed(data.as_bytes())
            .expect("レスポンスのデコードは成功するはず (実装バグ)");
        let (_, body_kind) = decoder
            .decode_headers()
            .expect("結果は存在するはず (実装バグ)")
            .expect("レスポンスのデコードは成功するはず (実装バグ)");
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

// ========================================
// ボディなしステータスコード PBT
// ========================================

#[test]
fn prop_status_1xx_no_body() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        // 1xx レスポンスは Content-Length があってもボディなし
        let code = noprop::sample_usize_in(ctx, 100..200) as u16;
        let content_length = noprop::sample_usize_in(ctx, 1..1000);
        let data = format!(
            "HTTP/1.1 {} Continue\r\nContent-Length: {}\r\n\r\n",
            code, content_length
        );
        let mut decoder = ResponseDecoder::new();
        decoder
            .feed(data.as_bytes())
            .expect("レスポンスのデコードは成功するはず (実装バグ)");
        let (_, body_kind) = decoder
            .decode_headers()
            .expect("結果は存在するはず (実装バグ)")
            .expect("レスポンスのデコードは成功するはず (実装バグ)");
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
fn prop_status_204_no_body() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        // 204 No Content はボディなし
        let content_length = noprop::sample_usize_in(ctx, 1..1000);
        let data = format!(
            "HTTP/1.1 204 No Content\r\nContent-Length: {}\r\n\r\n",
            content_length
        );
        let mut decoder = ResponseDecoder::new();
        decoder
            .feed(data.as_bytes())
            .expect("レスポンスのデコードは成功するはず (実装バグ)");
        let (_, body_kind) = decoder
            .decode_headers()
            .expect("結果は存在するはず (実装バグ)")
            .expect("レスポンスのデコードは成功するはず (実装バグ)");
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
fn prop_status_304_no_body() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        // 304 Not Modified はボディなし
        let content_length = noprop::sample_usize_in(ctx, 1..1000);
        let data = format!(
            "HTTP/1.1 304 Not Modified\r\nContent-Length: {}\r\n\r\n",
            content_length
        );
        let mut decoder = ResponseDecoder::new();
        decoder
            .feed(data.as_bytes())
            .expect("レスポンスのデコードは成功するはず (実装バグ)");
        let (_, body_kind) = decoder
            .decode_headers()
            .expect("結果は存在するはず (実装バグ)")
            .expect("レスポンスのデコードは成功するはず (実装バグ)");
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
fn prop_status_code_boundary_199() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        // 199 以下は 1xx
        let code = noprop::sample_usize_in(ctx, 100..200) as u16;
        let data = format!("HTTP/1.1 {} Info\r\n\r\n", code);
        let mut decoder = ResponseDecoder::new();
        decoder
            .feed(data.as_bytes())
            .expect("レスポンスのデコードは成功するはず (実装バグ)");
        let (head, body_kind) = decoder
            .decode_headers()
            .expect("結果は存在するはず (実装バグ)")
            .expect("レスポンスのデコードは成功するはず (実装バグ)");
        assert_eq!(head.status_class(), StatusClass::Informational);
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
fn prop_status_code_boundary_200() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        // 200-299 は成功
        let code = noprop::sample_usize_in(ctx, 200..300) as u16;
        let data = format!("HTTP/1.1 {} OK\r\nContent-Length: 0\r\n\r\n", code);
        let mut decoder = ResponseDecoder::new();
        decoder
            .feed(data.as_bytes())
            .expect("レスポンスのデコードは成功するはず (実装バグ)");
        let (head, _) = decoder
            .decode_headers()
            .expect("結果は存在するはず (実装バグ)")
            .expect("レスポンスのデコードは成功するはず (実装バグ)");
        // 204 は特別扱い
        if code != 204 {
            assert_eq!(head.status_class(), StatusClass::Successful);
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
fn prop_status_code_boundary_203() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        // 200-203 はボディあり可能
        let code = noprop::sample_usize_in(ctx, 200..204) as u16;
        let data = format!("HTTP/1.1 {} OK\r\nContent-Length: 5\r\n\r\nhello", code);
        let mut decoder = ResponseDecoder::new();
        decoder
            .feed(data.as_bytes())
            .expect("レスポンスのデコードは成功するはず (実装バグ)");
        let (_, body_kind) = decoder
            .decode_headers()
            .expect("結果は存在するはず (実装バグ)")
            .expect("レスポンスのデコードは成功するはず (実装バグ)");
        assert_eq!(body_kind, BodyKind::ContentLength(5));
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
// UTF-8 エラー PBT (ステータス行 / ヘッダー)
// ========================================

#[test]
fn prop_invalid_utf8_status_line_error() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        // 無効な UTF-8 バイトを含むステータス行はエラー
        let invalid_byte = noprop::sample_usize_in(ctx, 128..=255) as u8;
        let mut data = b"HTTP/1.1 200 ".to_vec();
        data.push(invalid_byte);
        data.extend(b"OK\r\n\r\n");
        let mut decoder = ResponseDecoder::new();
        decoder
            .feed(&data)
            .expect("レスポンスのデコードは成功するはず (実装バグ)");
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
fn prop_invalid_utf8_response_header_error() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        // 無効な UTF-8 バイトを含むレスポンスヘッダーはエラー
        let header_name = alpha_string(ctx, 1..=16);
        let invalid_byte = noprop::sample_usize_in(ctx, 128..=255) as u8;
        let mut data = b"HTTP/1.1 200 OK\r\n".to_vec();
        data.extend(header_name.as_bytes());
        data.extend(b": ");
        data.push(invalid_byte);
        data.extend(b"\r\n\r\n");
        let mut decoder = ResponseDecoder::new();
        decoder
            .feed(&data)
            .expect("レスポンスのデコードは成功するはず (実装バグ)");
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
// HTTP/1.1 以外で Transfer-Encoding 受理を拒否
// ========================================

/// HTTP/1.1 完全一致以外のレスポンスで Transfer-Encoding は reject される
#[test]
fn prop_response_te_rejected_for_non_http11() -> noprop::TestResult {
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
        let data = format!("{} 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n", version);
        let mut decoder = ResponseDecoder::new();
        decoder
            .feed(data.as_bytes())
            .expect("レスポンスのデコードは成功するはず (実装バグ)");
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

/// HTTP/1.1 のレスポンスで Transfer-Encoding: chunked は引き続き受理される
#[test]
fn prop_response_te_accepted_for_http11() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        // 元の proptest 版ではダミーの入力変数でプロパティとして駆動していたため、
        // 同様にダミーを 1 回サンプリングしてケース数を担保する。
        let _dummy = noprop::sample_bool(ctx);
        let data = b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n";
        let mut decoder = ResponseDecoder::new();
        decoder
            .feed(data)
            .expect("レスポンスのデコードは成功するはず (実装バグ)");
        let (_, body_kind) = decoder
            .decode_headers()
            .expect("結果は存在するはず (実装バグ)")
            .expect("レスポンスのデコードは成功するはず (実装バグ)");
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

// ========================================
// decode_headers を2回呼んだ場合の挙動 PBT (レスポンス)
// ========================================

#[test]
fn prop_response_decode_headers_twice_returns_none() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        // ボディなしレスポンスの場合、2 回目の decode_headers は Ok(None)
        let status_code = noprop::sample_usize_in(ctx, 200..600) as u16;
        let data = format!("HTTP/1.1 {} OK\r\nContent-Length: 0\r\n\r\n", status_code);
        let mut decoder = ResponseDecoder::new();
        decoder
            .feed(data.as_bytes())
            .expect("レスポンスのデコードは成功するはず (実装バグ)");
        let _ = decoder
            .decode_headers()
            .expect("結果は存在するはず (実装バグ)")
            .expect("レスポンスのデコードは成功するはず (実装バグ)");
        // 2回目は次のメッセージがないので Ok(None)
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
