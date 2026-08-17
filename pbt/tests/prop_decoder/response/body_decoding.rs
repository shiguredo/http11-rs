//! ResponseDecoder のボディデコード関連プロパティテスト
//!
//! chunked / Content-Length / close-delimited / None / Tunnel の各 BodyKind と
//! CONNECT トンネル、1xx の TE 無視、Content-Length のカンマ区切り値などの
//! ボディ解釈ロジックを対象にする。

use shiguredo_http11::{BodyKind, BodyProgress, Error, HttpHead, Response, ResponseDecoder};

use crate::{body, reason_phrase, status_code};

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

// ========================================
// UTF-8 エラー PBT (チャンクサイズ)
// ========================================

#[test]
fn prop_invalid_utf8_chunk_size_error() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        // 無効な UTF-8 バイトを含むチャンクサイズはエラー
        let invalid_byte = noprop::sample_usize_in(ctx, 128..=255) as u8;
        let mut data = b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n".to_vec();
        data.push(invalid_byte);
        data.extend(b"\r\n");
        let mut decoder = ResponseDecoder::new();
        decoder
            .feed(&data)
            .expect("レスポンスのデコードは成功するはず (実装バグ)");
        let (_, _) = decoder
            .decode_headers()
            .expect("結果は存在するはず (実装バグ)")
            .expect("レスポンスのデコードは成功するはず (実装バグ)");
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
// 部分的なデータ (None を返す) PBT (レスポンス)
// ========================================

