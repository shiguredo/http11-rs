//! エンコーダーのプロパティテスト

use shiguredo_http11::{
    EncodeError, HeaderName, Method, Request, RequestEncoder, Response, ResponseEncoder,
    StatusCode, encode_chunk, encode_chunks, encode_request, encode_request_headers,
    encode_response, encode_response_headers,
};
use std::cell::Cell;

// ========================================
// Strategy 定義
// ========================================

// ALPHA (A-Z / a-z) の 1 文字
fn alpha_char(ctx: &mut noprop::TestCaseContext) -> char {
    let index = noprop::sample_usize_in(ctx, 0..52);
    if index < 26 {
        char::from(b'A' + index as u8)
    } else {
        char::from(b'a' + (index - 26) as u8)
    }
}

// 小文字 [a-z] の 1 文字
fn lower_char(ctx: &mut noprop::TestCaseContext) -> char {
    char::from(b'a' + noprop::sample_usize_in(ctx, 0..26) as u8)
}

// 英数字 [a-zA-Z0-9] の 1 バイト
fn alnum_byte(ctx: &mut noprop::TestCaseContext) -> u8 {
    match noprop::sample_usize_in(ctx, 0..3) {
        0 => b'a' + noprop::sample_usize_in(ctx, 0..26) as u8,
        1 => b'A' + noprop::sample_usize_in(ctx, 0..26) as u8,
        _ => b'0' + noprop::sample_usize_in(ctx, 0..10) as u8,
    }
}

// HTTP メソッド
fn http_method(ctx: &mut noprop::TestCaseContext) -> Method {
    match noprop::sample_usize_in(ctx, 0..8) {
        0 => Method::GET,
        1 => Method::POST,
        2 => Method::PUT,
        3 => Method::DELETE,
        4 => Method::HEAD,
        5 => Method::OPTIONS,
        6 => Method::PATCH,
        _ => Method::QUERY,
    }
}

// URI
fn uri(ctx: &mut noprop::TestCaseContext) -> String {
    match noprop::sample_usize_in(ctx, 0..4) {
        0 => "/".to_string(),
        1 => {
            // "/[a-z]{1,8}"
            let len = noprop::sample_usize_in(ctx, 1..=8);
            let mut s = String::with_capacity(len + 1);
            s.push('/');
            for _ in 0..len {
                s.push(lower_char(ctx));
            }
            s
        }
        2 => {
            // "/[a-z]{1,4}/[a-z]{1,4}"
            let first_len = noprop::sample_usize_in(ctx, 1..=4);
            let second_len = noprop::sample_usize_in(ctx, 1..=4);
            let mut s = String::with_capacity(first_len + second_len + 2);
            s.push('/');
            for _ in 0..first_len {
                s.push(lower_char(ctx));
            }
            s.push('/');
            for _ in 0..second_len {
                s.push(lower_char(ctx));
            }
            s
        }
        _ => {
            // "/[a-z]{1,4}\?[a-z]{1,4}=[a-z]{1,4}"
            let query_name_len = noprop::sample_usize_in(ctx, 1..=4);
            let query_value_len = noprop::sample_usize_in(ctx, 1..=4);
            let mut s = String::with_capacity(query_name_len + query_value_len + 3);
            s.push('/');
            for _ in 0..query_name_len {
                s.push(lower_char(ctx));
            }
            s.push('?');
            for _ in 0..query_value_len {
                s.push(lower_char(ctx));
            }
            s.push('=');
            // 末尾のパラメータ値は無条件に 1 文字以上生成する
            let tail_len = noprop::sample_usize_in(ctx, 1..=4);
            for _ in 0..tail_len {
                s.push(lower_char(ctx));
            }
            s
        }
    }
}

// ヘッダー名
fn header_name(ctx: &mut noprop::TestCaseContext) -> HeaderName {
    match noprop::sample_usize_in(ctx, 0..5) {
        0 => HeaderName::from_static(b"Content-Type"),
        1 => HeaderName::from_static(b"Accept"),
        2 => HeaderName::from_static(b"User-Agent"),
        3 => HeaderName::from_static(b"Cache-Control"),
        _ => {
            // "[A-Za-z]{1,8}(-[A-Za-z]{1,8})?" 相当
            let first_len = noprop::sample_usize_in(ctx, 1..=8);
            let mut s = String::with_capacity(first_len + 9);
            for _ in 0..first_len {
                s.push(alpha_char(ctx));
            }
            // オプションのハイフン区切りサブトークン
            if noprop::sample_bool(ctx) {
                s.push('-');
                let second_len = noprop::sample_usize_in(ctx, 1..=8);
                for _ in 0..second_len {
                    s.push(alpha_char(ctx));
                }
            }
            HeaderName::new(s.as_bytes()).expect("valid token")
        }
    }
}

// ヘッダー値
fn header_value(ctx: &mut noprop::TestCaseContext) -> String {
    // "[a-zA-Z0-9 /-]{1,32}"
    let len = noprop::sample_usize_in(ctx, 1..=32);
    let mut s = String::with_capacity(len);
    for _ in 0..len {
        match noprop::sample_usize_in(ctx, 0..6) {
            0 => s.push(lower_char(ctx)),
            1 => s.push(char::from(b'A' + noprop::sample_usize_in(ctx, 0..26) as u8)),
            2 => s.push(char::from(b'0' + noprop::sample_usize_in(ctx, 0..10) as u8)),
            3 => s.push(' '),
            4 => s.push('/'),
            _ => s.push('-'),
        }
    }
    s
}

