//! ヘッダーパース・HttpHead トレイトの PBT

use shiguredo_http11::{
    BodyKind, HeaderName, HttpHead, Request, RequestDecoder, Response, ResponseDecoder,
    ResponseHead, StatusClass,
};

use super::{
    http_method, http_uri, invalid_header_name_char, reason_phrase, status_code,
    valid_header_name_special_char,
};

/// PBT 用に `ResponseHead` を直接構築するヘルパー
///
/// 本クレートは `ResponseHead` のフィールドを非公開化しており、外部からは
/// `with_version` + `add_header` のビルダー API のみで構築できる。
/// PBT ではトレイトメソッド (`is_keep_alive` / `is_chunked` 等) の動作を
/// 任意の `(version, status_code, reason_phrase, headers)` 組合せで検証
/// したいため、本ヘルパーで一括構築する。バリデーション失敗時は panic
/// する (テストデータは生成器側で valid 範囲に収めている前提)。
fn make_response_head(
    version: &str,
    status_code: u16,
    reason_phrase: &str,
    headers: Vec<(HeaderName, String)>,
) -> ResponseHead {
    let mut head = ResponseHead::with_version(version, status_code, reason_phrase)
        .expect("テスト入力は valid な version / status_code / reason_phrase 前提");
    for (name, value) in headers {
        head.add_header(name, &value)
            .expect("テスト入力は valid な header name / value 前提");
    }
    head
}

/// HTTP バージョン (HTTP/1.0 と HTTP/1.1) の 1 つを生成する
fn http_version(ctx: &mut noprop::TestCaseContext) -> String {
    match noprop::sample_usize_in(ctx, 0..2) {
        0 => "HTTP/1.0".to_string(),
        _ => "HTTP/1.1".to_string(),
    }
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
        s.push(char::from(b'a' + noprop::sample_usize_in(ctx, 0..26) as u8));
    }
    s
}

/// chunked 以外の Transfer-Encoding トークンを生成する
///
/// 固定トークン 5 つのうち chunked を除外した 4 つから一様に選ぶ
/// (valid-by-construction)。
fn non_chunked_transfer_encoding_token(ctx: &mut noprop::TestCaseContext) -> String {
    match noprop::sample_usize_in(ctx, 0..4) {
        0 => "gzip".to_string(),
        1 => "deflate".to_string(),
        2 => "compress".to_string(),
        _ => "identity".to_string(),
    }
}

// ========================================
// ヘッダーパースエラーの PBT
// ========================================