#[test]
fn prop_incomplete_chunk_size() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        // 不完全なチャンクサイズ行は None
        let size = noprop::sample_usize_in(ctx, 1..100);
        let data = format!(
            "HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n{:x}",
            size
        );
        let mut decoder = ResponseDecoder::new();
        decoder
            .feed(data.as_bytes())
            .expect("レスポンスのデコードは成功するはず (実装バグ)");
        let (_, _) = decoder
            .decode_headers()
            .expect("結果は存在するはず (実装バグ)")
            .expect("レスポンスのデコードは成功するはず (実装バグ)");
        // peek_body は None (チャンクサイズ行が不完全)
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
fn prop_incomplete_chunk_data() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        // 不完全なチャンクデータは部分データを返す
        let chunk_size = noprop::sample_usize_in(ctx, 10..100);
        let partial_size = noprop::sample_usize_in(ctx, 1..10);
        let partial_data = "x".repeat(partial_size);
        let data = format!(
            "HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n{:x}\r\n{}",
            chunk_size, partial_data
        );
        let mut decoder = ResponseDecoder::new();
        decoder
            .feed(data.as_bytes())
            .expect("レスポンスのデコードは成功するはず (実装バグ)");
        let (_, _) = decoder
            .decode_headers()
            .expect("結果は存在するはず (実装バグ)")
            .expect("レスポンスのデコードは成功するはず (実装バグ)");
        decoder
            .progress()
            .expect("レスポンスのデコードは成功するはず (実装バグ)"); // チャンクサイズを処理
        let peeked = decoder
            .peek_body()
            .expect("レスポンスのデコードは成功するはず (実装バグ)");
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
fn prop_incomplete_trailer() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        // 不完全なトレーラーは Complete に到達してはならない
        let body_content = lower_string(ctx, 1..=32);
        let trailer_name = alpha_string(ctx, 1..=16);
        let len = body_content.len();
        let data = format!(
            "HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n{:x}\r\n{}\r\n0\r\n{}: value",
            len, body_content, trailer_name
        );
        let mut decoder = ResponseDecoder::new();
        decoder
            .feed(data.as_bytes())
            .expect("レスポンスのデコードは成功するはず (実装バグ)");
        let (_, _) = decoder
            .decode_headers()
            .expect("結果は存在するはず (実装バグ)")
            .expect("レスポンスのデコードは成功するはず (実装バグ)");

        // ボディを消費。多段遷移を経て最終的に NeedData (もしくは Complete) に収束する。
        // 不完全トレーラの場合は Complete に到達してはならない。
        let final_result = loop {
            if let Some(data) = decoder.peek_body() {
                let len = data.len();
                match decoder
                    .consume_body(len)
                    .expect("レスポンスのデコードは成功するはず (実装バグ)")
                {
                    r @ BodyProgress::Complete { .. } => break r,
                    BodyProgress::Advanced | BodyProgress::NeedData => continue,
                }
            }
            match decoder
                .progress()
                .expect("レスポンスのデコードは成功するはず (実装バグ)")
            {
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
// Content-Length / Transfer-Encoding なしのボディ判定
// ========================================

#[test]
fn prop_response_no_content_length_no_transfer_encoding() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        // RFC 9112: Content-Length も Transfer-Encoding もない場合は close-delimited
        // (接続が閉じられるまでがボディ)
        let status_code = noprop::sample_usize_in(ctx, 200..204) as u16;
        let data = format!("HTTP/1.1 {} OK\r\n\r\n", status_code);
        let mut decoder = ResponseDecoder::new();
        decoder
            .feed(data.as_bytes())
            .expect("レスポンスのデコードは成功するはず (実装バグ)");
        let (_, body_kind) = decoder
            .decode_headers()
            .expect("結果は存在するはず (実装バグ)")
            .expect("レスポンスのデコードは成功するはず (実装バグ)");
        assert_eq!(body_kind, BodyKind::CloseDelimited);
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
fn prop_response_content_length_zero() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        // Content-Length: 0 はボディなし
        let status_code = noprop::sample_usize_in(ctx, 200..204) as u16;
        let data = format!("HTTP/1.1 {} OK\r\nContent-Length: 0\r\n\r\n", status_code);
        let mut decoder = ResponseDecoder::new();
        decoder
            .feed(data.as_bytes())
            .expect("レスポンスのデコードは成功するはず (実装バグ)");
        let (_, body_kind) = decoder
            .decode_headers()
            .expect("結果は存在するはず (実装バグ)")
            .expect("レスポンスのデコードは成功するはず (実装バグ)");
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

// ========================================
// decode_headers 前の consume_body はエラー
// ========================================

#[test]
fn prop_response_consume_body_before_decode_headers_error() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let status_code = noprop::sample_usize_in(ctx, 200..600) as u16;
        let mut decoder = ResponseDecoder::new();
        let data = format!("HTTP/1.1 {} OK\r\n\r\n", status_code);
        decoder
            .feed(data.as_bytes())
            .expect("レスポンスのデコードは成功するはず (実装バグ)");
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
// Response ラウンドトリップ PBT
// ========================================

#[test]
fn prop_response_roundtrip() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);
    let body_forbidden_seen = std::cell::Cell::new(0usize);
    let body_allowed_seen = std::cell::Cell::new(0usize);

    runner.run(256, |ctx| {
        let status = status_code(ctx);
        let reason = reason_phrase(ctx);
        let body_data = body(ctx);
        let mut response =
            Response::new(status, &reason).expect("レスポンスのデコードは成功するはず (実装バグ)");

        // RFC 9110: 1xx/204/205/304 はエンコーダー側でボディ生成を禁止
        // (デコーダー側では 205 はメッセージ長決定規則に従うが、ラウンドトリップテストでは
        //  エンコーダーの制約に合わせる)
        let status_forbids_body =
            (100..200).contains(&status) || status == 204 || status == 205 || status == 304;

        if status_forbids_body {
            // 205 は status_has_body=true かつボディ禁止のため、
            // close-delimited を避けるには Content-Length: 0 を明示する必要がある。
            // 1xx/204/304 は status_has_body=false なので body 設定不要。
            body_forbidden_seen.set(body_forbidden_seen.get() + 1);
            if status == 205 {
                response = response.body(Vec::new());
            }
        } else {
            // body=None のままだと close-delimited になるため、空でも明示的に
            // body() を呼んで Content-Length: 0 を付与する。
            body_allowed_seen.set(body_allowed_seen.get() + 1);
            response = response.body(body_data.clone());
        }

        let encoded = response
            .encode()
            .expect("レスポンスのデコードは成功するはず (実装バグ)");
        let mut decoder = ResponseDecoder::new();
        decoder
            .feed(&encoded)
            .expect("レスポンスのデコードは成功するはず (実装バグ)");
        let decoded = decoder
            .decode()
            .expect("結果は存在するはず (実装バグ)")
            .expect("レスポンスのデコードは成功するはず (実装バグ)");

        assert_eq!(decoded.status_code(), status);
        if !status_forbids_body {
            // 空ボディも .body(vec![]) で明示しているため、デコーダーは Some(vec![]) を返す。
            assert_eq!(decoded.body_bytes(), Some(body_data.as_slice()));
        }
        Ok(())
    })?;

    // p 推定値: status_code() は全 82 値のうち 1xx/204/205/304 が 5 値なので
    // ボディ禁止側の確率は 5/82 ≈ 0.061。256 ケースでの miss 確率は
    // (1 - 5/82)^256 ≈ 1e-7。
    assert!(
        body_forbidden_seen.get() > 0,
        "ボディ禁止ステータスコード側が一度も検証されなかった\n{runner}"
    );
    assert!(
        body_allowed_seen.get() > 0,
        "ボディ許可ステータスコード側が一度も検証されなかった\n{runner}"
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
// CONNECT トンネルモードの PBT
// ========================================

/// CONNECT + 2xx (204 を除く) の全ステータスコードでトンネルモードになることを確認
///
/// 204 は除外する: RFC 9112 Section 6.3 の "in order of precedence" により
/// item 1 (1xx/204/304 はボディなし) が item 2 (CONNECT 2xx はトンネル) より
/// 優先されるため、CONNECT + 204 は `BodyKind::None` になる。
///
/// status == 204 のときはケース全体を棄却するため、valid-by-construction チェック
/// (rejected_cases == 0) は付けない。
#[test]
fn prop_connect_all_2xx_tunnel() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let status = noprop::sample_usize_in(ctx, 200..300) as u16;
        if status == 204 {
            ctx.reject_case();
        }
        let mut decoder = ResponseDecoder::new();
        decoder.set_request_method("CONNECT");

        let response = format!("HTTP/1.1 {} OK\r\n\r\n", status);
        decoder
            .feed(response.as_bytes())
            .expect("レスポンスのデコードは成功するはず (実装バグ)");

        let result = decoder
            .decode_headers()
            .expect("結果は存在するはず (実装バグ)")
            .expect("レスポンスのデコードは成功するはず (実装バグ)");
        assert_eq!(
            result.1,
            BodyKind::Tunnel,
            "expected Tunnel for CONNECT {}",
            status
        );
        assert!(decoder.is_tunnel());
        Ok(())
    })?;

    // このテストは status != 204 を前提とするため、status == 204 のケースを
    // ctx.reject_case() で棄却している。ジェネレータは意図的に rejection を使う
    // ため、valid-by-construction チェック (rejected_cases == 0) は付けない
    // (204 は 100 値中 1 値なので棄却率は約 1% と低く、256 ケースの到達には影響しない)。
    Ok(())
}

