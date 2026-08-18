//! request-target 形式のプロパティテスト (RFC 9112 Section 3.2)

use shiguredo_http11::RequestDecoder;

// ========================================
// 生成関数
// ========================================

// パス用文字 (RFC 3986 pchar + "/")
const PATH_CHARS: &[char] = &[
    'a', 'b', 'c', 'd', 'e', 'f', 'g', 'h', 'i', 'j', 'k', 'l', 'm', 'n', 'o', 'p', 'q', 'r', 's',
    't', 'u', 'v', 'w', 'x', 'y', 'z', 'A', 'B', 'C', 'D', 'E', 'F', 'G', 'H', 'I', 'J', 'K', 'L',
    'M', 'N', 'O', 'P', 'Q', 'R', 'S', 'T', 'U', 'V', 'W', 'X', 'Y', 'Z', '0', '1', '2', '3', '4',
    '5', '6', '7', '8', '9', '-', '.', '_', '~', '/', ':', '@', '!', '$', '&', '\'', '(', ')', '*',
    '+', ',', ';', '=',
];

/// パスの 1 文字を生成する
fn path_char(ctx: &mut noprop::TestCaseContext) -> char {
    PATH_CHARS[noprop::sample_usize_in(ctx, 0..PATH_CHARS.len())]
}

/// パスセグメント (1..32 文字) を生成する
fn path_segment(ctx: &mut noprop::TestCaseContext) -> String {
    let len = noprop::sample_usize_in(ctx, 1..32);
    let mut s = String::with_capacity(len);
    for _ in 0..len {
        s.push(path_char(ctx));
    }
    s
}

/// origin-form URI を生成する
fn origin_form_uri(ctx: &mut noprop::TestCaseContext) -> String {
    match noprop::sample_usize_in(ctx, 0..3) {
        0 => "/".to_string(),
        1 => format!("/{}", path_segment(ctx)),
        _ => format!("/{}/{}", path_segment(ctx), path_segment(ctx)),
    }
}

/// クエリ付き origin-form を生成する
fn origin_form_with_query(ctx: &mut noprop::TestCaseContext) -> String {
    format!("{}?{}", origin_form_uri(ctx), path_segment(ctx))
}

/// ホスト名を生成する
fn hostname(ctx: &mut noprop::TestCaseContext) -> String {
    let len = noprop::sample_usize_in(ctx, 1..16);
    let mut s = String::with_capacity(len);
    for _ in 0..len {
        s.push(char::from(b'a' + noprop::sample_usize_in(ctx, 0..26) as u8));
    }
    format!("{}.com", s)
}

/// absolute-form URI を生成する
fn absolute_form_uri(ctx: &mut noprop::TestCaseContext) -> String {
    format!("http://{}{}", hostname(ctx), origin_form_uri(ctx))
}

/// authority-form (host:port) を生成する
fn authority_form_uri(ctx: &mut noprop::TestCaseContext) -> String {
    let port = noprop::sample_usize_in(ctx, 1..=65535) as u16;
    format!("{}:{}", hostname(ctx), port)
}

/// URN の NID (2..=8 文字の a-z) を生成する
fn urn_nid(ctx: &mut noprop::TestCaseContext) -> String {
    let len = noprop::sample_usize_in(ctx, 2..=8);
    let mut s = String::with_capacity(len);
    for _ in 0..len {
        s.push(char::from(b'a' + noprop::sample_usize_in(ctx, 0..26) as u8));
    }
    s
}

/// URN の NSS (1..=32 文字) を生成する
fn urn_nss(ctx: &mut noprop::TestCaseContext) -> String {
    const URN_NSS_CHARS: &[char] = &[
        'a', 'b', 'c', 'd', 'e', 'f', 'g', 'h', 'i', 'j', 'k', 'l', 'm', 'n', 'o', 'p', 'q', 'r',
        's', 't', 'u', 'v', 'w', 'x', 'y', 'z', 'A', 'B', 'C', 'D', 'E', 'F', 'G', 'H', 'I', 'J',
        'K', 'L', 'M', 'N', 'O', 'P', 'Q', 'R', 'S', 'T', 'U', 'V', 'W', 'X', 'Y', 'Z', '0', '1',
        '2', '3', '4', '5', '6', '7', '8', '9', ':', '.', '-',
    ];
    let len = noprop::sample_usize_in(ctx, 1..=32);
    let mut s = String::with_capacity(len);
    for _ in 0..len {
        s.push(URN_NSS_CHARS[noprop::sample_usize_in(ctx, 0..URN_NSS_CHARS.len())]);
    }
    s
}

// ========================================
// origin-form テスト
// ========================================