// ステータスコード
fn status_code(ctx: &mut noprop::TestCaseContext) -> u16 {
    match noprop::sample_usize_in(ctx, 0..15) {
        // 1xx
        0 => 100,
        1 => 101,
        // 2xx
        2 => 200,
        3 => 201,
        4 => 204,
        // 3xx
        5 => 301,
        6 => 302,
        7 => 304,
        // 4xx
        8 => 400,
        9 => 401,
        10 => 403,
        11 => 404,
        // 5xx
        12 => 500,
        13 => 502,
        _ => 503,
    }
}

// Reason phrase
fn reason_phrase(ctx: &mut noprop::TestCaseContext) -> &'static str {
    match noprop::sample_usize_in(ctx, 0..6) {
        0 => "OK",
        1 => "Created",
        2 => "No Content",
        3 => "Not Found",
        4 => "Internal Server Error",
        _ => "Bad Gateway",
    }
}

// ボディ
fn body(ctx: &mut noprop::TestCaseContext) -> Vec<u8> {
    match noprop::sample_usize_in(ctx, 0..3) {
        0 => Vec::new(),
        1 => {
            // "[a-zA-Z0-9]{1,32}" 相当
            let len = noprop::sample_usize_in(ctx, 1..=32);
            let mut v = Vec::with_capacity(len);
            for _ in 0..len {
                v.push(alnum_byte(ctx));
            }
            v
        }
        _ => {
            // 任意バイト列 (0..64 個)
            let len = noprop::sample_usize_in(ctx, 0..64);
            noprop::sample_bytes_vec(ctx, len)
        }
    }
}

// ========================================
// encode_request のテスト
// ========================================

/// リクエストの基本エンコード: リクエストラインで始まり、ヘッダー終端を持つ
#[test]
fn prop_encode_request_basic() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let method = http_method(ctx);
        let uri = uri(ctx);
        let req = Request::new(method.clone(), &uri)
            .expect("エンコードは成功するはず (実装バグ)")
            .header(HeaderName::from_static(b"Host"), "example.com")
            .expect("エンコードは成功するはず (実装バグ)");
        let encoded = encode_request(&req).expect("エンコードは成功するはず (実装バグ)");

        let request_line = format!("{} {} HTTP/1.1\r\n", method, uri);
        let encoded_str = String::from_utf8_lossy(&encoded);
        assert!(encoded_str.starts_with(&request_line));
        assert!(encoded_str.contains("\r\n\r\n"));
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

/// リクエストのヘッダーが正しくエンコードされる
#[test]
fn prop_encode_request_with_headers() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let method = http_method(ctx);
        let uri = uri(ctx);
        let header_name = header_name(ctx);
        let header_value = header_value(ctx);
        let req = Request::new(method, &uri)
            .expect("エンコードは成功するはず (実装バグ)")
            .header(HeaderName::from_static(b"Host"), "example.com")
            .expect("エンコードは成功するはず (実装バグ)")
            .header(header_name.clone(), &header_value)
            .expect("エンコードは成功するはず (実装バグ)");
        let encoded = encode_request(&req).expect("エンコードは成功するはず (実装バグ)");
        let encoded_str = String::from_utf8_lossy(&encoded);

        let header_line = format!("{}: {}\r\n", header_name, header_value);
        assert!(encoded_str.contains(&header_line));
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