// ========================================
// RFC 9112 Section 6.3 準拠テスト
// ========================================

/// 1xx レスポンスで不正な Transfer-Encoding があってもエラーにならない
#[test]
fn prop_1xx_ignores_invalid_te() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let status = noprop::sample_usize_in(ctx, 100..200) as u16;
        let mut decoder = ResponseDecoder::new();
        // gzip のみは通常エラーだが、1xx では無視される
        let response = format!(
            "HTTP/1.1 {} Continue\r\nTransfer-Encoding: gzip\r\n\r\n",
            status
        );
        decoder
            .feed(response.as_bytes())
            .expect("レスポンスのデコードは成功するはず (実装バグ)");

        let result = decoder
            .decode_headers()
            .expect("結果は存在するはず (実装バグ)")
            .expect("レスポンスのデコードは成功するはず (実装バグ)");
        assert_eq!(result.1, BodyKind::None, "1xx should have no body");
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

/// 同じ値のカンマ区切り Content-Length は受理される
#[test]
fn prop_cl_comma_same_values() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let len = noprop::sample_usize_in(ctx, 0..10000);
        let mut decoder = ResponseDecoder::new();
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Length: {}, {}, {}\r\n\r\n",
            len, len, len
        );
        decoder
            .feed(response.as_bytes())
            .expect("レスポンスのデコードは成功するはず (実装バグ)");

        let result = decoder
            .decode_headers()
            .expect("結果は存在するはず (実装バグ)")
            .expect("レスポンスのデコードは成功するはず (実装バグ)");
        assert_eq!(result.1, BodyKind::ContentLength(len as u64));
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

/// 異なる値のカンマ区切り Content-Length はエラー
///
/// len1 != len2 を前提とするため、len1 == len2 のケースは ctx.reject_case() で
/// 棄却する。このテストは意図的に rejection を使うため、valid-by-construction
/// チェック (rejected_cases == 0) は付けない (一致する確率は 1/10000 と低く、
/// 256 ケースの到達には影響しない)。
#[test]
fn prop_cl_comma_different_values_error() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let len1 = noprop::sample_usize_in(ctx, 0..10000);
        let len2 = noprop::sample_usize_in(ctx, 0..10000);
        if len1 == len2 {
            ctx.reject_case();
        }

        let mut decoder = ResponseDecoder::new();
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Length: {}, {}\r\n\r\n",
            len1, len2
        );
        decoder
            .feed(response.as_bytes())
            .expect("レスポンスのデコードは成功するはず (実装バグ)");

        assert!(decoder.decode_headers().is_err());
        Ok(())
    })?;

    Ok(())
}

// ========================================
// トンネルモードの PBT (decode エラー)
// ========================================