#[test]
fn prop_header_obs_fold_space_error() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        // obs-fold (行頭スペース) はエラー
        let header_name = alpha_string(ctx, 1..=16);
        let header_value = alnum_string(ctx, 1..=16);
        let data = format!(
            "GET / HTTP/1.1\r\nHost: localhost\r\n {}: {}\r\n\r\n",
            header_name, header_value
        );
        let mut decoder = RequestDecoder::new();
        decoder
            .feed(data.as_bytes())
            .expect("ヘッダーのデコードは成功するはず (実装バグ)");
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
fn prop_header_obs_fold_tab_error() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        // obs-fold (行頭タブ) はエラー
        let header_name = alpha_string(ctx, 1..=16);
        let header_value = alnum_string(ctx, 1..=16);
        let data = format!(
            "GET / HTTP/1.1\r\nHost: localhost\r\n\t{}: {}\r\n\r\n",
            header_name, header_value
        );
        let mut decoder = RequestDecoder::new();
        decoder
            .feed(data.as_bytes())
            .expect("ヘッダーのデコードは成功するはず (実装バグ)");
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
fn prop_header_contains_cr_error() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        // ヘッダー名に CR を含むとエラー
        let prefix = alpha_string(ctx, 1..=8);
        let suffix = alpha_string(ctx, 1..=8);
        let data = format!(
            "GET / HTTP/1.1\r\nHost: localhost\r\n{}\r{}: value\r\n\r\n",
            prefix, suffix
        );
        let mut decoder = RequestDecoder::new();
        decoder
            .feed(data.as_bytes())
            .expect("ヘッダーのデコードは成功するはず (実装バグ)");
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
fn prop_header_contains_lf_error() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        // ヘッダー名に LF を含むとエラー
        let prefix = alpha_string(ctx, 1..=8);
        let suffix = alpha_string(ctx, 1..=8);
        let data = format!(
            "GET / HTTP/1.1\r\nHost: localhost\r\n{}\n{}: value\r\n\r\n",
            prefix, suffix
        );
        let mut decoder = RequestDecoder::new();
        decoder
            .feed(data.as_bytes())
            .expect("ヘッダーのデコードは成功するはず (実装バグ)");
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
fn prop_header_missing_colon_error() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        // コロンがないとエラー
        let header_name = alpha_string(ctx, 1..=16);
        let header_value = alnum_string(ctx, 1..=16);
        let data = format!(
            "GET / HTTP/1.1\r\nHost: localhost\r\n{} {}\r\n\r\n",
            header_name, header_value
        );
        let mut decoder = RequestDecoder::new();
        decoder
            .feed(data.as_bytes())
            .expect("ヘッダーのデコードは成功するはず (実装バグ)");
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
fn prop_header_empty_name_error() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        // 空のヘッダー名はエラー
        let header_value = alnum_string(ctx, 1..=16);
        let data = format!(
            "GET / HTTP/1.1\r\nHost: localhost\r\n: {}\r\n\r\n",
            header_value
        );
        let mut decoder = RequestDecoder::new();
        decoder
            .feed(data.as_bytes())
            .expect("ヘッダーのデコードは成功するはず (実装バグ)");
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
fn prop_header_name_with_space_error() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        // ヘッダー名にスペースを含むとエラー
        let prefix = alpha_string(ctx, 1..=8);
        let suffix = alpha_string(ctx, 1..=8);
        let data = format!(
            "GET / HTTP/1.1\r\nHost: localhost\r\n{} {}: value\r\n\r\n",
            prefix, suffix
        );
        let mut decoder = RequestDecoder::new();
        decoder
            .feed(data.as_bytes())
            .expect("ヘッダーのデコードは成功するはず (実装バグ)");
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
fn prop_header_name_trailing_space_error() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        // ヘッダー名の後にスペースがあるとエラー
        let header_name = alpha_string(ctx, 1..=16);
        let data = format!(
            "GET / HTTP/1.1\r\nHost: localhost\r\n{} : value\r\n\r\n",
            header_name
        );
        let mut decoder = RequestDecoder::new();
        decoder
            .feed(data.as_bytes())
            .expect("ヘッダーのデコードは成功するはず (実装バグ)");
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
fn prop_header_invalid_name_char_error() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        // 無効な文字を含むヘッダー名はエラー
        let prefix = alpha_string(ctx, 1..=8);
        let invalid_char = invalid_header_name_char(ctx);
        let suffix = alpha_string(ctx, 1..=8);
        let data = format!(
            "GET / HTTP/1.1\r\nHost: localhost\r\n{}{}{}: value\r\n\r\n",
            prefix, invalid_char, suffix
        );
        let mut decoder = RequestDecoder::new();
        decoder
            .feed(data.as_bytes())
            .expect("ヘッダーのデコードは成功するはず (実装バグ)");
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
fn prop_valid_header_name_chars() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        // 有効な特殊文字を含むヘッダー名は OK
        let prefix = alpha_string(ctx, 1..=8);
        let special_char = valid_header_name_special_char(ctx);
        let suffix = alpha_string(ctx, 1..=8);
        let header_name = format!("{}{}{}", prefix, special_char, suffix);
        let data = format!(
            "GET / HTTP/1.1\r\nHost: localhost\r\n{}: value\r\n\r\n",
            header_name
        );
        let mut decoder = RequestDecoder::new();
        decoder
            .feed(data.as_bytes())
            .expect("ヘッダーのデコードは成功するはず (実装バグ)");
        assert!(decoder.decode_headers().is_ok());
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
fn prop_header_value_leading_trailing_spaces() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        // ヘッダー値の前後スペースは許可され、トリムされる
        let header_name = alpha_string(ctx, 1..=16);
        let value = alnum_string(ctx, 1..=16);
        let leading_spaces = noprop::sample_usize_in(ctx, 0..4);
        let trailing_spaces = noprop::sample_usize_in(ctx, 0..4);
        let padded_value = format!(
            "{}{}{}",
            " ".repeat(leading_spaces),
            value,
            " ".repeat(trailing_spaces)
        );
        let data = format!(
            "GET / HTTP/1.1\r\nHost: localhost\r\n{}:{}\r\n\r\n",
            header_name, padded_value
        );
        let mut decoder = RequestDecoder::new();
        decoder
            .feed(data.as_bytes())
            .expect("ヘッダーのデコードは成功するはず (実装バグ)");
        let result = decoder.decode_headers();
        assert!(result.is_ok());
        if let Ok(Some((head, _))) = result {
            let header_value = head
                .get_header(&header_name)
                .expect("ヘッダーのデコードは成功するはず (実装バグ)");
            assert_eq!(header_value, value);
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
// Transfer-Encoding と Content-Length のエラー PBT
// ========================================

#[test]
fn prop_transfer_encoding_and_content_length_error() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        // Transfer-Encoding と Content-Length の両方があるとエラー
        let content_length = noprop::sample_usize_in(ctx, 1..1000);
        let data = format!(
            "GET / HTTP/1.1\r\nHost: localhost\r\nTransfer-Encoding: chunked\r\nContent-Length: {}\r\n\r\n",
            content_length
        );
        let mut decoder = RequestDecoder::new();
        decoder.feed(data.as_bytes()).expect("ヘッダーのデコードは成功するはず (実装バグ)");
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
fn prop_transfer_encoding_unsupported_error() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        // chunked 以外の Transfer-Encoding はエラー
        let coding = non_chunked_transfer_encoding_token(ctx);
        let data = format!(
            "GET / HTTP/1.1\r\nHost: localhost\r\nTransfer-Encoding: {}\r\n\r\n",
            coding
        );
        let mut decoder = RequestDecoder::new();
        decoder
            .feed(data.as_bytes())
            .expect("ヘッダーのデコードは成功するはず (実装バグ)");
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
fn prop_transfer_encoding_duplicate_or_unsupported_with_empty_elements() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        // RFC 9110 Section 5.6.1.2: 空リスト要素は無視する
        // 空要素を無視した後も duplicate chunked または unsupported coding でエラー
        let before_comma = noprop::sample_usize_in(ctx, 0..3);
        let after_comma = noprop::sample_usize_in(ctx, 0..3);
        let data = format!(
            "GET / HTTP/1.1\r\nHost: localhost\r\nTransfer-Encoding: {},,{}\r\n\r\n",
            "chunked".repeat(before_comma.max(1)),
            "chunked".repeat(after_comma.max(1))
        );
        let mut decoder = RequestDecoder::new();
        decoder
            .feed(data.as_bytes())
            .expect("ヘッダーのデコードは成功するはず (実装バグ)");
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
fn prop_transfer_encoding_empty_value_accepted() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        // RFC 9110 Section 5.6.1.2: 空リスト要素は無視する (MUST)
        // 有効要素なし → Transfer-Encoding なしとして受理する
        let method = http_method(ctx);
        let data = format!(
            "{} / HTTP/1.1\r\nHost: localhost\r\nTransfer-Encoding: \r\n\r\n",
            method
        );
        let mut decoder = RequestDecoder::new();
        decoder
            .feed(data.as_bytes())
            .expect("ヘッダーのデコードは成功するはず (実装バグ)");
        assert!(decoder.decode_headers().is_ok());
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
fn prop_transfer_encoding_case_insensitive() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        // Transfer-Encoding は大文字小文字を区別しない
        let chunked_case = match noprop::sample_usize_in(ctx, 0..4) {
            0 => "chunked",
            1 => "CHUNKED",
            2 => "Chunked",
            _ => "cHuNkEd",
        };
        let data = format!(
            "HTTP/1.1 200 OK\r\ntransfer-encoding: {}\r\n\r\n5\r\nhello\r\n0\r\n\r\n",
            chunked_case
        );
        let mut decoder = ResponseDecoder::new();
        decoder
            .feed(data.as_bytes())
            .expect("ヘッダーのデコードは成功するはず (実装バグ)");
        let (head, body_kind) = decoder
            .decode_headers()
            .expect("結果は存在するはず (実装バグ)")
            .expect("ヘッダーのデコードは成功するはず (実装バグ)");
        assert_eq!(head.status_code(), 200);
        assert_eq!(body_kind, BodyKind::Chunked);

        // ボディを読み取る
        let mut body = Vec::new();
        loop {
            if let Some(data) = decoder.peek_body() {
                body.extend_from_slice(data);
                let len = data.len();
                if let shiguredo_http11::BodyProgress::Complete { .. } = decoder
                    .consume_body(len)
                    .expect("ヘッダーのデコードは成功するはず (実装バグ)")
                {
                    break;
                }
            } else if let shiguredo_http11::BodyProgress::Complete { .. } = decoder
                .progress()
                .expect("ヘッダーのデコードは成功するはず (実装バグ)")
            {
                break;
            }
        }
        assert_eq!(body, b"hello");
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
fn prop_multiple_transfer_encoding_chunked_error() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        // RFC 9112: chunked は一度だけ指定可能、重複はエラー
        let count = noprop::sample_usize_in(ctx, 2..4);
        let headers = (0..count)
            .map(|_| "Transfer-Encoding: chunked")
            .collect::<Vec<_>>()
            .join("\r\n");
        let data = format!(
            "HTTP/1.1 200 OK\r\n{}\r\n\r\n5\r\nhello\r\n0\r\n\r\n",
            headers
        );
        let mut decoder = ResponseDecoder::new();
        decoder
            .feed(data.as_bytes())
            .expect("ヘッダーのデコードは成功するはず (実装バグ)");
        // 重複 chunked はエラーを返すべき
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
fn prop_single_transfer_encoding_chunked_ok() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        // 単一の chunked ヘッダーは OK
        let body = lower_string(ctx, 1..=100);
        let chunk_size = format!("{:x}", body.len());
        let data = format!(
            "HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n{}\r\n{}\r\n0\r\n\r\n",
            chunk_size, body
        );
        let mut decoder = ResponseDecoder::new();
        decoder
            .feed(data.as_bytes())
            .expect("ヘッダーのデコードは成功するはず (実装バグ)");
        let (_, body_kind) = decoder
            .decode_headers()
            .expect("結果は存在するはず (実装バグ)")
            .expect("ヘッダーのデコードは成功するはず (実装バグ)");
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

// ========================================
// Content-Length のエラー PBT
// ========================================

#[test]
fn prop_content_length_not_number_error() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        // 数字でない Content-Length はエラー
        let invalid_value = alpha_string(ctx, 1..=8);
        let data = format!(
            "GET / HTTP/1.1\r\nHost: localhost\r\nContent-Length: {}\r\n\r\n",
            invalid_value
        );
        let mut decoder = RequestDecoder::new();
        decoder
            .feed(data.as_bytes())
            .expect("ヘッダーのデコードは成功するはず (実装バグ)");
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
fn prop_content_length_empty_error() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        // 空の Content-Length はエラー
        let method = http_method(ctx);
        let data = format!(
            "{} / HTTP/1.1\r\nHost: localhost\r\nContent-Length: \r\n\r\n",
            method
        );
        let mut decoder = RequestDecoder::new();
        decoder
            .feed(data.as_bytes())
            .expect("ヘッダーのデコードは成功するはず (実装バグ)");
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
fn prop_content_length_mismatch_error() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        // 異なる値の Content-Length はエラー
        let len1 = noprop::sample_usize_in(ctx, 1..100);
        let len2 = noprop::sample_usize_in(ctx, 101..200);
        let data = format!(
            "GET / HTTP/1.1\r\nHost: localhost\r\nContent-Length: {}\r\nContent-Length: {}\r\n\r\n",
            len1, len2
        );
        let mut decoder = RequestDecoder::new();
        decoder
            .feed(data.as_bytes())
            .expect("ヘッダーのデコードは成功するはず (実装バグ)");
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
fn prop_content_length_match_ok() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        // 同じ値の Content-Length は OK
        let length = noprop::sample_usize_in(ctx, 1..100);
        let body_content = lower_string(ctx, 1..=100);
        let body_bytes = &body_content.as_bytes()[..length.min(body_content.len())];
        let actual_len = body_bytes.len();
        let data = format!(
            "GET / HTTP/1.1\r\nHost: localhost\r\nContent-Length: {}\r\nContent-Length: {}\r\n\r\n",
            actual_len, actual_len
        );
        let mut full_data = data.into_bytes();
        full_data.extend_from_slice(body_bytes);

        let mut decoder = RequestDecoder::new();
        decoder
            .feed(&full_data)
            .expect("ヘッダーのデコードは成功するはず (実装バグ)");
        let (_, body_kind) = decoder
            .decode_headers()
            .expect("結果は存在するはず (実装バグ)")
            .expect("ヘッダーのデコードは成功するはず (実装バグ)");
        assert_eq!(body_kind, BodyKind::ContentLength(actual_len as u64));

        let mut body = Vec::new();
        while let Some(data) = decoder.peek_body() {
            body.extend_from_slice(data);
            let len = data.len();
            if let shiguredo_http11::BodyProgress::Complete { .. } = decoder
                .consume_body(len)
                .expect("ヘッダーのデコードは成功するはず (実装バグ)")
            {
                break;
            }
        }
        assert_eq!(&body, body_bytes);
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
fn prop_content_length_zero_no_body() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        // Content-Length: 0 はボディなし
        let method = http_method(ctx);
        let data = format!(
            "{} / HTTP/1.1\r\nHost: localhost\r\nContent-Length: 0\r\n\r\n",
            method
        );
        let mut decoder = RequestDecoder::new();
        decoder
            .feed(data.as_bytes())
            .expect("ヘッダーのデコードは成功するはず (実装バグ)");
        let (_, body_kind) = decoder
            .decode_headers()
            .expect("結果は存在するはず (実装バグ)")
            .expect("ヘッダーのデコードは成功するはず (実装バグ)");
        assert_eq!(body_kind, BodyKind::ContentLength(0));
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
fn prop_content_length_case_insensitive() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        // Content-Length は大文字小文字を区別しない
        let header_case = match noprop::sample_usize_in(ctx, 0..4) {
            0 => "Content-Length",
            1 => "content-length",
            2 => "CONTENT-LENGTH",
            _ => "Content-length",
        };
        let length = noprop::sample_usize_in(ctx, 1..100);
        let body_content = "x".repeat(length);
        let data = format!(
            "GET / HTTP/1.1\r\nHost: localhost\r\n{}: {}\r\n\r\n{}",
            header_case, length, body_content
        );
        let mut decoder = RequestDecoder::new();
        decoder
            .feed(data.as_bytes())
            .expect("ヘッダーのデコードは成功するはず (実装バグ)");
        let (_, body_kind) = decoder
            .decode_headers()
            .expect("結果は存在するはず (実装バグ)")
            .expect("ヘッダーのデコードは成功するはず (実装バグ)");
        assert_eq!(body_kind, BodyKind::ContentLength(length as u64));

        let mut body = Vec::new();
        while let Some(data) = decoder.peek_body() {
            body.extend_from_slice(data);
            let len = data.len();
            if let shiguredo_http11::BodyProgress::Complete { .. } = decoder
                .consume_body(len)
                .expect("ヘッダーのデコードは成功するはず (実装バグ)")
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
// is_chunked() トークン解析 PBT
// ========================================

#[test]
fn prop_is_chunked_only_chunked_token() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        // "chunked" のみ (前後にスペースあり) の場合は true
        let leading_spaces = noprop::sample_usize_in(ctx, 0..4);
        let trailing_spaces = noprop::sample_usize_in(ctx, 0..4);
        let te_value = format!(
            "{}chunked{}",
            " ".repeat(leading_spaces),
            " ".repeat(trailing_spaces)
        );
        let head = make_response_head(
            "HTTP/1.1",
            200,
            "OK",
            vec![(HeaderName::from_static(b"Transfer-Encoding"), te_value)],
        );
        assert!(head.is_chunked());
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
fn prop_is_chunked_last_token_determines_result() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);
    let chunked_first_seen = std::cell::Cell::new(0usize);
    let chunked_last_seen = std::cell::Cell::new(0usize);

    runner.run(256, |ctx| {
        // RFC 9112 Section 6.3: 最後のトークンが chunked かどうかで判定
        let other_token = non_chunked_transfer_encoding_token(ctx);
        let chunked_first = noprop::sample_bool(ctx);
        let te_value = if chunked_first {
            // "chunked, other" → 最後が chunked でない → false
            format!("chunked, {}", other_token)
        } else {
            // "other, chunked" → 最後が chunked → true
            format!("{}, chunked", other_token)
        };
        let head = make_response_head(
            "HTTP/1.1",
            200,
            "OK",
            vec![(HeaderName::from_static(b"Transfer-Encoding"), te_value)],
        );
        if chunked_first {
            // chunked が先頭に来る側の分岐が一度も実行されないと false 側の検証が空振りする
            chunked_first_seen.set(chunked_first_seen.get() + 1);
            assert!(!head.is_chunked());
        } else {
            chunked_last_seen.set(chunked_last_seen.get() + 1);
            assert!(head.is_chunked());
        }
        Ok(())
    })?;

    // p 推定値: どちらの分岐も sample_bool で確率 1/2。
    // 256 ケースでの miss 確率は (1/2)^256 ≈ 0。
    assert!(
        chunked_first_seen.get() > 0,
        "「chunked, other」側が一度も検証されなかった\n{runner}"
    );
    assert!(
        chunked_last_seen.get() > 0,
        "「other, chunked」側が一度も検証されなかった\n{runner}"
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
fn prop_is_chunked_other_token_only_returns_false() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        // chunked 以外のトークンのみの場合は false
        let token = non_chunked_transfer_encoding_token(ctx);
        let head = make_response_head(
            "HTTP/1.1",
            200,
            "OK",
            vec![(HeaderName::from_static(b"Transfer-Encoding"), token)],
        );
        assert!(!head.is_chunked());
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
fn prop_is_chunked_no_header_returns_false() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        // Transfer-Encoding ヘッダーがない場合は false
        let status_code = noprop::sample_usize_in(ctx, 200..600) as u16;
        let head = make_response_head("HTTP/1.1", status_code, "OK", vec![]);
        assert!(!head.is_chunked());
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
fn prop_is_chunked_consistency_with_body_kind() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);
    let chunked_seen = std::cell::Cell::new(0usize);
    let non_chunked_seen = std::cell::Cell::new(0usize);

    runner.run(256, |ctx| {
        // is_chunked() と BodyKind::Chunked の整合性を検証
        let use_chunked = noprop::sample_bool(ctx);
        let mut decoder = ResponseDecoder::new();
        let data = if use_chunked {
            b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n".to_vec()
        } else {
            b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\n\r\n".to_vec()
        };
        decoder
            .feed(&data)
            .expect("ヘッダーのデコードは成功するはず (実装バグ)");
        let (head, body_kind) = decoder
            .decode_headers()
            .expect("結果は存在するはず (実装バグ)")
            .expect("ヘッダーのデコードは成功するはず (実装バグ)");

        if use_chunked {
            // chunked 側の分岐が一度も実行されないと true 側の検証が空振りする
            chunked_seen.set(chunked_seen.get() + 1);
            assert!(head.is_chunked());
            assert!(matches!(body_kind, BodyKind::Chunked));
        } else {
            non_chunked_seen.set(non_chunked_seen.get() + 1);
            assert!(!head.is_chunked());
            assert!(!matches!(body_kind, BodyKind::Chunked));
        }
        Ok(())
    })?;

    // p 推定値: どちらの分岐も sample_bool で確率 1/2。
    // 256 ケースでの miss 確率は (1/2)^256 ≈ 0。
    assert!(
        chunked_seen.get() > 0,
        "chunked 側が一度も検証されなかった\n{runner}"
    );
    assert!(
        non_chunked_seen.get() > 0,
        "chunked 以外の側が一度も検証されなかった\n{runner}"
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
// is_keep_alive() トークン解析 PBT
// ========================================

#[test]
fn prop_is_keep_alive_close_token_returns_false() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        // "close" トークンがあれば false
        let version = http_version(ctx);
        let head = make_response_head(
            &version,
            200,
            "OK",
            vec![(HeaderName::from_static(b"Connection"), "close".to_string())],
        );
        assert!(!head.is_keep_alive());
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
fn prop_is_keep_alive_keep_alive_token_returns_true() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        // "keep-alive" トークンがあれば true
        let version = http_version(ctx);
        let head = make_response_head(
            &version,
            200,
            "OK",
            vec![(
                HeaderName::from_static(b"Connection"),
                "keep-alive".to_string(),
            )],
        );
        assert!(head.is_keep_alive());
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
fn prop_is_keep_alive_default_by_version() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        // HTTP/1.1 のデフォルトは keep-alive、HTTP/1.0 のデフォルトは close
        let status_code = noprop::sample_usize_in(ctx, 200..600) as u16;
        let head_11 = make_response_head("HTTP/1.1", status_code, "OK", vec![]);
        assert!(head_11.is_keep_alive());

        let head_10 = make_response_head("HTTP/1.0", status_code, "OK", vec![]);
        assert!(!head_10.is_keep_alive());
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
fn prop_is_keep_alive_close_priority_over_keep_alive() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        // close と keep-alive が両方ある場合、close が優先される
        let keep_alive_first = noprop::sample_bool(ctx);
        let conn_value = if keep_alive_first {
            "keep-alive, close".to_string()
        } else {
            "close, keep-alive".to_string()
        };
        let head = make_response_head(
            "HTTP/1.1",
            200,
            "OK",
            vec![(HeaderName::from_static(b"Connection"), conn_value)],
        );
        assert!(!head.is_keep_alive());
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
// 複数 Connection ヘッダーの is_keep_alive() PBT
// RFC 9110 Section 5.3: 複数ヘッダーはリストとして結合して処理
// ========================================

#[test]
fn prop_is_keep_alive_multiple_headers_all_keep_alive() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        // 複数の Connection: keep-alive ヘッダーがある場合は true
        let header_count = noprop::sample_usize_in(ctx, 2..5);
        let headers: Vec<(HeaderName, String)> = (0..header_count)
            .map(|_| {
                (
                    HeaderName::from_static(b"Connection"),
                    "keep-alive".to_string(),
                )
            })
            .collect();
        let head = make_response_head("HTTP/1.1", 200, "OK", headers);
        assert!(head.is_keep_alive());
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
fn prop_is_keep_alive_multiple_headers_close_in_later() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        // 最初に keep-alive、後に close がある場合は false (close 優先)
        let keep_alive_count = noprop::sample_usize_in(ctx, 1..4);
        let mut headers: Vec<(HeaderName, String)> = (0..keep_alive_count)
            .map(|_| {
                (
                    HeaderName::from_static(b"Connection"),
                    "keep-alive".to_string(),
                )
            })
            .collect();
        headers.push((HeaderName::from_static(b"Connection"), "close".to_string()));

        let head = make_response_head("HTTP/1.1", 200, "OK", headers);
        assert!(!head.is_keep_alive());
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
fn prop_is_keep_alive_multiple_headers_close_in_first() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        // 最初に close、後に keep-alive がある場合も false (close 優先)
        let keep_alive_count = noprop::sample_usize_in(ctx, 1..4);
        let mut headers: Vec<(HeaderName, String)> =
            vec![(HeaderName::from_static(b"Connection"), "close".to_string())];
        for _ in 0..keep_alive_count {
            headers.push((
                HeaderName::from_static(b"Connection"),
                "keep-alive".to_string(),
            ));
        }

        let head = make_response_head("HTTP/1.1", 200, "OK", headers);
        assert!(!head.is_keep_alive());
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
fn prop_is_keep_alive_multiple_headers_mixed_tokens() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        // 複数のヘッダーに分散した keep-alive と close
        // close がどの位置にあっても false
        let version = http_version(ctx);
        let close_position = noprop::sample_usize_in(ctx, 0..3);
        let mut headers: Vec<(HeaderName, String)> = vec![
            (
                HeaderName::from_static(b"Connection"),
                "keep-alive".to_string(),
            ),
            (
                HeaderName::from_static(b"Connection"),
                "keep-alive".to_string(),
            ),
            (
                HeaderName::from_static(b"Connection"),
                "keep-alive".to_string(),
            ),
        ];
        headers[close_position] = (HeaderName::from_static(b"Connection"), "close".to_string());

        let head = make_response_head(&version, 200, "OK", headers);
        assert!(!head.is_keep_alive());
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
fn prop_is_keep_alive_multiple_headers_no_connection_token() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);
    let http11_seen = std::cell::Cell::new(0usize);
    let http10_seen = std::cell::Cell::new(0usize);

    runner.run(256, |ctx| {
        // Connection ヘッダーはあるが keep-alive も close もない場合
        // デフォルト動作（HTTP/1.1 は true、HTTP/1.0 は false）
        let version = http_version(ctx);
        let other_token = lower_string(ctx, 1..=8);
        let headers = vec![(HeaderName::from_static(b"Connection"), other_token)];

        let head = make_response_head(&version, 200, "OK", headers);

        if version == "HTTP/1.1" {
            // HTTP/1.1 側の分岐が一度も実行されないと true 側の検証が空振りする
            http11_seen.set(http11_seen.get() + 1);
            assert!(head.is_keep_alive());
        } else {
            http10_seen.set(http10_seen.get() + 1);
            assert!(!head.is_keep_alive());
        }
        Ok(())
    })?;

    // p 推定値: version は http_version で HTTP/1.0 と HTTP/1.1 を確率 1/2 ずつ。
    // 256 ケースでの miss 確率は (1/2)^256 ≈ 0。
    assert!(
        http11_seen.get() > 0,
        "HTTP/1.1 側が一度も検証されなかった\n{runner}"
    );
    assert!(
        http10_seen.get() > 0,
        "HTTP/1.0 側が一度も検証されなかった\n{runner}"
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
// HttpHead トレイトメソッドの PBT
// ========================================

/// 複数 TE ヘッダーの結合処理
#[test]
fn prop_is_chunked_multiple_te_headers() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        // 複数の Transfer-Encoding: chunked ヘッダー → 最後のトークンが chunked → true
        let count = noprop::sample_usize_in(ctx, 2..5);
        let headers: Vec<(HeaderName, String)> = (0..count)
            .map(|_| {
                (
                    HeaderName::from_static(b"Transfer-Encoding"),
                    "chunked".to_string(),
                )
            })
            .collect();
        let head = make_response_head("HTTP/1.1", 200, "OK", headers);
        assert!(head.is_chunked());
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

/// 不正な Content-Length → content_length() は Err を返す
/// (旧実装は `None` を黙って返していたが、smuggling 検知のため Err を返すよう変更された)
#[test]
fn prop_content_length_invalid_returns_err() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let invalid_value = alpha_string(ctx, 1..=8);
        let head = make_response_head(
            "HTTP/1.1",
            200,
            "OK",
            vec![(HeaderName::from_static(b"Content-Length"), invalid_value)],
        );
        assert!(head.content_length().is_err());
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

/// decoder の BodyKind::ContentLength(n) と head.content_length() の整合性
#[test]
fn prop_body_kind_content_length_matches_head() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let len = noprop::sample_u64_in(ctx, 0..1_000_000);
        let data = format!(
            "POST / HTTP/1.1\r\nHost: example.com\r\nContent-Length: {}\r\n\r\n",
            len
        );
        let mut decoder = RequestDecoder::new();
        decoder
            .feed(data.as_bytes())
            .expect("ヘッダーのデコードは成功するはず (実装バグ)");
        let (head, body_kind) = decoder
            .decode_headers()
            .expect("結果は存在するはず (実装バグ)")
            .expect("ヘッダーのデコードは成功するはず (実装バグ)");
        assert!(matches!(body_kind, BodyKind::ContentLength(n) if n == len));
        assert_eq!(
            head.content_length()
                .expect("ヘッダーのデコードは成功するはず (実装バグ)"),
            Some(len)
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

/// CL 不在のリクエストでは head.content_length() は Ok(None)
#[test]
fn prop_no_content_length_header_returns_ok_none() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let method = match noprop::sample_usize_in(ctx, 0..3) {
            0 => "GET",
            1 => "HEAD",
            _ => "DELETE",
        };
        let data = format!("{} / HTTP/1.1\r\nHost: example.com\r\n\r\n", method);
        let mut decoder = RequestDecoder::new();
        decoder
            .feed(data.as_bytes())
            .expect("ヘッダーのデコードは成功するはず (実装バグ)");
        let (head, _) = decoder
            .decode_headers()
            .expect("結果は存在するはず (実装バグ)")
            .expect("ヘッダーのデコードは成功するはず (実装バグ)");
        assert_eq!(
            head.content_length()
                .expect("ヘッダーのデコードは成功するはず (実装バグ)"),
            None
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
// ResponseHead ヘルパーメソッドの PBT
// ========================================

/// status_class: 3xx ステータスコードで Redirection
#[test]
fn prop_response_head_status_class_redirection() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let status = noprop::sample_usize_in(ctx, 300..=399) as u16;
        let data = format!("HTTP/1.1 {} Redirect\r\n\r\n", status);
        let mut decoder = ResponseDecoder::new();
        decoder
            .feed(data.as_bytes())
            .expect("ヘッダーのデコードは成功するはず (実装バグ)");
        let (head, _) = decoder
            .decode_headers()
            .expect("結果は存在するはず (実装バグ)")
            .expect("ヘッダーのデコードは成功するはず (実装バグ)");
        assert_eq!(head.status_class(), StatusClass::Redirection);
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

/// status_class: 4xx ステータスコードで ClientError
#[test]
fn prop_response_head_status_class_client_error() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let status = noprop::sample_usize_in(ctx, 400..=499) as u16;
        let data = format!("HTTP/1.1 {} Error\r\n\r\n", status);
        let mut decoder = ResponseDecoder::new();
        decoder
            .feed(data.as_bytes())
            .expect("ヘッダーのデコードは成功するはず (実装バグ)");
        let (head, _) = decoder
            .decode_headers()
            .expect("結果は存在するはず (実装バグ)")
            .expect("ヘッダーのデコードは成功するはず (実装バグ)");
        assert_eq!(head.status_class(), StatusClass::ClientError);
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

/// status_class: 5xx ステータスコードで ServerError
#[test]
fn prop_response_head_status_class_server_error() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let status = noprop::sample_usize_in(ctx, 500..=599) as u16;
        let data = format!("HTTP/1.1 {} Error\r\n\r\n", status);
        let mut decoder = ResponseDecoder::new();
        decoder
            .feed(data.as_bytes())
            .expect("ヘッダーのデコードは成功するはず (実装バグ)");
        let (head, _) = decoder
            .decode_headers()
            .expect("結果は存在するはず (実装バグ)")
            .expect("ヘッダーのデコードは成功するはず (実装バグ)");
        assert_eq!(head.status_class(), StatusClass::ServerError);
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

/// status_class: 任意の status_code (100..=599) が
/// StatusClass::from_status_code と整合する
#[test]
fn prop_response_head_status_class_consistency() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let status = noprop::sample_usize_in(ctx, 100..=599) as u16;
        let data = format!("HTTP/1.1 {} Status\r\n\r\n", status);
        let mut decoder = ResponseDecoder::new();
        decoder
            .feed(data.as_bytes())
            .expect("ヘッダーのデコードは成功するはず (実装バグ)");
        let (head, _) = decoder
            .decode_headers()
            .expect("結果は存在するはず (実装バグ)")
            .expect("ヘッダーのデコードは成功するはず (実装バグ)");
        let expected = StatusClass::from_status_code(status)
            .expect("100..=599 は必ず分類できるはず (実装バグ)");
        assert_eq!(head.status_class(), expected);
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

/// has_header / connection メソッドの検証
#[test]
fn prop_response_head_has_header_and_connection() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let conn_value = match noprop::sample_usize_in(ctx, 0..2) {
            0 => "keep-alive",
            _ => "close",
        };
        let data = format!(
            "HTTP/1.1 200 OK\r\nConnection: {}\r\nX-Custom: test\r\n\r\n",
            conn_value
        );
        let mut decoder = ResponseDecoder::new();
        decoder
            .feed(data.as_bytes())
            .expect("ヘッダーのデコードは成功するはず (実装バグ)");
        let (head, _) = decoder
            .decode_headers()
            .expect("結果は存在するはず (実装バグ)")
            .expect("ヘッダーのデコードは成功するはず (実装バグ)");
        assert!(head.has_header("Connection"));
        assert!(head.has_header("X-Custom"));
        assert!(!head.has_header("X-Missing"));
        assert_eq!(
            head.connection()
                .expect("ヘッダーのデコードは成功するはず (実装バグ)"),
            conn_value
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

/// RequestHead の version() メソッド
#[test]
fn prop_request_head_version() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let version = http_version(ctx);
        let data = format!("GET / {}\r\nHost: localhost\r\n\r\n", version);
        let mut decoder = RequestDecoder::new();
        decoder
            .feed(data.as_bytes())
            .expect("ヘッダーのデコードは成功するはず (実装バグ)");
        let (head, _) = decoder
            .decode_headers()
            .expect("結果は存在するはず (実装バグ)")
            .expect("ヘッダーのデコードは成功するはず (実装バグ)");
        assert_eq!(head.version(), version);
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
// into_parts() ラウンドトリップ PBT
// ========================================

/// RequestHead → into_parts() → Request::with_version() → encode_headers() → decode → 一致
#[test]
fn prop_request_head_into_parts_roundtrip() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let method = http_method(ctx);
        let uri = http_uri(ctx);
        let version = http_version(ctx);
        let extra_header_count = noprop::sample_usize_in(ctx, 0..3);
        let mut request_line = format!(
            "{} {} {}\r\nHost: localhost\r\n",
            method.as_str(),
            uri,
            version
        );
        for _ in 0..extra_header_count {
            request_line.push_str("X-Custom: value\r\n");
        }
        request_line.push_str("\r\n");

        let mut decoder = RequestDecoder::new();
        decoder
            .feed(request_line.as_bytes())
            .expect("ヘッダーのデコードは成功するはず (実装バグ)");
        let (head, _) = decoder
            .decode_headers()
            .expect("結果は存在するはず (実装バグ)")
            .expect("ヘッダーのデコードは成功するはず (実装バグ)");

        let orig_method = head.method().to_string();
        let orig_uri = head.uri().to_string();
        let orig_version = head.version().to_string();
        let orig_headers: Vec<_> = head.headers().to_vec();

        let (method, uri, version, headers) = head.into_parts();
        let mut request = Request::with_version(method, uri, version)?;
        for (name, value) in headers {
            request.add_header(name, value)?;
        }

        let encoded = request.encode_headers()?;
        let mut decoder2 = RequestDecoder::new();
        decoder2
            .feed(&encoded)
            .expect("ヘッダーのデコードは成功するはず (実装バグ)");
        let (head2, _) = decoder2
            .decode_headers()
            .expect("結果は存在するはず (実装バグ)")
            .expect("ヘッダーのデコードは成功するはず (実装バグ)");

        assert_eq!(head2.method(), orig_method.as_str());
        assert_eq!(head2.uri(), orig_uri.as_str());
        assert_eq!(head2.version(), orig_version.as_str());
        assert_eq!(head2.headers().len(), orig_headers.len());
        for ((n1, v1), (n2, v2)) in orig_headers.iter().zip(head2.headers().iter()) {
            assert_eq!(n1, n2);
            assert_eq!(v1, v2);
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

/// ResponseHead → into_parts() → Response::with_version() → encode_headers() → decode → 一致
#[test]
fn prop_response_head_into_parts_roundtrip() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let status = status_code(ctx);
        let reason = reason_phrase(ctx);
        let version = http_version(ctx);
        let extra_header_count = noprop::sample_usize_in(ctx, 0..3);
        let mut status_line = format!("{} {} {}\r\n", version, status, reason);
        for _ in 0..extra_header_count {
            status_line.push_str("X-Custom: value\r\n");
        }
        status_line.push_str("\r\n");

        let mut decoder = ResponseDecoder::new();
        decoder
            .feed(status_line.as_bytes())
            .expect("ヘッダーのデコードは成功するはず (実装バグ)");
        let (head, _) = decoder
            .decode_headers()
            .expect("結果は存在するはず (実装バグ)")
            .expect("ヘッダーのデコードは成功するはず (実装バグ)");

        let orig_version = head.version().to_string();
        let orig_status_code = head.status_code();
        let orig_reason = head.reason_phrase().to_string();
        let orig_headers: Vec<_> = head.headers().to_vec();

        let (version, status_code, reason_phrase, headers) = head.into_parts();
        let mut response = Response::with_version(version, status_code, reason_phrase)?;
        for (name, value) in headers {
            response.add_header(name, value)?;
        }

        let encoded = response.encode_headers()?;
        let mut decoder2 = ResponseDecoder::new();
        decoder2
            .feed(&encoded)
            .expect("ヘッダーのデコードは成功するはず (実装バグ)");
        let (head2, _) = decoder2
            .decode_headers()
            .expect("結果は存在するはず (実装バグ)")
            .expect("ヘッダーのデコードは成功するはず (実装バグ)");

        assert_eq!(head2.version(), orig_version.as_str());
        assert_eq!(head2.status_code(), orig_status_code);
        assert_eq!(head2.reason_phrase(), orig_reason.as_str());
        assert_eq!(head2.headers().len(), orig_headers.len());
        for ((n1, v1), (n2, v2)) in orig_headers.iter().zip(head2.headers().iter()) {
            assert_eq!(n1, n2);
            assert_eq!(v1, v2);
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
// 個別 into_xxx() メソッドの PBT
// ========================================

/// RequestHead の個別 into_xxx() メソッドが各フィールドを正しく返す
#[test]
fn prop_request_head_into_method() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let method = http_method(ctx);
        let uri = http_uri(ctx);
        let version = http_version(ctx);
        let data = format!(
            "{} {} {}\r\nHost: localhost\r\n\r\n",
            method.as_str(),
            uri,
            version
        );
        let mut decoder = RequestDecoder::new();
        decoder
            .feed(data.as_bytes())
            .expect("ヘッダーのデコードは成功するはず (実装バグ)");
        let (head, _) = decoder
            .decode_headers()
            .expect("結果は存在するはず (実装バグ)")
            .expect("ヘッダーのデコードは成功するはず (実装バグ)");

        let orig_method = head.method().to_string();
        let m = head.into_method();
        assert_eq!(m.as_str(), orig_method.as_str());
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

/// RequestHead の into_uri() が URI を正しく返す
#[test]
fn prop_request_head_into_uri() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let method = http_method(ctx);
        let uri = http_uri(ctx);
        let version = http_version(ctx);
        let data = format!(
            "{} {} {}\r\nHost: localhost\r\n\r\n",
            method.as_str(),
            uri,
            version
        );
        let mut decoder = RequestDecoder::new();
        decoder
            .feed(data.as_bytes())
            .expect("ヘッダーのデコードは成功するはず (実装バグ)");
        let (head, _) = decoder
            .decode_headers()
            .expect("結果は存在するはず (実装バグ)")
            .expect("ヘッダーのデコードは成功するはず (実装バグ)");

        let orig_uri = head.uri().to_string();
        let u = head.into_uri();
        assert_eq!(u, orig_uri);
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

/// RequestHead の into_version() がバージョンを正しく返す
#[test]
fn prop_request_head_into_version() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let method = http_method(ctx);
        let uri = http_uri(ctx);
        let version = http_version(ctx);
        let data = format!(
            "{} {} {}\r\nHost: localhost\r\n\r\n",
            method.as_str(),
            uri,
            version
        );
        let mut decoder = RequestDecoder::new();
        decoder
            .feed(data.as_bytes())
            .expect("ヘッダーのデコードは成功するはず (実装バグ)");
        let (head, _) = decoder
            .decode_headers()
            .expect("結果は存在するはず (実装バグ)")
            .expect("ヘッダーのデコードは成功するはず (実装バグ)");

        let orig_version = head.version().to_string();
        let v = head.into_version();
        assert_eq!(v, orig_version);
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

/// RequestHead の into_headers() がヘッダーを正しく返す
#[test]
fn prop_request_head_into_headers() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let method = http_method(ctx);
        let uri = http_uri(ctx);
        let version = http_version(ctx);
        let data = format!(
            "{} {} {}\r\nHost: localhost\r\n\r\n",
            method.as_str(),
            uri,
            version
        );
        let mut decoder = RequestDecoder::new();
        decoder
            .feed(data.as_bytes())
            .expect("ヘッダーのデコードは成功するはず (実装バグ)");
        let (head, _) = decoder
            .decode_headers()
            .expect("結果は存在するはず (実装バグ)")
            .expect("ヘッダーのデコードは成功するはず (実装バグ)");

        let orig_headers: Vec<_> = head.headers().to_vec();
        let h = head.into_headers();
        assert_eq!(h.len(), orig_headers.len());
        for ((n1, v1), (n2, v2)) in orig_headers.iter().zip(h.iter()) {
            assert_eq!(n1, n2);
            assert_eq!(v1, v2);
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

/// ResponseHead の個別 into_xxx() メソッドが各フィールドを正しく返す
#[test]
fn prop_response_head_into_version() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let status = status_code(ctx);
        let reason = reason_phrase(ctx);
        let version = http_version(ctx);
        let data = format!("{} {} {}\r\n\r\n", version, status, reason);
        let mut decoder = ResponseDecoder::new();
        decoder
            .feed(data.as_bytes())
            .expect("ヘッダーのデコードは成功するはず (実装バグ)");
        let (head, _) = decoder
            .decode_headers()
            .expect("結果は存在するはず (実装バグ)")
            .expect("ヘッダーのデコードは成功するはず (実装バグ)");

        let orig_version = head.version().to_string();
        let v = head.into_version();
        assert_eq!(v, orig_version);
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

/// ResponseHead の into_reason_phrase() が reason_phrase を正しく返す
#[test]
fn prop_response_head_into_reason_phrase() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let status = status_code(ctx);
        let reason = reason_phrase(ctx);
        let version = http_version(ctx);
        let data = format!("{} {} {}\r\n\r\n", version, status, reason);
        let mut decoder = ResponseDecoder::new();
        decoder
            .feed(data.as_bytes())
            .expect("ヘッダーのデコードは成功するはず (実装バグ)");
        let (head, _) = decoder
            .decode_headers()
            .expect("結果は存在するはず (実装バグ)")
            .expect("ヘッダーのデコードは成功するはず (実装バグ)");

        let orig_reason = head.reason_phrase().to_string();
        let r = head.into_reason_phrase();
        assert_eq!(r, orig_reason);
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

/// ResponseHead の into_headers() がヘッダーを正しく返す
#[test]
fn prop_response_head_into_headers() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let status = status_code(ctx);
        let reason = reason_phrase(ctx);
        let version = http_version(ctx);
        let data = format!("{} {} {}\r\n\r\n", version, status, reason);
        let mut decoder = ResponseDecoder::new();
        decoder
            .feed(data.as_bytes())
            .expect("ヘッダーのデコードは成功するはず (実装バグ)");
        let (head, _) = decoder
            .decode_headers()
            .expect("結果は存在するはず (実装バグ)")
            .expect("ヘッダーのデコードは成功するはず (実装バグ)");

        let orig_headers: Vec<_> = head.headers().to_vec();
        let h = head.into_headers();
        assert_eq!(h.len(), orig_headers.len());
        for ((n1, v1), (n2, v2)) in orig_headers.iter().zip(h.iter()) {
            assert_eq!(n1, n2);
            assert_eq!(v1, v2);
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