/// リクエストボディが Content-Length ヘッダーと実体として正しくエンコードされる
#[test]
fn prop_encode_request_with_body() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let body_asserted = Cell::new(0usize);
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let method = http_method(ctx);
        let uri = uri(ctx);
        let data = body(ctx);
        let req = Request::new(method, &uri)
            .expect("エンコードは成功するはず (実装バグ)")
            .header(HeaderName::from_static(b"Host"), "example.com")
            .expect("エンコードは成功するはず (実装バグ)")
            .body(data.clone());
        let encoded = encode_request(&req).expect("エンコードは成功するはず (実装バグ)");

        if !data.is_empty() {
            let encoded_str = String::from_utf8_lossy(&encoded);
            let cl_header = format!("Content-Length: {}\r\n", data.len());
            assert!(encoded_str.contains(&cl_header));
            assert!(encoded.ends_with(&data));

            // ボディ非空のケースで Content-Length と実体の検証を行ったことを記録する
            body_asserted.set(body_asserted.get() + 1);
        }
        Ok(())
    })?;

    // ゲート: ボディ非空 (p ≈ 0.99) のケースが 1 度も生成されないと Content-Length 検証が
    // 空振りになるため、到達を保証する。256 ケースでの miss 確率は 0.01^256 ≈ 0。
    assert!(
        body_asserted.get() > 0,
        "ボディ非空の Content-Length 検証が一度も実行されていない\n{runner}"
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
// encode_response のテスト
// ========================================

/// レスポンスの基本エンコード: ステータスラインで始まり、ヘッダー終端を持つ
#[test]
fn prop_encode_response_basic() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let status = status_code(ctx);
        let phrase = reason_phrase(ctx);
        let res = Response::new(status, phrase).expect("エンコードは成功するはず (実装バグ)");
        let encoded = encode_response(&res).expect("エンコードは成功するはず (実装バグ)");

        let status_line = format!("HTTP/1.1 {} {}\r\n", status, phrase);
        let encoded_str = String::from_utf8_lossy(&encoded);
        assert!(encoded_str.starts_with(&status_line));
        assert!(encoded_str.contains("\r\n\r\n"));
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

/// レスポンスのヘッダーが正しくエンコードされる
#[test]
fn prop_encode_response_with_headers() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let status = status_code(ctx);
        let phrase = reason_phrase(ctx);
        let header_name = header_name(ctx);
        let header_value = header_value(ctx);
        let header_line = format!("{}: {}\r\n", header_name, header_value);
        let res = Response::new(status, phrase)
            .expect("エンコードは成功するはず (実装バグ)")
            .header(header_name, header_value.as_str())
            .expect("エンコードは成功するはず (実装バグ)");
        let encoded = encode_response(&res).expect("エンコードは成功するはず (実装バグ)");
        let encoded_str = String::from_utf8_lossy(&encoded);

        assert!(encoded_str.contains(&header_line));
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

/// レスポンスボディが Content-Length ヘッダーと実体として正しくエンコードされる
/// (1xx / 204 / 304 はボディを送信しないため除外)
#[test]
fn prop_encode_response_with_body() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let body_asserted = Cell::new(0usize);
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let status = status_code(ctx);
        let phrase = reason_phrase(ctx);
        let data = body(ctx);
        let res = Response::new(status, phrase)
            .expect("エンコードは成功するはず (実装バグ)")
            .body(data.clone());
        let encoded = encode_response(&res).expect("エンコードは成功するはず (実装バグ)");

        let status_has_body = !((100..200).contains(&status) || status == 204 || status == 304);

        if !data.is_empty() && status_has_body {
            let encoded_str = String::from_utf8_lossy(&encoded);
            let cl_header = format!("Content-Length: {}\r\n", data.len());
            assert!(encoded_str.contains(&cl_header));
            assert!(encoded.ends_with(&data));

            // ボディ送信が期待されるケースで検証を行ったことを記録する
            body_asserted.set(body_asserted.get() + 1);
        }
        Ok(())
    })?;

    // ゲート: データ非空 (p ≈ 0.99) かつボディ送信可能ステータス (p = 11/15 ≈ 0.73) の
    // ケースが 1 度も生成されないと Content-Length 検証が空振りになるため、到達を保証する。
    // 全体の p ≈ 0.73、256 ケースでの miss 確率は 0.27^256 ≈ 0。
    assert!(
        body_asserted.get() > 0,
        "ボディ送信可能ステータスでの Content-Length 検証が一度も実行されていない\n{runner}"
    );

    // ジェネレータは valid-by-construction であり、ケース棄却が発生しないことの検証
    assert_eq!(
        runner.stats().rejected_cases,
        0,
        "ジェネレータが valid-by-construction であること\n{runner}"
    );
    Ok(())
}

/// omit_body(true) と手動 Content-Length 設定の組み合わせで
/// ボディなしレスポンスに Content-Length ヘッダーが残ること
#[test]
fn prop_encode_response_omit_body_with_content_length() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        // 元の戦略: (200u16..204).prop_union(206..300)
        let status = match noprop::sample_usize_in(ctx, 0..2) {
            0 => noprop::sample_usize_in(ctx, 200..204) as u16,
            _ => noprop::sample_usize_in(ctx, 206..300) as u16,
        };
        let content_length = noprop::sample_usize_in(ctx, 1..10000);
        let res = Response::new(status, "OK")
            .expect("エンコードは成功するはず (実装バグ)")
            .header(
                HeaderName::from_static(b"Content-Length"),
                content_length.to_string(),
            )
            .expect("エンコードは成功するはず (実装バグ)")
            .omit_body(true);
        let encoded = encode_response(&res).expect("エンコードは成功するはず (実装バグ)");
        let encoded_str = String::from_utf8_lossy(&encoded);

        let cl_header = format!("Content-Length: {}\r\n", content_length);
        assert!(encoded_str.contains(&cl_header));
        assert!(encoded_str.ends_with("\r\n\r\n"));
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

/// omit_body(true) とボディなしの組み合わせでは Content-Length ヘッダーが付かないこと
#[test]
fn prop_encode_response_omit_body_empty_no_header() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let status = noprop::sample_usize_in(ctx, 200..204) as u16;
        let res = Response::new(status, "OK")
            .expect("エンコードは成功するはず (実装バグ)")
            .omit_body(true);
        let encoded = encode_response(&res).expect("エンコードは成功するはず (実装バグ)");
        let encoded_str = String::from_utf8_lossy(&encoded);

        assert!(!encoded_str.contains("Content-Length"));
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
// encode_chunk のテスト
// ========================================

/// 非空ボディのチャンクエンコード: サイズ行で始まり、データが正しく埋め込まれる
#[test]
fn prop_encode_chunk_non_empty() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let len = noprop::sample_usize_in(ctx, 1..64);
        let data = noprop::sample_bytes_vec(ctx, len);
        let encoded = encode_chunk(&data);
        let encoded_str = String::from_utf8_lossy(&encoded);

        let size_line = format!("{:x}\r\n", data.len());
        assert!(encoded_str.starts_with(&size_line));
        assert!(encoded.ends_with(b"\r\n"));
        let data_start = size_line.len();
        let data_end = encoded.len() - 2;
        assert_eq!(&encoded[data_start..data_end], &data[..]);
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
// encode_chunks のテスト
// ========================================