#[test]
fn prop_origin_form_with_get_succeeds() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let uri = origin_form_uri(ctx);
        let request_line = format!("GET {} HTTP/1.1\r\nHost: example.com\r\n\r\n", uri);
        let mut decoder = RequestDecoder::new();
        decoder
            .feed(request_line.as_bytes())
            .expect("リクエストターゲットのパースは成功するはず (実装バグ)");
        let result = decoder.decode_headers();
        assert!(result.is_ok(), "GET + origin-form は成功すべき: {}", uri);
        assert!(
            result
                .expect("リクエストターゲットのパースは成功するはず (実装バグ)")
                .is_some()
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
fn prop_origin_form_with_query_succeeds() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let uri = origin_form_with_query(ctx);
        let request_line = format!("GET {} HTTP/1.1\r\nHost: example.com\r\n\r\n", uri);
        let mut decoder = RequestDecoder::new();
        decoder
            .feed(request_line.as_bytes())
            .expect("リクエストターゲットのパースは成功するはず (実装バグ)");
        let result = decoder.decode_headers();
        assert!(
            result.is_ok(),
            "GET + origin-form?query は成功すべき: {}",
            uri
        );
        assert!(
            result
                .expect("リクエストターゲットのパースは成功するはず (実装バグ)")
                .is_some()
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
fn prop_origin_form_with_post_succeeds() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let uri = origin_form_uri(ctx);
        let request_line = format!("POST {} HTTP/1.1\r\nHost: example.com\r\n\r\n", uri);
        let mut decoder = RequestDecoder::new();
        decoder
            .feed(request_line.as_bytes())
            .expect("リクエストターゲットのパースは成功するはず (実装バグ)");
        let result = decoder.decode_headers();
        assert!(result.is_ok(), "POST + origin-form は成功すべき: {}", uri);
        assert!(
            result
                .expect("リクエストターゲットのパースは成功するはず (実装バグ)")
                .is_some()
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
// absolute-form テスト
// ========================================

#[test]
fn prop_absolute_form_with_get_succeeds() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let uri = absolute_form_uri(ctx);
        let request_line = format!("GET {} HTTP/1.1\r\nHost: example.com\r\n\r\n", uri);
        let mut decoder = RequestDecoder::new();
        decoder
            .feed(request_line.as_bytes())
            .expect("リクエストターゲットのパースは成功するはず (実装バグ)");
        let result = decoder.decode_headers();
        assert!(result.is_ok(), "GET + absolute-form は成功すべき: {}", uri);
        assert!(
            result
                .expect("リクエストターゲットのパースは成功するはず (実装バグ)")
                .is_some()
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
// authority-form テスト
// ========================================

#[test]
fn prop_authority_form_with_connect_succeeds() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let uri = authority_form_uri(ctx);
        let request_line = format!("CONNECT {} HTTP/1.1\r\nHost: {}\r\n\r\n", uri, uri);
        let mut decoder = RequestDecoder::new();
        decoder
            .feed(request_line.as_bytes())
            .expect("リクエストターゲットのパースは成功するはず (実装バグ)");
        let result = decoder.decode_headers();
        assert!(
            result.is_ok(),
            "CONNECT + authority-form は成功すべき: {}",
            uri
        );
        assert!(
            result
                .expect("リクエストターゲットのパースは成功するはず (実装バグ)")
                .is_some()
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
fn prop_authority_form_with_get_fails() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let uri = authority_form_uri(ctx);
        let request_line = format!("GET {} HTTP/1.1\r\nHost: example.com\r\n\r\n", uri);
        let mut decoder = RequestDecoder::new();
        decoder
            .feed(request_line.as_bytes())
            .expect("リクエストターゲットのパースは成功するはず (実装バグ)");
        let result = decoder.decode_headers();
        assert!(
            result.is_err(),
            "GET + authority-form は失敗すべき: {}",
            uri
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
fn prop_authority_form_with_post_fails() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let uri = authority_form_uri(ctx);
        let request_line = format!("POST {} HTTP/1.1\r\nHost: example.com\r\n\r\n", uri);
        let mut decoder = RequestDecoder::new();
        decoder
            .feed(request_line.as_bytes())
            .expect("リクエストターゲットのパースは成功するはず (実装バグ)");
        let result = decoder.decode_headers();
        assert!(
            result.is_err(),
            "POST + authority-form は失敗すべき: {}",
            uri
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
// フラグメント禁止テスト (RFC 9112)
// ========================================

#[test]
fn prop_fragment_in_request_target_fails() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let path = origin_form_uri(ctx);
        let fragment = path_segment(ctx);
        let uri = format!("{}#{}", path, fragment);
        let request_line = format!("GET {} HTTP/1.1\r\nHost: example.com\r\n\r\n", uri);
        let mut decoder = RequestDecoder::new();
        decoder
            .feed(request_line.as_bytes())
            .expect("リクエストターゲットのパースは成功するはず (実装バグ)");
        let result = decoder.decode_headers();
        assert!(
            result.is_err(),
            "fragment を含む request-target は失敗すべき: {}",
            uri
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
fn prop_fragment_in_absolute_form_fails() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let uri = absolute_form_uri(ctx);
        let fragment = path_segment(ctx);
        let uri_with_fragment = format!("{}#{}", uri, fragment);
        let request_line = format!(
            "GET {} HTTP/1.1\r\nHost: example.com\r\n\r\n",
            uri_with_fragment
        );
        let mut decoder = RequestDecoder::new();
        decoder
            .feed(request_line.as_bytes())
            .expect("リクエストターゲットのパースは成功するはず (実装バグ)");
        let result = decoder.decode_headers();
        assert!(
            result.is_err(),
            "fragment を含む absolute-form は失敗すべき: {}",
            uri_with_fragment
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
// CONNECT メソッド制限テスト (RFC 9112 Section 3.2.3)
// ========================================

#[test]
fn prop_connect_with_origin_form_fails() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let uri = origin_form_uri(ctx);
        let request_line = format!("CONNECT {} HTTP/1.1\r\nHost: example.com\r\n\r\n", uri);
        let mut decoder = RequestDecoder::new();
        decoder
            .feed(request_line.as_bytes())
            .expect("リクエストターゲットのパースは成功するはず (実装バグ)");
        let result = decoder.decode_headers();
        assert!(
            result.is_err(),
            "CONNECT + origin-form は失敗すべき: {}",
            uri
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
fn prop_connect_with_absolute_form_fails() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let uri = absolute_form_uri(ctx);
        let request_line = format!("CONNECT {} HTTP/1.1\r\nHost: example.com\r\n\r\n", uri);
        let mut decoder = RequestDecoder::new();
        decoder
            .feed(request_line.as_bytes())
            .expect("リクエストターゲットのパースは成功するはず (実装バグ)");
        let result = decoder.decode_headers();
        assert!(
            result.is_err(),
            "CONNECT + absolute-form は失敗すべき: {}",
            uri
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
// OPTIONS メソッド制限テスト (RFC 9112 Section 3.2.4)
// ========================================

#[test]
fn prop_options_with_origin_form_succeeds() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let uri = origin_form_uri(ctx);
        let request_line = format!("OPTIONS {} HTTP/1.1\r\nHost: example.com\r\n\r\n", uri);
        let mut decoder = RequestDecoder::new();
        decoder
            .feed(request_line.as_bytes())
            .expect("リクエストターゲットのパースは成功するはず (実装バグ)");
        let result = decoder.decode_headers();
        assert!(
            result.is_ok(),
            "OPTIONS + origin-form は成功すべき: {}",
            uri
        );
        assert!(
            result
                .expect("リクエストターゲットのパースは成功するはず (実装バグ)")
                .is_some()
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
fn prop_options_with_absolute_form_succeeds() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let uri = absolute_form_uri(ctx);
        let request_line = format!("OPTIONS {} HTTP/1.1\r\nHost: example.com\r\n\r\n", uri);
        let mut decoder = RequestDecoder::new();
        decoder
            .feed(request_line.as_bytes())
            .expect("リクエストターゲットのパースは成功するはず (実装バグ)");
        let result = decoder.decode_headers();
        assert!(
            result.is_ok(),
            "OPTIONS + absolute-form は成功すべき: {}",
            uri
        );
        assert!(
            result
                .expect("リクエストターゲットのパースは成功するはず (実装バグ)")
                .is_some()
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
fn prop_options_with_authority_form_fails() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let uri = authority_form_uri(ctx);
        let request_line = format!("OPTIONS {} HTTP/1.1\r\nHost: example.com\r\n\r\n", uri);
        let mut decoder = RequestDecoder::new();
        decoder
            .feed(request_line.as_bytes())
            .expect("リクエストターゲットのパースは成功するはず (実装バグ)");
        let result = decoder.decode_headers();
        assert!(
            result.is_err(),
            "OPTIONS + authority-form は失敗すべき: {}",
            uri
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
// "://" なしの absolute-form テスト
// ========================================

// urn: スキームの absolute-form ("://" を含まない)

#[test]
fn prop_urn_absolute_form_succeeds() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let nid = urn_nid(ctx);
        let nss = urn_nss(ctx);
        let uri = format!("urn:{}:{}", nid, nss);
        let raw = format!("GET {} HTTP/1.1\r\nHost: \r\n\r\n", uri);
        let mut decoder = RequestDecoder::new();
        decoder
            .feed(raw.as_bytes())
            .expect("リクエストターゲットのパースは成功するはず (実装バグ)");
        let result = decoder.decode_headers();
        assert!(
            result.is_ok(),
            "urn: の absolute-form は成功すべき: {}",
            uri
        );
        let (head, _) = result
            .expect("結果は存在するはず (実装バグ)")
            .expect("リクエストターゲットのパースは成功するはず (実装バグ)");
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