/// CONNECT 2xx (204 を除く) 後に decode() → エラー
///
/// 204 は除外する: RFC 9112 Section 6.3 の "in order of precedence" により
/// CONNECT + 204 は `BodyKind::None` になり、`decode()` はエラーにならない。
///
/// status == 204 のときはケース全体を棄却するため、valid-by-construction チェック
/// (rejected_cases == 0) は付けない。
#[test]
fn prop_response_decode_tunnel_error() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let status = noprop::sample_usize_in(ctx, 200..300) as u16;
        if status == 204 {
            ctx.reject_case();
        }
        let mut decoder = ResponseDecoder::new();
        decoder.set_request_method("CONNECT");

        let response_data = format!("HTTP/1.1 {} OK\r\n\r\n", status);
        decoder
            .feed(response_data.as_bytes())
            .expect("レスポンスのデコードは成功するはず (実装バグ)");

        let result = decoder.decode();
        assert!(result.is_err());
        if let Err(Error::InvalidData(msg)) = result {
            assert!(msg.contains("tunnel"));
        }
        Ok(())
    })?;

    // このテストは status != 204 を前提とするため、status == 204 のケースを
    // ctx.reject_case() で棄却している。ジェネレータは意図的に rejection を使う
    // ため、valid-by-construction チェック (rejected_cases == 0) は付けない
    // (204 は 100 値中 1 値なので棄却率は約 1% と低く、256 ケースの到達には影響しない)。
    Ok(())
}

// ========================================
// CONNECT 2xx で Transfer-Encoding / Content-Length が ResponseHead から消える
// ========================================

/// CONNECT への 2xx レスポンスでは Transfer-Encoding / Content-Length が
/// ResponseHead.headers から消去される (RFC 9110 Section 9.3.6 MUST ignore)
///
/// RFC 9112 Section 6.3 の precedence により item 1 (1xx/204/304 はボディなし) が
/// item 2 (CONNECT 2xx は Tunnel) より優先されるため、204 は範囲から除外する
/// (CONNECT + 204 は `BodyKind::None`)。
#[test]
fn prop_connect_2xx_drops_te_cl_from_head() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let status = match noprop::sample_usize_in(ctx, 0..2) {
            0 => noprop::sample_usize_in(ctx, 200..=203) as u16,
            _ => noprop::sample_usize_in(ctx, 205..=299) as u16,
        };
        let cl = noprop::sample_u64_in(ctx, 0..1_000_000);
        let response = format!(
            "HTTP/1.1 {} OK\r\nTransfer-Encoding: chunked\r\nContent-Length: {}\r\n\r\n",
            status, cl
        );
        let mut decoder = ResponseDecoder::new();
        decoder.set_request_method("CONNECT");
        decoder
            .feed(response.as_bytes())
            .expect("レスポンスのデコードは成功するはず (実装バグ)");
        let (head, body_kind) = decoder
            .decode_headers()
            .expect("結果は存在するはず (実装バグ)")
            .expect("レスポンスのデコードは成功するはず (実装バグ)");
        assert_eq!(body_kind, BodyKind::Tunnel);
        assert_eq!(head.get_header("Transfer-Encoding"), None);
        assert_eq!(head.get_header("Content-Length"), None);
        assert!(!head.is_chunked());
        assert_eq!(
            head.content_length()
                .expect("レスポンスのデコードは成功するはず (実装バグ)"),
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

/// CONNECT への 1xx / 3xx / 4xx / 5xx レスポンスでは
/// Content-Length が ResponseHead.headers に残る (Tunnel に遷移しないため)
#[test]
fn prop_connect_non_2xx_keeps_cl_in_head() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let status = match noprop::sample_usize_in(ctx, 0..3) {
            0 => noprop::sample_usize_in(ctx, 300..400) as u16,
            1 => noprop::sample_usize_in(ctx, 400..500) as u16,
            _ => noprop::sample_usize_in(ctx, 500..600) as u16,
        };
        let cl = noprop::sample_u64_in(ctx, 0..1_000);
        let body = "x".repeat(cl as usize);
        let response = format!(
            "HTTP/1.1 {} Some\r\nContent-Length: {}\r\n\r\n{}",
            status, cl, body
        );
        let mut decoder = ResponseDecoder::new();
        decoder.set_request_method("CONNECT");
        decoder
            .feed(response.as_bytes())
            .expect("レスポンスのデコードは成功するはず (実装バグ)");
        let (head, body_kind) = decoder
            .decode_headers()
            .expect("結果は存在するはず (実装バグ)")
            .expect("レスポンスのデコードは成功するはず (実装バグ)");
        // status_has_body 系のロジックは status_code 単位で判定されるため、ここでは
        // CL がそのまま残っていることだけを検証する。
        let _ = body_kind;
        let cl_str = cl.to_string();
        assert_eq!(head.get_header("Content-Length"), Some(cl_str.as_str()));
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