/// 複数チャンクのエンコード: ターミネータを持ち、チャンク数が正しく並ぶ
#[test]
fn prop_encode_chunks_basic() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let count = noprop::sample_usize_in(ctx, 1..=5);
        let chunks: Vec<&[u8]> = (0..count).map(|_| b"test".as_ref()).collect();
        let encoded = encode_chunks(&chunks);

        assert!(encoded.ends_with(b"0\r\n\r\n"));

        let encoded_str = String::from_utf8_lossy(&encoded);
        assert_eq!(encoded_str.matches("4\r\n").count(), count);
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

// `write_hex_usize` ヘルパーは encoder.rs のプライベート関数のため、
// 公開 API `encode_chunk` / `encode_chunks` の出力が
// `alloc::format!("{:x}\r\n", n)` ベースの参照実装とバイト単位で完全一致することで
// ヘルパーの正しさを間接検証する

/// `encode_chunk` の出力が参照実装と完全一致すること
#[test]
fn prop_encode_chunk_equals_format_reference() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let len = noprop::sample_usize_in(ctx, 0..256);
        let data = noprop::sample_bytes_vec(ctx, len);
        let encoded = encode_chunk(&data);

        let mut expected: Vec<u8> = Vec::new();
        if data.is_empty() {
            expected.extend_from_slice(b"0\r\n\r\n");
        } else {
            expected.extend_from_slice(format!("{:x}\r\n", data.len()).as_bytes());
            expected.extend_from_slice(&data);
            expected.extend_from_slice(b"\r\n");
        }
        assert_eq!(encoded, expected);
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

/// `encode_chunks` の出力が参照実装と完全一致すること
#[test]
fn prop_encode_chunks_equals_format_reference() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let chunk_count = noprop::sample_usize_in(ctx, 0..6);
        let mut chunks = Vec::with_capacity(chunk_count);
        for _ in 0..chunk_count {
            let len = noprop::sample_usize_in(ctx, 0..32);
            chunks.push(noprop::sample_bytes_vec(ctx, len));
        }
        let chunk_refs: Vec<&[u8]> = chunks.iter().map(|c| c.as_slice()).collect();
        let encoded = encode_chunks(&chunk_refs);

        let mut expected: Vec<u8> = Vec::new();
        for chunk in &chunks {
            expected.extend_from_slice(format!("{:x}\r\n", chunk.len()).as_bytes());
            expected.extend_from_slice(chunk);
            expected.extend_from_slice(b"\r\n");
        }
        expected.extend_from_slice(b"0\r\n\r\n");
        assert_eq!(encoded, expected);
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
// encode_request_headers のテスト
// ========================================

/// encode_request_headers がリクエストライン、Host、ヘッダー終端を正しく出力する
#[test]
fn prop_encode_request_headers_basic() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let method = http_method(ctx);
        let uri = uri(ctx);
        let req = Request::new(method.clone(), &uri)
            .expect("エンコードは成功するはず (実装バグ)")
            .header(HeaderName::from_static(b"Host"), "example.com")
            .expect("エンコードは成功するはず (実装バグ)");
        let encoded = encode_request_headers(&req).expect("エンコードは成功するはず (実装バグ)");
        let encoded_str = String::from_utf8_lossy(&encoded);

        let request_line = format!("{} {} HTTP/1.1\r\n", method, uri);
        assert!(encoded_str.starts_with(&request_line));
        assert!(encoded_str.contains("Host: example.com\r\n"));
        assert!(encoded_str.ends_with("\r\n\r\n"));
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
// encode_response_headers のテスト
// ========================================

/// encode_response_headers がステータスライン、Content-Type、ヘッダー終端を正しく出力する
#[test]
fn prop_encode_response_headers_basic() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let status = status_code(ctx);
        let phrase = reason_phrase(ctx);
        let res = Response::new(status, phrase)
            .expect("エンコードは成功するはず (実装バグ)")
            .header(HeaderName::from_static(b"Content-Type"), "text/html")
            .expect("エンコードは成功するはず (実装バグ)");
        let encoded = encode_response_headers(&res).expect("エンコードは成功するはず (実装バグ)");
        let encoded_str = String::from_utf8_lossy(&encoded);

        let status_line = format!("HTTP/1.1 {} {}\r\n", status, phrase);
        assert!(encoded_str.starts_with(&status_line));
        assert!(encoded_str.contains("Content-Type: text/html\r\n"));
        assert!(encoded_str.ends_with("\r\n\r\n"));
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
// Host 必須チェックのテスト (RFC 9112 Section 3.2)
// ========================================

/// HTTP/1.1 リクエストで Host ヘッダーが無い場合は常に MissingHostHeader
#[test]
fn prop_encode_request_host_required_for_http11() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let method = http_method(ctx);
        let uri = uri(ctx);
        let req = Request::new(method, &uri).expect("エンコードは成功するはず (実装バグ)");
        let result = encode_request(&req);
        assert!(matches!(result, Err(EncodeError::MissingHostHeader)));
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

