//! ボディデコードの PBT (chunked, content-length, close-delimited)

use shiguredo_http11::{
    BodyKind, BodyProgress, DecoderLimits, Error, RequestDecoder, ResponseDecoder, encode_chunk,
    encode_chunks,
};

use super::body;

/// `[a-z]` の 1 文字を生成する
fn lower_char(ctx: &mut noprop::TestCaseContext) -> char {
    char::from(b'a' + noprop::sample_usize_in(ctx, 0..26) as u8)
}

/// `[A-Za-z]` の 1 文字を生成する
fn alpha_char(ctx: &mut noprop::TestCaseContext) -> char {
    let index = noprop::sample_usize_in(ctx, 0..52);
    if index < 26 {
        char::from(b'A' + index as u8)
    } else {
        char::from(b'a' + (index - 26) as u8)
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
        s.push(lower_char(ctx));
    }
    s
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

/// `[a-z0-9]` で構成された長さ `len_range` の文字列を生成する
fn lower_alnum_string(
    ctx: &mut noprop::TestCaseContext,
    len_range: std::ops::RangeInclusive<usize>,
) -> String {
    let len = noprop::sample_usize_in(ctx, len_range);
    let mut s = String::with_capacity(len);
    for _ in 0..len {
        match noprop::sample_usize_in(ctx, 0..2) {
            0 => s.push(lower_char(ctx)),
            _ => s.push(char::from(b'0' + noprop::sample_usize_in(ctx, 0..10) as u8)),
        }
    }
    s
}

/// `[A-Z]` で構成された長さ `len_range` の文字列を生成する
fn upper_string(
    ctx: &mut noprop::TestCaseContext,
    len_range: std::ops::RangeInclusive<usize>,
) -> String {
    let len = noprop::sample_usize_in(ctx, len_range);
    let mut s = String::with_capacity(len);
    for _ in 0..len {
        s.push(char::from(b'A' + noprop::sample_usize_in(ctx, 0..26) as u8));
    }
    s
}

/// `[G-Zg-z]` の 1 文字を生成する (16 進数文字 0-9/A-F/a-f 以外の英字)
fn invalid_hex_char(ctx: &mut noprop::TestCaseContext) -> char {
    match noprop::sample_usize_in(ctx, 0..2) {
        0 => char::from(b'G' + noprop::sample_usize_in(ctx, 0..20) as u8),
        _ => char::from(b'g' + noprop::sample_usize_in(ctx, 0..20) as u8),
    }
}

/// 空でない 1..=64 バイトのチャンクを 1..=3 個生成する
fn random_chunks(ctx: &mut noprop::TestCaseContext) -> Vec<Vec<u8>> {
    let chunk_count = noprop::sample_usize_in(ctx, 1..4);
    let mut chunks = Vec::new();
    for _ in 0..chunk_count {
        let chunk_len = noprop::sample_usize_in(ctx, 1..64);
        chunks.push(noprop::sample_bytes_vec(ctx, chunk_len));
    }
    chunks
}

// ========================================
// チャンクエンコーディング PBT
// ========================================

#[test]
fn prop_chunked_invalid_size_error() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        // 無効なチャンクサイズはエラー
        let invalid_len = noprop::sample_usize_in(ctx, 1..=5);
        let mut invalid_size = String::with_capacity(invalid_len);
        for _ in 0..invalid_len {
            invalid_size.push(invalid_hex_char(ctx));
        }
        let data = format!(
            "HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n{}\r\n",
            invalid_size
        );
        let mut decoder = ResponseDecoder::new();
        decoder
            .feed(data.as_bytes())
            .expect("ボディのデコードは成功するはず (実装バグ)");
        let (_, body_kind) = decoder
            .decode_headers()
            .expect("結果は存在するはず (実装バグ)")
            .expect("ボディのデコードは成功するはず (実装バグ)");
        assert_eq!(body_kind, BodyKind::Chunked);
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

#[test]
fn prop_chunked_size_with_extension_ok() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        // チャンク拡張は OK
        let body_content = lower_string(ctx, 1..=32);
        let ext_name = lower_string(ctx, 1..=8);
        let ext_value = lower_alnum_string(ctx, 1..=8);
        let len = body_content.len();
        let data = format!(
            "HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n{:x};{}={}\r\n{}\r\n0\r\n\r\n",
            len, ext_name, ext_value, body_content
        );
        let mut decoder = ResponseDecoder::new();
        decoder
            .feed(data.as_bytes())
            .expect("ボディのデコードは成功するはず (実装バグ)");
        let (_, body_kind) = decoder
            .decode_headers()
            .expect("結果は存在するはず (実装バグ)")
            .expect("ボディのデコードは成功するはず (実装バグ)");
        assert_eq!(body_kind, BodyKind::Chunked);

        let mut body = Vec::new();
        loop {
            if let Some(data) = decoder.peek_body() {
                body.extend_from_slice(data);
                let len = data.len();
                if let BodyProgress::Complete { .. } = decoder
                    .consume_body(len)
                    .expect("ボディのデコードは成功するはず (実装バグ)")
                {
                    break;
                }
            } else if let BodyProgress::Complete { .. } = decoder
                .progress()
                .expect("ボディのデコードは成功するはず (実装バグ)")
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

#[test]
fn prop_chunked_with_trailer_ok() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let body_content = lower_string(ctx, 1..=32);
        // RFC 9110 Section 6.5.1 で `Authorization` / `TE` / `Date` 等の禁止
        // フィールドは trailer に置けないため、`X-` prefix を強制して
        // 拒否リストと衝突しない名前のみ生成する。
        let trailer_name = format!("X-{}", alpha_string(ctx, 0..=14));
        let trailer_value = lower_alnum_string(ctx, 1..=16);
        // トレーラーは OK
        // RFC 9110 Section 6.5.1 ホワイトリスト方式: `Trailer:` ヘッダーで
        // 事前申告したフィールドのみ受理される。
        let len = body_content.len();
        let data = format!(
            "HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\nTrailer: {}\r\n\r\n{:x}\r\n{}\r\n0\r\n{}: {}\r\n\r\n",
            trailer_name, len, body_content, trailer_name, trailer_value
        );
        let mut decoder = ResponseDecoder::new();
        decoder.feed(data.as_bytes()).expect("ボディのデコードは成功するはず (実装バグ)");
        let (_, body_kind) = decoder.decode_headers().expect("結果は存在するはず (実装バグ)").expect("ボディのデコードは成功するはず (実装バグ)");
        assert_eq!(body_kind, BodyKind::Chunked);

        let mut body = Vec::new();
        let trailers = loop {
            if let Some(data) = decoder.peek_body() {
                body.extend_from_slice(data);
                let len = data.len();
                if let BodyProgress::Complete { trailers } = decoder.consume_body(len).expect("ボディのデコードは成功するはず (実装バグ)") {
                    break trailers;
                }
            } else if let BodyProgress::Complete { trailers } = decoder.progress().expect("ボディのデコードは成功するはず (実装バグ)") {
                break trailers;
            }
        };
        assert_eq!(body, body_content.as_bytes());
        assert_eq!(trailers.len(), 1);
        assert_eq!(trailers[0].0.as_str(), trailer_name.as_str());
        assert_eq!(&trailers[0].1, &trailer_value);
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
fn prop_chunked_with_multiple_trailers_ok() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        // 複数のトレーラーは OK (`Trailer:` ヘッダーで事前申告したもののみ)
        let body_content = lower_string(ctx, 1..=32);
        let trailer_count = noprop::sample_usize_in(ctx, 1..4);
        let len = body_content.len();
        let declared = (0..trailer_count)
            .map(|i| format!("X-Trailer{}", i))
            .collect::<Vec<_>>()
            .join(", ");
        let trailers = (0..trailer_count)
            .map(|i| format!("X-Trailer{}: value{}", i, i))
            .collect::<Vec<_>>()
            .join("\r\n");
        let data = format!(
            "HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\nTrailer: {}\r\n\r\n{:x}\r\n{}\r\n0\r\n{}\r\n\r\n",
            declared, len, body_content, trailers
        );
        let mut decoder = ResponseDecoder::new();
        decoder.feed(data.as_bytes()).expect("ボディのデコードは成功するはず (実装バグ)");
        let (_, _) = decoder.decode_headers().expect("結果は存在するはず (実装バグ)").expect("ボディのデコードは成功するはず (実装バグ)");

        let mut body = Vec::new();
        let result_trailers = loop {
            if let Some(data) = decoder.peek_body() {
                body.extend_from_slice(data);
                let len = data.len();
                if let BodyProgress::Complete { trailers } = decoder.consume_body(len).expect("ボディのデコードは成功するはず (実装バグ)") {
                    break trailers;
                }
            } else if let BodyProgress::Complete { trailers } = decoder.progress().expect("ボディのデコードは成功するはず (実装バグ)") {
                break trailers;
            }
        };
        assert_eq!(body, body_content.as_bytes());
        assert_eq!(result_trailers.len(), trailer_count);
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
fn prop_chunked_trailer_too_many_error() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        // トレーラーフィールドを上限超で送ると TooManyHeaders になる
        // (トレーラーは `Trailer:` ヘッダーで申告したホワイトリストのみ受理される)
        let trailer_count = noprop::sample_usize_in(ctx, 12..30);
        let limits = DecoderLimits {
            max_headers_count: 10,
            ..DecoderLimits::default()
        };
        let declared = (0..trailer_count)
            .map(|i| format!("X-T{}", i))
            .collect::<Vec<_>>()
            .join(", ");
        let trailers = (0..trailer_count)
            .map(|i| format!("X-T{}: v{}", i, i))
            .collect::<Vec<_>>()
            .join("\r\n");
        let data = format!(
            "HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\nTrailer: {}\r\n\r\n0\r\n{}\r\n\r\n",
            declared, trailers
        );
        let mut decoder = ResponseDecoder::with_limits(limits);
        decoder
            .feed(data.as_bytes())
            .expect("ボディのデコードは成功するはず (実装バグ)");
        decoder
            .decode_headers()
            .expect("結果は存在するはず (実装バグ)")
            .expect("ボディのデコードは成功するはず (実装バグ)");
        // トレーラー処理でカウント上限に達し TooManyHeaders になる
        let result = decoder.progress();
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

#[test]
fn prop_chunked_multiple_chunks() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let chunk1 = {
            let len = noprop::sample_usize_in(ctx, 1..64);
            noprop::sample_bytes_vec(ctx, len)
        };
        let chunk2 = {
            let len = noprop::sample_usize_in(ctx, 1..64);
            noprop::sample_bytes_vec(ctx, len)
        };
        let chunk3 = {
            let len = noprop::sample_usize_in(ctx, 1..64);
            noprop::sample_bytes_vec(ctx, len)
        };
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
        decoder
            .feed(&data)
            .expect("ボディのデコードは成功するはず (実装バグ)");
        let (_, _) = decoder
            .decode_headers()
            .expect("結果は存在するはず (実装バグ)")
            .expect("ボディのデコードは成功するはず (実装バグ)");

        let mut body = Vec::new();
        loop {
            if let Some(d) = decoder.peek_body() {
                body.extend_from_slice(d);
                let len = d.len();
                if let BodyProgress::Complete { .. } = decoder
                    .consume_body(len)
                    .expect("ボディのデコードは成功するはず (実装バグ)")
                {
                    break;
                }
            } else if let BodyProgress::Complete { .. } = decoder
                .progress()
                .expect("ボディのデコードは成功するはず (実装バグ)")
            {
                break;
            }
        }
        let expected: Vec<u8> = [chunk1, chunk2, chunk3].concat();
        assert_eq!(body, expected);
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
fn prop_chunked_roundtrip() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let chunk_count = noprop::sample_usize_in(ctx, 1..5);
        let mut chunks = Vec::new();
        for _ in 0..chunk_count {
            chunks.push(body(ctx));
        }
        let non_empty_chunks: Vec<Vec<u8>> = chunks.into_iter().filter(|c| !c.is_empty()).collect();
        let chunk_refs: Vec<&[u8]> = non_empty_chunks.iter().map(|c| c.as_slice()).collect();
        let encoded = encode_chunks(&chunk_refs);

        let mut decoder = ResponseDecoder::new();
        let header = b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n";
        decoder
            .feed(header)
            .expect("ボディのデコードは成功するはず (実装バグ)");
        decoder
            .feed(&encoded)
            .expect("ボディのデコードは成功するはず (実装バグ)");

        let (_, _) = decoder
            .decode_headers()
            .expect("結果は存在するはず (実装バグ)")
            .expect("ボディのデコードは成功するはず (実装バグ)");

        let mut body = Vec::new();
        loop {
            if let Some(d) = decoder.peek_body() {
                body.extend_from_slice(d);
                let len = d.len();
                if let BodyProgress::Complete { .. } = decoder
                    .consume_body(len)
                    .expect("ボディのデコードは成功するはず (実装バグ)")
                {
                    break;
                }
            } else if let BodyProgress::Complete { .. } = decoder
                .progress()
                .expect("ボディのデコードは成功するはず (実装バグ)")
            {
                break;
            }
        }
        let expected: Vec<u8> = non_empty_chunks.iter().flatten().copied().collect();
        assert_eq!(&body, &expected);
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
fn prop_encode_chunk_valid() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);
    let empty_seen = std::cell::Cell::new(0usize);
    let non_empty_seen = std::cell::Cell::new(0usize);

    runner.run(256, |ctx| {
        // body() は長さ 0..256 の一様なボディを返すため空ボディの出現確率が低く、
        // 空ボディ側の分岐検証が空振りする可能性がある。ここでは空ボディを
        // 第一級ブランチとして確率 1/2 で生成する。
        let data = if noprop::sample_ratio(ctx, noprop::Ratio::one_nth(2)) {
            Vec::new()
        } else {
            let len = noprop::sample_usize_in(ctx, 1..=256);
            noprop::sample_bytes_vec(ctx, len)
        };
        let chunk = encode_chunk(&data);

        if data.is_empty() {
            empty_seen.set(empty_seen.get() + 1);
            assert_eq!(&chunk, b"0\r\n\r\n");
        } else {
            non_empty_seen.set(non_empty_seen.get() + 1);
            let expected_size = format!("{:x}\r\n", data.len());
            assert!(chunk.starts_with(expected_size.as_bytes()));
            assert!(chunk.ends_with(b"\r\n"));
        }
        Ok(())
    })?;

    // p 推定値: 空ボディも非空ボディも確率 1/2 で生成される。
    // 256 ケースでの miss 確率は (1/2)^256 ≈ 0。
    assert!(
        empty_seen.get() > 0,
        "空ボディの encode_chunk が一度も検証されなかった\n{runner}"
    );
    assert!(
        non_empty_seen.get() > 0,
        "非空ボディの encode_chunk が一度も検証されなかった\n{runner}"
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
// チャンク CRLF 検証 PBT
// ========================================

#[test]
fn prop_chunked_invalid_crlf_after_data_error() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        // チャンクデータ後に CRLF ではなく別のバイトがある
        let body_content = lower_string(ctx, 5..=10);
        let invalid_char1 = char::from(b'A' + noprop::sample_usize_in(ctx, 0..26) as u8);
        let invalid_char2 = char::from(b'A' + noprop::sample_usize_in(ctx, 0..26) as u8);
        let len = body_content.len();
        let data = format!(
            "HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n{:x}\r\n{}{}{}",
            len, body_content, invalid_char1, invalid_char2
        );
        let mut decoder = ResponseDecoder::new();
        decoder
            .feed(data.as_bytes())
            .expect("ボディのデコードは成功するはず (実装バグ)");
        let (_, _) = decoder
            .decode_headers()
            .expect("結果は存在するはず (実装バグ)")
            .expect("ボディのデコードは成功するはず (実装バグ)");

        // チャンクサイズを処理
        decoder
            .progress()
            .expect("ボディのデコードは成功するはず (実装バグ)");
        // チャンクデータを消費
        let peeked = decoder
            .peek_body()
            .expect("ボディのデコードは成功するはず (実装バグ)");
        assert_eq!(peeked, body_content.as_bytes());
        let result = decoder.consume_body(len);
        // CRLF ではないのでエラー
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
fn prop_chunked_invalid_crlf_partial_then_error() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        // 部分的にデータを受け取り、その後 CRLF ではない
        let body_content = lower_string(ctx, 5..=10);
        let invalid_chars = upper_string(ctx, 2..=4);
        let len = body_content.len();
        let mut decoder = ResponseDecoder::new();
        let initial_data = format!(
            "HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n{:x}\r\n{}",
            len, body_content
        );
        decoder
            .feed(initial_data.as_bytes())
            .expect("ボディのデコードは成功するはず (実装バグ)");
        let (_, _) = decoder
            .decode_headers()
            .expect("結果は存在するはず (実装バグ)")
            .expect("ボディのデコードは成功するはず (実装バグ)");

        // チャンクサイズを処理
        decoder
            .progress()
            .expect("ボディのデコードは成功するはず (実装バグ)");
        // チャンクデータを消費（CRLF はまだない）
        decoder
            .consume_body(len)
            .expect("ボディのデコードは成功するはず (実装バグ)");

        // 不正な CRLF を追加
        decoder
            .feed(invalid_chars.as_bytes())
            .expect("ボディのデコードは成功するはず (実装バグ)");
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
fn prop_request_chunked_invalid_crlf_error() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        // リクエストでもチャンクの CRLF 検証
        let body_content = lower_string(ctx, 5..=10);
        let invalid_chars = upper_string(ctx, 2..=4);
        let len = body_content.len();
        let data = format!(
            "POST / HTTP/1.1\r\nHost: localhost\r\nTransfer-Encoding: chunked\r\n\r\n{:x}\r\n{}{}",
            len, body_content, invalid_chars
        );
        let mut decoder = RequestDecoder::new();
        decoder
            .feed(data.as_bytes())
            .expect("ボディのデコードは成功するはず (実装バグ)");
        let (_, _) = decoder
            .decode_headers()
            .expect("結果は存在するはず (実装バグ)")
            .expect("ボディのデコードは成功するはず (実装バグ)");

        decoder
            .progress()
            .expect("ボディのデコードは成功するはず (実装バグ)");
        let peeked = decoder
            .peek_body()
            .expect("ボディのデコードは成功するはず (実装バグ)");
        assert_eq!(peeked, body_content.as_bytes());
        let result = decoder.consume_body(len);
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
// Chunked Keep-Alive 連続デコード PBT
// ========================================

#[test]
fn prop_decode_multiple_chunked_responses_with_body_limit() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let body1_len = noprop::sample_usize_in(ctx, 10..50);
        let body2_len = noprop::sample_usize_in(ctx, 10..50);
        let limits = DecoderLimits {
            max_body_size: 100,
            ..DecoderLimits::default()
        };
        let mut decoder = ResponseDecoder::with_limits(limits);

        let body1 = "x".repeat(body1_len);
        let body2 = "y".repeat(body2_len);
        let resp1 = format!(
            "HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n{:x}\r\n{}\r\n0\r\n\r\n",
            body1.len(),
            body1
        );
        let resp2 = format!(
            "HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n{:x}\r\n{}\r\n0\r\n\r\n",
            body2.len(),
            body2
        );

        decoder
            .feed(resp1.as_bytes())
            .expect("ボディのデコードは成功するはず (実装バグ)");
        decoder
            .feed(resp2.as_bytes())
            .expect("ボディのデコードは成功するはず (実装バグ)");

        // 1 回目のデコード
        let response1 = decoder
            .decode()
            .expect("結果は存在するはず (実装バグ)")
            .expect("ボディのデコードは成功するはず (実装バグ)");
        assert_eq!(response1.body_bytes().map(<[u8]>::len), Some(body1_len));

        // 2 回目のデコード
        let response2 = decoder
            .decode()
            .expect("結果は存在するはず (実装バグ)")
            .expect("ボディのデコードは成功するはず (実装バグ)");
        assert_eq!(response2.body_bytes().map(<[u8]>::len), Some(body2_len));
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
fn prop_decode_multiple_chunked_requests_with_body_limit() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let body1_len = noprop::sample_usize_in(ctx, 10..50);
        let body2_len = noprop::sample_usize_in(ctx, 10..50);
        let limits = DecoderLimits {
            max_body_size: 100,
            ..DecoderLimits::default()
        };
        let mut decoder = RequestDecoder::with_limits(limits);

        let body1 = "a".repeat(body1_len);
        let body2 = "b".repeat(body2_len);
        let req1 = format!(
            "POST /1 HTTP/1.1\r\nHost: localhost\r\nTransfer-Encoding: chunked\r\n\r\n{:x}\r\n{}\r\n0\r\n\r\n",
            body1.len(), body1
        );
        let req2 = format!(
            "POST /2 HTTP/1.1\r\nHost: localhost\r\nTransfer-Encoding: chunked\r\n\r\n{:x}\r\n{}\r\n0\r\n\r\n",
            body2.len(), body2
        );

        decoder.feed(req1.as_bytes()).expect("ボディのデコードは成功するはず (実装バグ)");
        decoder.feed(req2.as_bytes()).expect("ボディのデコードは成功するはず (実装バグ)");

        let request1 = decoder.decode().expect("結果は存在するはず (実装バグ)").expect("ボディのデコードは成功するはず (実装バグ)");
        assert_eq!(request1.body_bytes().map(<[u8]>::len), Some(body1_len));

        let request2 = decoder.decode().expect("結果は存在するはず (実装バグ)").expect("ボディのデコードは成功するはず (実装バグ)");
        assert_eq!(request2.body_bytes().map(<[u8]>::len), Some(body2_len));
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
fn prop_decode_multiple_chunked_responses_keep_alive_pbt() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let response_count = noprop::sample_usize_in(ctx, 2..4);
        let mut bodies = Vec::new();
        for _ in 0..response_count {
            let body_len = noprop::sample_usize_in(ctx, 0..=64);
            bodies.push(noprop::sample_bytes_vec(ctx, body_len));
        }
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
        decoder
            .feed(&all_data)
            .expect("ボディのデコードは成功するはず (実装バグ)");

        // decode() を連続して呼ぶ
        for body_data in &bodies {
            let response = decoder
                .decode()
                .expect("結果は存在するはず (実装バグ)")
                .expect("ボディのデコードは成功するはず (実装バグ)");
            assert_eq!(response.body_bytes(), Some(body_data.as_slice()));
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
fn prop_decode_multiple_chunked_requests_keep_alive_pbt() -> noprop::TestResult {
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

        // chunked リクエストを生成してバッファに追加
        let mut all_data = Vec::new();
        for (i, body_data) in bodies.iter().enumerate() {
            let mut data = format!(
                "POST /{} HTTP/1.1\r\nHost: localhost\r\nTransfer-Encoding: chunked\r\n\r\n",
                i
            )
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
        decoder
            .feed(&all_data)
            .expect("ボディのデコードは成功するはず (実装バグ)");

        // decode() を連続して呼ぶ
        for body_data in &bodies {
            let request = decoder
                .decode()
                .expect("結果は存在するはず (実装バグ)")
                .expect("ボディのデコードは成功するはず (実装バグ)");
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
// リクエストとレスポンスの incomplete チャンクテスト
// ========================================

#[test]
fn prop_request_incomplete_chunk_size() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        // 不完全なチャンクサイズ行は None
        let size = noprop::sample_usize_in(ctx, 1..100);
        let data = format!(
            "POST / HTTP/1.1\r\nHost: localhost\r\nTransfer-Encoding: chunked\r\n\r\n{:x}",
            size
        );
        let mut decoder = RequestDecoder::new();
        decoder
            .feed(data.as_bytes())
            .expect("ボディのデコードは成功するはず (実装バグ)");
        let (_, _) = decoder
            .decode_headers()
            .expect("結果は存在するはず (実装バグ)")
            .expect("ボディのデコードは成功するはず (実装バグ)");
        assert!(decoder.peek_body().is_none());
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
fn prop_request_incomplete_chunk_data() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        // 不完全なチャンクデータは部分データを返す
        let chunk_size = noprop::sample_usize_in(ctx, 10..100);
        let partial_size = noprop::sample_usize_in(ctx, 1..10);
        let partial_data = "x".repeat(partial_size);
        let data = format!(
            "POST / HTTP/1.1\r\nHost: localhost\r\nTransfer-Encoding: chunked\r\n\r\n{:x}\r\n{}",
            chunk_size, partial_data
        );
        let mut decoder = RequestDecoder::new();
        decoder
            .feed(data.as_bytes())
            .expect("ボディのデコードは成功するはず (実装バグ)");
        let (_, _) = decoder
            .decode_headers()
            .expect("結果は存在するはず (実装バグ)")
            .expect("ボディのデコードは成功するはず (実装バグ)");
        decoder
            .progress()
            .expect("ボディのデコードは成功するはず (実装バグ)");
        let peeked = decoder
            .peek_body()
            .expect("ボディのデコードは成功するはず (実装バグ)");
        assert_eq!(peeked, partial_data.as_bytes());
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
fn prop_request_incomplete_trailer() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let body_content = lower_string(ctx, 1..=32);
        // 禁止フィールドと衝突しない `X-` prefix の名前のみ生成する
        // (本テストは「不完全な trailer 行は Complete に到達しない」を確認するため、
        // 禁止判定で reject されてもテスト本来の意図とは別軸の reject になり
        // 結論が曖昧になるのを避ける)。
        let trailer_name = format!("X-{}", alpha_string(ctx, 0..=14));
        // 不完全なトレーラーは Complete に到達してはならない
        let len = body_content.len();
        let data = format!(
            "POST / HTTP/1.1\r\nHost: localhost\r\nTransfer-Encoding: chunked\r\n\r\n{:x}\r\n{}\r\n0\r\n{}: value",
            len, body_content, trailer_name
        );
        let mut decoder = RequestDecoder::new();
        decoder.feed(data.as_bytes()).expect("ボディのデコードは成功するはず (実装バグ)");
        let (_, _) = decoder.decode_headers().expect("結果は存在するはず (実装バグ)").expect("ボディのデコードは成功するはず (実装バグ)");

        // 不完全なトレーラ行は Complete に到達せず、最終的に NeedData で停止する。
        // 多段遷移 (BodyChunkedSize → ChunkedTrailer) は Advanced を経由するため、
        // ループで NeedData / Complete のいずれかに収束するまで進める。
        let final_result = loop {
            if let Some(data) = decoder.peek_body() {
                let len = data.len();
                match decoder.consume_body(len).expect("ボディのデコードは成功するはず (実装バグ)") {
                    r @ BodyProgress::Complete { .. } => break r,
                    BodyProgress::Advanced | BodyProgress::NeedData => continue,
                }
            }
            match decoder.progress().expect("ボディのデコードは成功するはず (実装バグ)") {
                r @ BodyProgress::Complete { .. } => break r,
                BodyProgress::Advanced => continue,
                r @ BodyProgress::NeedData => break r,
            }
        };
        let is_complete = matches!(final_result, BodyProgress::Complete { .. });
        assert!(!is_complete);
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
// ChunkLineTooLong PBT
// ========================================

#[test]
fn response_decoder_chunk_line_too_long() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let ext_len = noprop::sample_usize_in(ctx, 100..200);
        let limits = DecoderLimits {
            max_chunk_line_size: 64,
            ..DecoderLimits::default()
        };
        let mut decoder = ResponseDecoder::with_limits(limits);
        // Transfer-Encoding: chunked のレスポンス
        decoder
            .feed(b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n")
            .expect("ボディのデコードは成功するはず (実装バグ)");
        let (_, body_kind) = decoder
            .decode_headers()
            .expect("結果は存在するはず (実装バグ)")
            .expect("ボディのデコードは成功するはず (実装バグ)");
        assert_eq!(body_kind, BodyKind::Chunked);

        // max_chunk_line_size を超える長いチャンク拡張を持つチャンクサイズ行
        let ext = "x".repeat(ext_len);
        let chunk_line = format!("5;ext={}\r\nhello\r\n0\r\n\r\n", ext);
        decoder
            .feed(chunk_line.as_bytes())
            .expect("ボディのデコードは成功するはず (実装バグ)");

        // progress() でチャンクサイズ行をパースしようとするとエラー
        let result = decoder.progress();
        let is_chunk_line_too_long = matches!(result, Err(Error::ChunkLineTooLong { .. }));
        assert!(
            is_chunk_line_too_long,
            "ChunkLineTooLong を期待したが {:?} だった",
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
fn request_decoder_chunk_line_too_long() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let ext_len = noprop::sample_usize_in(ctx, 100..200);
        let limits = DecoderLimits {
            max_chunk_line_size: 64,
            ..DecoderLimits::default()
        };
        let mut decoder = RequestDecoder::with_limits(limits);
        // Transfer-Encoding: chunked のリクエスト
        decoder
            .feed(b"POST / HTTP/1.1\r\nHost: localhost\r\nTransfer-Encoding: chunked\r\n\r\n")
            .expect("ボディのデコードは成功するはず (実装バグ)");
        let (_, body_kind) = decoder
            .decode_headers()
            .expect("結果は存在するはず (実装バグ)")
            .expect("ボディのデコードは成功するはず (実装バグ)");
        assert_eq!(body_kind, BodyKind::Chunked);

        // max_chunk_line_size を超える長いチャンク拡張を持つチャンクサイズ行
        let ext = "x".repeat(ext_len);
        let chunk_line = format!("5;ext={}\r\nhello\r\n0\r\n\r\n", ext);
        decoder
            .feed(chunk_line.as_bytes())
            .expect("ボディのデコードは成功するはず (実装バグ)");

        // progress() でチャンクサイズ行をパースしようとするとエラー
        let result = decoder.progress();
        let is_chunk_line_too_long = matches!(result, Err(Error::ChunkLineTooLong { .. }));
        assert!(
            is_chunk_line_too_long,
            "ChunkLineTooLong を期待したが {:?} だった",
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
// decode() ラウンドトリップの PBT
// ========================================

/// リクエストの chunked ボディの decode() ラウンドトリップ
#[test]
fn prop_request_decode_chunked_roundtrip() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let chunks = random_chunks(ctx);
        let headers = b"POST / HTTP/1.1\r\nHost: localhost\r\nTransfer-Encoding: chunked\r\n\r\n";
        let chunk_refs: Vec<&[u8]> = chunks.iter().map(|c| c.as_slice()).collect();
        let chunked_body = encode_chunks(&chunk_refs);

        let mut full = headers.to_vec();
        full.extend_from_slice(&chunked_body);

        let mut decoder = RequestDecoder::new();
        decoder
            .feed(&full)
            .expect("ボディのデコードは成功するはず (実装バグ)");
        let request = decoder
            .decode()
            .expect("結果は存在するはず (実装バグ)")
            .expect("ボディのデコードは成功するはず (実装バグ)");

        let expected_body: Vec<u8> = chunks.into_iter().flatten().collect();
        assert_eq!(request.body_bytes(), Some(expected_body.as_slice()));
        assert_eq!(request.method(), "POST");
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

/// レスポンスの chunked ボディの decode() ラウンドトリップ
#[test]
fn prop_response_decode_chunked_roundtrip() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let chunks = random_chunks(ctx);
        let headers = b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n";
        let chunk_refs: Vec<&[u8]> = chunks.iter().map(|c| c.as_slice()).collect();
        let chunked_body = encode_chunks(&chunk_refs);

        let mut full = headers.to_vec();
        full.extend_from_slice(&chunked_body);

        let mut decoder = ResponseDecoder::new();
        decoder
            .feed(&full)
            .expect("ボディのデコードは成功するはず (実装バグ)");
        let response = decoder
            .decode()
            .expect("結果は存在するはず (実装バグ)")
            .expect("ボディのデコードは成功するはず (実装バグ)");

        let expected_body: Vec<u8> = chunks.into_iter().flatten().collect();
        assert_eq!(response.body_bytes(), Some(expected_body.as_slice()));
        assert_eq!(response.status_code(), 200);
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
// Content-Length ボディの decode() ラウンドトリップ PBT
// ========================================

/// Content-Length ボディの decode() ラウンドトリップ (リクエスト)
#[test]
fn prop_request_decode_content_length_roundtrip() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let body_len = noprop::sample_usize_in(ctx, 1..256);
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
            .expect("ボディのデコードは成功するはず (実装バグ)");
        let request = decoder
            .decode()
            .expect("結果は存在するはず (実装バグ)")
            .expect("ボディのデコードは成功するはず (実装バグ)");
        assert_eq!(request.body_bytes(), Some(body_data.as_slice()));
        assert_eq!(request.method(), "POST");
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

/// Content-Length ボディの decode() ラウンドトリップ (レスポンス)
#[test]
fn prop_response_decode_content_length_roundtrip() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let body_len = noprop::sample_usize_in(ctx, 1..256);
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
            .expect("ボディのデコードは成功するはず (実装バグ)");
        let response = decoder
            .decode()
            .expect("結果は存在するはず (実装バグ)")
            .expect("ボディのデコードは成功するはず (実装バグ)");
        assert_eq!(response.body_bytes(), Some(body_data.as_slice()));
        assert_eq!(response.status_code(), 200);
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
// Content-Length 部分消費の PBT
// ========================================

/// Content-Length ボディを複数回に分けて消費
#[test]
fn prop_request_partial_body_consume() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let body_len = noprop::sample_usize_in(ctx, 4..256);
        let body_data = noprop::sample_bytes_vec(ctx, body_len);
        let split_ratio = noprop::sample_usize_in(ctx, 1..4);
        let data = format!(
            "POST / HTTP/1.1\r\nHost: localhost\r\nContent-Length: {}\r\n\r\n",
            body_data.len()
        );
        let mut full = data.into_bytes();
        full.extend_from_slice(&body_data);

        let mut decoder = RequestDecoder::new();
        decoder
            .feed(&full)
            .expect("ボディのデコードは成功するはず (実装バグ)");
        let (_, body_kind) = decoder
            .decode_headers()
            .expect("結果は存在するはず (実装バグ)")
            .expect("ボディのデコードは成功するはず (実装バグ)");
        assert_eq!(body_kind, BodyKind::ContentLength(body_data.len() as u64));

        // 複数回に分けて消費
        let first_len = body_data.len() / split_ratio.max(1);
        let first_len = first_len.max(1);
        let mut consumed = Vec::new();

        loop {
            let peeked = decoder.peek_body();
            if peeked.is_none() {
                // progress を試す
                match decoder
                    .progress()
                    .expect("ボディのデコードは成功するはず (実装バグ)")
                {
                    BodyProgress::Complete { .. } => break,
                    BodyProgress::Advanced => continue,
                    BodyProgress::NeedData => break,
                }
            }
            if let Some(data) = decoder.peek_body() {
                let take = data.len().min(first_len);
                consumed.extend_from_slice(&data[..take]);
                match decoder
                    .consume_body(take)
                    .expect("ボディのデコードは成功するはず (実装バグ)")
                {
                    BodyProgress::Complete { .. } => break,
                    BodyProgress::Advanced | BodyProgress::NeedData => continue,
                }
            } else {
                break;
            }
        }

        assert_eq!(&consumed, &body_data);
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
// Unicode 空白 OWS の reject
// ========================================
//
// RFC 9110 Section 5.6.3 では OWS = *( SP / HTAB ) と定義されており、
// trim_ows は SP/HTAB のみ除去する。Unicode 空白 (NBSP / U+2028 等) を
// 含む Transfer-Encoding は token として不正と判定され reject される。

/// Unicode 空白の UTF-8 表現 (NBSP / U+2028 / U+2000 / U+205F) を生成する
fn unicode_whitespace_bytes(ctx: &mut noprop::TestCaseContext) -> &'static [u8] {
    match noprop::sample_usize_in(ctx, 0..4) {
        0 => &b"\xC2\xA0"[..],     // U+00A0 NBSP
        1 => &b"\xE2\x80\xA8"[..], // U+2028 LINE SEPARATOR
        2 => &b"\xE2\x80\x80"[..], // U+2000 EN QUAD
        _ => &b"\xE2\x81\x9F"[..], // U+205F MEDIUM MATHEMATICAL SPACE
    }
}

/// 任意の Unicode 空白を前後に挿入した Transfer-Encoding: chunked は
/// request 経路で reject される (HRS 経路の遮断)。
#[test]
fn prop_request_te_unicode_whitespace_rejected() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let ws = unicode_whitespace_bytes(ctx);
        let position = noprop::sample_usize_in(ctx, 0..3) as u8;
        let prefix = b"POST / HTTP/1.1\r\nHost: example.com\r\nTransfer-Encoding: ";
        let mut payload: Vec<u8> = prefix.to_vec();
        match position {
            0 => {
                payload.extend_from_slice(ws);
                payload.extend_from_slice(b"chunked");
            }
            1 => {
                payload.extend_from_slice(b"chunked");
                payload.extend_from_slice(ws);
            }
            _ => {
                payload.extend_from_slice(ws);
                payload.extend_from_slice(b"chunked");
                payload.extend_from_slice(ws);
            }
        }
        payload.extend_from_slice(b"\r\n\r\n");

        let mut decoder = RequestDecoder::new();
        decoder
            .feed(&payload)
            .expect("ボディのデコードは成功するはず (実装バグ)");
        let result = decoder.decode_headers();
        assert!(
            result.is_err(),
            "Unicode 空白を含む TE は reject されるべき"
        );
        let err = result.unwrap_err();
        assert!(
            matches!(&err, Error::InvalidData(_)),
            "InvalidData を期待: {:?}",
            err
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

/// response 経路でも同様に reject される。
#[test]
fn prop_response_te_unicode_whitespace_rejected() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let ws = unicode_whitespace_bytes(ctx);
        let position = noprop::sample_usize_in(ctx, 0..3) as u8;
        let prefix = b"HTTP/1.1 200 OK\r\nTransfer-Encoding: ";
        let mut payload: Vec<u8> = prefix.to_vec();
        match position {
            0 => {
                payload.extend_from_slice(ws);
                payload.extend_from_slice(b"chunked");
            }
            1 => {
                payload.extend_from_slice(b"chunked");
                payload.extend_from_slice(ws);
            }
            _ => {
                payload.extend_from_slice(ws);
                payload.extend_from_slice(b"chunked");
                payload.extend_from_slice(ws);
            }
        }
        payload.extend_from_slice(b"\r\n\r\n");

        let mut decoder = ResponseDecoder::new();
        decoder
            .feed(&payload)
            .expect("ボディのデコードは成功するはず (実装バグ)");
        let result = decoder.decode_headers();
        assert!(
            result.is_err(),
            "Unicode 空白を含む TE は reject されるべき"
        );
        let err = result.unwrap_err();
        assert!(
            matches!(&err, Error::InvalidData(_)),
            "InvalidData を期待: {:?}",
            err
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

/// SP / HTAB のみで構成された OWS は引き続き受理される (リグレッション防止)。
#[test]
fn prop_request_te_ascii_ows_accepted() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let leading_len = noprop::sample_usize_in(ctx, 0..4);
        let mut leading = Vec::new();
        for _ in 0..leading_len {
            let b = match noprop::sample_usize_in(ctx, 0..2) {
                0 => b' ',
                _ => b'\t',
            };
            leading.push(b);
        }
        let trailing_len = noprop::sample_usize_in(ctx, 0..4);
        let mut trailing = Vec::new();
        for _ in 0..trailing_len {
            let b = match noprop::sample_usize_in(ctx, 0..2) {
                0 => b' ',
                _ => b'\t',
            };
            trailing.push(b);
        }
        let mut payload: Vec<u8> =
            b"POST / HTTP/1.1\r\nHost: example.com\r\nTransfer-Encoding: ".to_vec();
        payload.extend_from_slice(&leading);
        payload.extend_from_slice(b"chunked");
        payload.extend_from_slice(&trailing);
        payload.extend_from_slice(b"\r\n\r\n");

        let mut decoder = RequestDecoder::new();
        decoder
            .feed(&payload)
            .expect("ボディのデコードは成功するはず (実装バグ)");
        let (_, body_kind) = decoder
            .decode_headers()
            .expect("結果は存在するはず (実装バグ)")
            .expect("ボディのデコードは成功するはず (実装バグ)");
        assert_eq!(body_kind, BodyKind::Chunked);
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