/// HTTP/1.0 リクエストでは Host ヘッダーが無くてもエンコードできる
#[test]
fn prop_encode_request_host_optional_for_http10() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let method = http_method(ctx);
        let uri = uri(ctx);
        let req = Request::with_version(method, &uri, "HTTP/1.0")
            .expect("エンコードは成功するはず (実装バグ)");
        let result = encode_request(&req);
        assert!(result.is_ok());
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
// RFC 9112 Section 6.2: Transfer-Encoding と Content-Length の同時送信禁止
// ========================================

/// リクエストで TE と CL が同時設定されたら常に Conflicting... エラー
#[test]
fn prop_encode_request_te_and_cl_always_error() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let method = http_method(ctx);
        let uri = uri(ctx);
        let cl = noprop::sample_usize_in(ctx, 1..10000);
        let req = Request::new(method, &uri)
            .expect("エンコードは成功するはず (実装バグ)")
            .header(HeaderName::from_static(b"Host"), "example.com")
            .expect("エンコードは成功するはず (実装バグ)")
            .header(HeaderName::from_static(b"Transfer-Encoding"), "chunked")
            .expect("エンコードは成功するはず (実装バグ)")
            .header(HeaderName::from_static(b"Content-Length"), cl.to_string())
            .expect("エンコードは成功するはず (実装バグ)");
        let result = encode_request(&req);
        assert!(matches!(
            result,
            Err(EncodeError::ConflictingTransferEncodingAndContentLength)
        ));
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

/// レスポンスで TE と CL が同時設定されたら常に Conflicting... エラー
#[test]
fn prop_encode_response_te_and_cl_always_error() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        // 元の戦略: (200u16..204).prop_union(205..600)
        let status = match noprop::sample_usize_in(ctx, 0..2) {
            0 => noprop::sample_usize_in(ctx, 200..204) as u16,
            _ => noprop::sample_usize_in(ctx, 205..600) as u16,
        };
        let cl = noprop::sample_usize_in(ctx, 1..10000);
        let res = Response::new(status, "OK")
            .expect("エンコードは成功するはず (実装バグ)")
            .header(HeaderName::from_static(b"Transfer-Encoding"), "chunked")
            .expect("エンコードは成功するはず (実装バグ)")
            .header(HeaderName::from_static(b"Content-Length"), cl.to_string())
            .expect("エンコードは成功するはず (実装バグ)");
        let result = encode_response(&res);
        assert!(matches!(
            result,
            Err(EncodeError::ConflictingTransferEncodingAndContentLength)
        ));
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
// RFC 9112 Section 6.1: 1xx / 204 レスポンスで Transfer-Encoding 禁止
// ========================================

/// 1xx / 204 レスポンス + Transfer-Encoding は常に ForbiddenTransferEncoding
#[test]
fn prop_encode_response_1xx_or_204_with_te_always_error() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        // 元の戦略: prop_oneof![100u16..200, Just(204u16)]
        let status = match noprop::sample_usize_in(ctx, 0..2) {
            0 => noprop::sample_usize_in(ctx, 100..200) as u16,
            _ => 204,
        };
        let res = Response::new(status, "Info")
            .expect("エンコードは成功するはず (実装バグ)")
            .header(HeaderName::from_static(b"Transfer-Encoding"), "chunked")
            .expect("エンコードは成功するはず (実装バグ)");
        let result = encode_response(&res);
        match result {
            Err(EncodeError::ForbiddenTransferEncoding { status_code }) => {
                assert_eq!(status_code, status);
            }
            other => {
                panic!("ForbiddenTransferEncoding を期待したが {:?} だった", other);
            }
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
// RFC 9110 Section 8.6: 1xx / 204 レスポンスで Content-Length 禁止
// ========================================

/// 1xx / 204 レスポンス + Content-Length は常に ForbiddenContentLength
#[test]
fn prop_encode_response_1xx_or_204_with_cl_always_error() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        // 元の戦略: prop_oneof![100u16..200, Just(204u16)]
        let status = match noprop::sample_usize_in(ctx, 0..2) {
            0 => noprop::sample_usize_in(ctx, 100..200) as u16,
            _ => 204,
        };
        let res = Response::new(status, "Info")
            .expect("エンコードは成功するはず (実装バグ)")
            .header(HeaderName::from_static(b"Content-Length"), "0")
            .expect("エンコードは成功するはず (実装バグ)");
        let result = encode_response(&res);
        match result {
            Err(EncodeError::ForbiddenContentLength { status_code }) => {
                assert_eq!(status_code, status);
            }
            other => {
                panic!("ForbiddenContentLength を期待したが {:?} だった", other);
            }
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
// 205 Reset Content テスト
// ========================================

/// 205 レスポンスにボディがあると常に ForbiddenBodyFor205
#[test]
fn prop_encode_response_205_with_body_always_error() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let len = noprop::sample_usize_in(ctx, 1..128);
        let data = noprop::sample_bytes_vec(ctx, len);
        let res = Response::with_status(StatusCode::RESET_CONTENT).body(data);
        let result = encode_response(&res);
        assert!(matches!(result, Err(EncodeError::ForbiddenBodyFor205)));
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

/// 205 レスポンスで Transfer-Encoding は常にエラー
#[test]
fn prop_encode_response_205_with_te_always_error() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        // 元の戦略: prop_oneof![Just("chunked"), Just("gzip"), Just("deflate")]
        let te_value = match noprop::sample_usize_in(ctx, 0..3) {
            0 => "chunked".to_string(),
            1 => "gzip".to_string(),
            _ => "deflate".to_string(),
        };
        let res = Response::with_status(StatusCode::RESET_CONTENT)
            .header(HeaderName::from_static(b"Transfer-Encoding"), &te_value)
            .expect("エンコードは成功するはず (実装バグ)");
        let result = encode_response(&res);
        match result {
            Err(EncodeError::ForbiddenTransferEncoding { status_code: 205 }) => {}
            other => {
                panic!(
                    "205 で ForbiddenTransferEncoding を期待したが {:?} だった",
                    other
                );
            }
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

/// 205 レスポンスで Content-Length が非 0 は常にエラー
#[test]
fn prop_encode_response_205_with_cl_nonzero_always_error() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let cl = noprop::sample_usize_in(ctx, 1..10000);
        let res = Response::with_status(StatusCode::RESET_CONTENT)
            .header(HeaderName::from_static(b"Content-Length"), cl.to_string())
            .expect("エンコードは成功するはず (実装バグ)");
        let result = encode_response(&res);
        match result {
            Err(EncodeError::ForbiddenContentLength { status_code: 205 }) => {}
            other => {
                panic!(
                    "205 で ForbiddenContentLength を期待したが {:?} だった",
                    other
                );
            }
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
// 304 レスポンスのボディ除外 PBT
// ========================================

/// 304 レスポンスはボディを含めない
#[test]
fn prop_encode_response_304_no_body() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let len = noprop::sample_usize_in(ctx, 1..128);
        let data = noprop::sample_bytes_vec(ctx, len);
        let res = Response::with_status(StatusCode::NOT_MODIFIED).body(data);
        let encoded = encode_response(&res).expect("エンコードは成功するはず (実装バグ)");
        let encoded_str = String::from_utf8_lossy(&encoded);
        let header_end = encoded_str
            .find("\r\n\r\n")
            .expect("エンコードは成功するはず (実装バグ)");
        assert_eq!(encoded.len(), header_end + 4);
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
// Host ヘッダーバリデーション PBT
// ========================================

/// Host ヘッダーの重複は常に DuplicateHostHeader エラーになる
#[test]
fn prop_encode_request_duplicate_host_always_error() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let method = http_method(ctx);
        let uri = uri(ctx);
        // 元の戦略: "[a-z]{3,8}\.com" / "[a-z]{3,8}\.org"
        let host1_len = noprop::sample_usize_in(ctx, 3..=8);
        let host2_len = noprop::sample_usize_in(ctx, 3..=8);
        let mut host1 = String::with_capacity(host1_len + 4);
        let mut host2 = String::with_capacity(host2_len + 4);
        for _ in 0..host1_len {
            host1.push(lower_char(ctx));
        }
        host1.push_str(".com");
        for _ in 0..host2_len {
            host2.push(lower_char(ctx));
        }
        host2.push_str(".org");
        let req = Request::new(method, &uri)
            .expect("エンコードは成功するはず (実装バグ)")
            .header(HeaderName::from_static(b"Host"), &host1)
            .expect("エンコードは成功するはず (実装バグ)")
            .header(HeaderName::from_static(b"Host"), &host2)
            .expect("エンコードは成功するはず (実装バグ)");
        let result = encode_request(&req);
        assert!(matches!(result, Err(EncodeError::DuplicateHostHeader)));
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
// encode_request_headers のエラーパス PBT
// ========================================

/// encode_request_headers でも Host ヘッダー必須
#[test]
fn prop_encode_request_headers_host_required_for_http11() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let method = http_method(ctx);
        let uri = uri(ctx);
        let req = Request::new(method, &uri).expect("エンコードは成功するはず (実装バグ)");
        let result = encode_request_headers(&req);
        assert!(matches!(result, Err(EncodeError::MissingHostHeader)));
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

/// encode_request_headers でも TE+CL 同時禁止
#[test]
fn prop_encode_request_headers_te_and_cl_error() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let method = http_method(ctx);
        let uri = uri(ctx);
        let cl = noprop::sample_usize_in(ctx, 1..10000);
        let req = Request::new(method, &uri)
            .expect("エンコードは成功するはず (実装バグ)")
            .header(HeaderName::from_static(b"Host"), "example.com")
            .expect("エンコードは成功するはず (実装バグ)")
            .header(HeaderName::from_static(b"Transfer-Encoding"), "chunked")
            .expect("エンコードは成功するはず (実装バグ)")
            .header(HeaderName::from_static(b"Content-Length"), cl.to_string())
            .expect("エンコードは成功するはず (実装バグ)");
        let result = encode_request_headers(&req);
        assert!(matches!(
            result,
            Err(EncodeError::ConflictingTransferEncodingAndContentLength)
        ));
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
// encode_response_headers のエラーパス PBT
// ========================================

/// encode_response_headers でも TE+CL 同時禁止
#[test]
fn prop_encode_response_headers_te_and_cl_error() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        // 元の戦略: (200u16..204).prop_union(206..600)
        let status = match noprop::sample_usize_in(ctx, 0..2) {
            0 => noprop::sample_usize_in(ctx, 200..204) as u16,
            _ => noprop::sample_usize_in(ctx, 206..600) as u16,
        };
        let cl = noprop::sample_usize_in(ctx, 1..10000);
        let res = Response::new(status, "OK")
            .expect("エンコードは成功するはず (実装バグ)")
            .header(HeaderName::from_static(b"Transfer-Encoding"), "chunked")
            .expect("エンコードは成功するはず (実装バグ)")
            .header(HeaderName::from_static(b"Content-Length"), cl.to_string())
            .expect("エンコードは成功するはず (実装バグ)");
        let result = encode_response_headers(&res);
        assert!(matches!(
            result,
            Err(EncodeError::ConflictingTransferEncodingAndContentLength)
        ));
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

/// encode_response_headers で 1xx/204+TE 禁止
#[test]
fn prop_encode_response_headers_1xx_or_204_with_te_error() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        // 元の戦略: prop_oneof![100u16..200, Just(204u16)]
        let status = match noprop::sample_usize_in(ctx, 0..2) {
            0 => noprop::sample_usize_in(ctx, 100..200) as u16,
            _ => 204,
        };
        let res = Response::new(status, "Info")
            .expect("エンコードは成功するはず (実装バグ)")
            .header(HeaderName::from_static(b"Transfer-Encoding"), "chunked")
            .expect("エンコードは成功するはず (実装バグ)");
        let result = encode_response_headers(&res);
        match result {
            Err(EncodeError::ForbiddenTransferEncoding { status_code }) => {
                assert_eq!(status_code, status);
            }
            other => {
                panic!("ForbiddenTransferEncoding を期待したが {:?} だった", other);
            }
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

/// encode_response_headers で 1xx/204+CL 禁止
#[test]
fn prop_encode_response_headers_1xx_or_204_with_cl_error() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        // 元の戦略: prop_oneof![100u16..200, Just(204u16)]
        let status = match noprop::sample_usize_in(ctx, 0..2) {
            0 => noprop::sample_usize_in(ctx, 100..200) as u16,
            _ => 204,
        };
        let res = Response::new(status, "Info")
            .expect("エンコードは成功するはず (実装バグ)")
            .header(HeaderName::from_static(b"Content-Length"), "0")
            .expect("エンコードは成功するはず (実装バグ)");
        let result = encode_response_headers(&res);
        match result {
            Err(EncodeError::ForbiddenContentLength { status_code }) => {
                assert_eq!(status_code, status);
            }
            other => {
                panic!("ForbiddenContentLength を期待したが {:?} だった", other);
            }
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

/// encode_response_headers で 205+TE 禁止
#[test]
fn prop_encode_response_headers_205_with_te_error() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        // 元の戦略: prop_oneof![Just("chunked"), Just("gzip")]
        let te_value = match noprop::sample_usize_in(ctx, 0..2) {
            0 => "chunked".to_string(),
            _ => "gzip".to_string(),
        };
        let res = Response::with_status(StatusCode::RESET_CONTENT)
            .header(HeaderName::from_static(b"Transfer-Encoding"), &te_value)
            .expect("エンコードは成功するはず (実装バグ)");
        let result = encode_response_headers(&res);
        match result {
            Err(EncodeError::ForbiddenTransferEncoding { status_code: 205 }) => {}
            other => {
                panic!(
                    "205 で ForbiddenTransferEncoding を期待したが {:?} だった",
                    other
                );
            }
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

/// encode_response_headers で 205+CL(非 0) 禁止
#[test]
fn prop_encode_response_headers_205_with_cl_nonzero_error() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let cl = noprop::sample_usize_in(ctx, 1..10000);
        let res = Response::with_status(StatusCode::RESET_CONTENT)
            .header(HeaderName::from_static(b"Content-Length"), cl.to_string())
            .expect("エンコードは成功するはず (実装バグ)");
        let result = encode_response_headers(&res);
        match result {
            Err(EncodeError::ForbiddenContentLength { status_code: 205 }) => {}
            other => {
                panic!(
                    "205 で ForbiddenContentLength を期待したが {:?} だった",
                    other
                );
            }
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
// メソッドラッパーのテスト
// ========================================

/// Request::encode() は encode_request() と同じ結果を返す
#[test]
fn prop_request_encode_equals_free_function() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let method = http_method(ctx);
        let uri = uri(ctx);
        let req = Request::new(method, &uri)
            .expect("エンコードは成功するはず (実装バグ)")
            .header(HeaderName::from_static(b"Host"), "example.com")
            .expect("エンコードは成功するはず (実装バグ)");
        let via_method = req.encode();
        let via_free = encode_request(&req);
        assert_eq!(via_method, via_free);
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

/// Response::encode() は encode_response() と同じ結果を返す
#[test]
fn prop_response_encode_equals_free_function() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let status = status_code(ctx);
        let phrase = reason_phrase(ctx);
        let res = Response::new(status, phrase).expect("エンコードは成功するはず (実装バグ)");
        let via_method = res.encode();
        let via_free = encode_response(&res);
        assert_eq!(via_method, via_free);
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

/// Request::encode_headers() は encode_request_headers() と同じ結果を返す
#[test]
fn prop_request_encode_headers_equals_free_function() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let method = http_method(ctx);
        let uri = uri(ctx);
        let req = Request::new(method, &uri)
            .expect("エンコードは成功するはず (実装バグ)")
            .header(HeaderName::from_static(b"Host"), "example.com")
            .expect("エンコードは成功するはず (実装バグ)");
        let via_method = req.encode_headers();
        let via_free = encode_request_headers(&req);
        assert_eq!(via_method, via_free);
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

/// Response::encode_headers() は encode_response_headers() と同じ結果を返す
#[test]
fn prop_response_encode_headers_equals_free_function() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let status = status_code(ctx);
        let phrase = reason_phrase(ctx);
        let res = Response::new(status, phrase)
            .expect("エンコードは成功するはず (実装バグ)")
            .header(HeaderName::from_static(b"Content-Type"), "text/html")
            .expect("エンコードは成功するはず (実装バグ)");
        let via_method = res.encode_headers();
        let via_free = encode_response_headers(&res);
        assert_eq!(via_method, via_free);
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
// ResponseEncoder / RequestEncoder の PBT
// ========================================

/// ResponseEncoder::new() で compress_body / finish / reset
#[test]
fn prop_response_encoder_compress_body() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let len = noprop::sample_usize_in(ctx, 1..256);
        let data = noprop::sample_bytes_vec(ctx, len);
        let mut encoder = ResponseEncoder::new();
        let mut output = vec![0u8; 512];

        let status = encoder
            .compress_body(&data, &mut output)
            .expect("エンコードは成功するはず (実装バグ)");
        assert_eq!(status.consumed(), data.len());
        assert_eq!(status.produced(), data.len());
        assert_eq!(&output[..data.len()], &data[..]);

        let finish_status = encoder
            .finish(&mut output)
            .expect("エンコードは成功するはず (実装バグ)");
        assert!(finish_status.is_complete());

        encoder.reset();
        let status2 = encoder
            .compress_body(&data, &mut output)
            .expect("エンコードは成功するはず (実装バグ)");
        assert_eq!(status2.consumed(), data.len());
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

/// RequestEncoder::new() で compress_body / finish / reset
#[test]
fn prop_request_encoder_compress_body() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let len = noprop::sample_usize_in(ctx, 1..256);
        let data = noprop::sample_bytes_vec(ctx, len);
        let mut encoder = RequestEncoder::new();
        let mut output = vec![0u8; 512];

        let status = encoder
            .compress_body(&data, &mut output)
            .expect("エンコードは成功するはず (実装バグ)");
        assert_eq!(status.consumed(), data.len());
        assert_eq!(status.produced(), data.len());
        assert_eq!(&output[..data.len()], &data[..]);

        let finish_status = encoder
            .finish(&mut output)
            .expect("エンコードは成功するはず (実装バグ)");
        assert!(finish_status.is_complete());

        encoder.reset();
        let status2 = encoder
            .compress_body(&data, &mut output)
            .expect("エンコードは成功するはず (実装バグ)");
        assert_eq!(status2.consumed(), data.len());
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

/// ResponseEncoder::default() は new() と同じ動作をする
#[test]
fn prop_response_encoder_default() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let len = noprop::sample_usize_in(ctx, 1..256);
        let data = noprop::sample_bytes_vec(ctx, len);
        let mut encoder: ResponseEncoder = ResponseEncoder::default();
        let mut output = vec![0u8; 512];

        let status = encoder
            .compress_body(&data, &mut output)
            .expect("エンコードは成功するはず (実装バグ)");
        assert_eq!(status.consumed(), data.len());
        assert_eq!(status.produced(), data.len());
        assert_eq!(&output[..data.len()], &data[..]);
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

/// RequestEncoder::default() は new() と同じ動作をする
#[test]
fn prop_request_encoder_default() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let len = noprop::sample_usize_in(ctx, 1..256);
        let data = noprop::sample_bytes_vec(ctx, len);
        let mut encoder: RequestEncoder = RequestEncoder::default();
        let mut output = vec![0u8; 512];

        let status = encoder
            .compress_body(&data, &mut output)
            .expect("エンコードは成功するはず (実装バグ)");
        assert_eq!(status.consumed(), data.len());
        assert_eq!(status.produced(), data.len());
        assert_eq!(&output[..data.len()], &data[..]);
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

/// ResponseEncoder::with_compressor(NoCompression) は new() と同じ動作をする
#[test]
fn prop_response_encoder_with_compressor() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let len = noprop::sample_usize_in(ctx, 1..256);
        let data = noprop::sample_bytes_vec(ctx, len);
        let compressor = shiguredo_http11::compression::NoCompression::new();
        let mut encoder = ResponseEncoder::with_compressor(compressor);
        let mut output = vec![0u8; 512];

        let status = encoder
            .compress_body(&data, &mut output)
            .expect("エンコードは成功するはず (実装バグ)");
        assert_eq!(status.consumed(), data.len());
        assert_eq!(status.produced(), data.len());
        assert_eq!(&output[..data.len()], &data[..]);
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

/// RequestEncoder::with_compressor(NoCompression) は new() と同じ動作をする
#[test]
fn prop_request_encoder_with_compressor() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let len = noprop::sample_usize_in(ctx, 1..256);
        let data = noprop::sample_bytes_vec(ctx, len);
        let compressor = shiguredo_http11::compression::NoCompression::new();
        let mut encoder = RequestEncoder::with_compressor(compressor);
        let mut output = vec![0u8; 512];

        let status = encoder
            .compress_body(&data, &mut output)
            .expect("エンコードは成功するはず (実装バグ)");
        assert_eq!(status.consumed(), data.len());
        assert_eq!(status.produced(), data.len());
        assert_eq!(&output[..data.len()], &data[..]);
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
