//! Content-Location ヘッダーのプロパティテスト

use shiguredo_http11::content_location::ContentLocation;

// ========================================
// ジェネレータ定義
// ========================================

// スキーム
fn scheme(ctx: &mut noprop::TestCaseContext) -> &'static str {
    noprop::sample_choice(ctx, &["http", "https", "ftp", "file"])
}

// ホスト名
fn hostname(ctx: &mut noprop::TestCaseContext) -> String {
    match noprop::sample_usize_in(ctx, 0..4) {
        0 => "localhost".to_string(),
        1 => "example.com".to_string(),
        2 => "test.example.org".to_string(),
        // "[a-z]{1,8}(\\.[a-z]{1,8}){0,2}" に相当するランダム生成
        _ => {
            let mut host = lowercase_label(ctx);
            let label_count = noprop::sample_usize_in(ctx, 0..=2);
            for _ in 0..label_count {
                host.push('.');
                host.push_str(&lowercase_label(ctx));
            }
            host
        }
    }
}

// IPv4 アドレス
fn ipv4(ctx: &mut noprop::TestCaseContext) -> String {
    format!(
        "{}.{}.{}.{}",
        noprop::sample_u8(ctx),
        noprop::sample_u8(ctx),
        noprop::sample_u8(ctx),
        noprop::sample_u8(ctx),
    )
}

// パスセグメント ([a-zA-Z0-9._-]{1,16})
fn path_segment(ctx: &mut noprop::TestCaseContext) -> String {
    const CHARS: &[u8] = b"abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789._-";
    let len = noprop::sample_usize_in(ctx, 1..=16);
    let mut s = String::with_capacity(len);
    for _ in 0..len {
        s.push(CHARS[noprop::sample_usize_in(ctx, 0..CHARS.len())] as char);
    }
    s
}

// パス
fn path(ctx: &mut noprop::TestCaseContext) -> String {
    match noprop::sample_usize_in(ctx, 0..4) {
        0 => "/".to_string(),
        1 => format!("/{}", path_segment(ctx)),
        2 => format!("/{}/{}", path_segment(ctx), path_segment(ctx)),
        _ => format!(
            "/{}/{}/{}",
            path_segment(ctx),
            path_segment(ctx),
            path_segment(ctx),
        ),
    }
}

// クエリ文字列
fn query(ctx: &mut noprop::TestCaseContext) -> String {
    match noprop::sample_usize_in(ctx, 0..3) {
        0 => String::new(),
        1 => format!("?{}", query_pair(ctx)),
        _ => format!("?{}&{}", query_pair(ctx), query_pair(ctx)),
    }
}

// [a-z]{1,8}=[a-z0-9]{1,8} の 1 ペア (クエリ文字列の `?` なし)
fn query_pair(ctx: &mut noprop::TestCaseContext) -> String {
    format!("{}={}", lowercase_label(ctx), lowercase_digit_label(ctx))
}

// フラグメント
fn fragment(ctx: &mut noprop::TestCaseContext) -> String {
    format!("#{}", lowercase_label(ctx))
}

// [a-z]{1,8} の小文字ラベル
fn lowercase_label(ctx: &mut noprop::TestCaseContext) -> String {
    let len = noprop::sample_usize_in(ctx, 1..=8);
    let mut s = String::with_capacity(len);
    for _ in 0..len {
        s.push(char::from(b'a' + noprop::sample_usize_in(ctx, 0..26) as u8));
    }
    s
}

// [a-z0-9]{1,8} のラベル
fn lowercase_digit_label(ctx: &mut noprop::TestCaseContext) -> String {
    const CHARS: &[u8] = b"abcdefghijklmnopqrstuvwxyz0123456789";
    let len = noprop::sample_usize_in(ctx, 1..=8);
    let mut s = String::with_capacity(len);
    for _ in 0..len {
        s.push(CHARS[noprop::sample_usize_in(ctx, 0..CHARS.len())] as char);
    }
    s
}

// 絶対 URI (フラグメントなし: RFC 9110 Section 8.7)
fn absolute_uri(ctx: &mut noprop::TestCaseContext) -> String {
    format!(
        "{}://{}{}{}",
        scheme(ctx),
        hostname(ctx),
        path(ctx),
        query(ctx),
    )
}

// 相対 URI (フラグメントなし: RFC 9110 Section 8.7)
fn relative_uri(ctx: &mut noprop::TestCaseContext) -> String {
    format!("{}{}", path(ctx), query(ctx))
}

// 有効な URI
fn valid_uri(ctx: &mut noprop::TestCaseContext) -> String {
    match noprop::sample_usize_in(ctx, 0..2) {
        0 => absolute_uri(ctx),
        _ => relative_uri(ctx),
    }
}

// ========================================
// 絶対 URI のテスト
// ========================================

// 絶対 URI のラウンドトリップ
#[test]
fn prop_content_location_absolute_uri_roundtrip() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let uri = absolute_uri(ctx);
        let cl = ContentLocation::parse(&uri)
            .expect("Content-Location のパースは成功するはず (実装バグ)");
        let display = cl.to_string();

        // Display した結果を再パースして path() が一致することを検証
        let reparsed = ContentLocation::parse(&display)
            .expect("Content-Location のパースは成功するはず (実装バグ)");
        assert_eq!(
            cl.uri().path(),
            reparsed.uri().path(),
            "Display 結果の再パースで path が一致すること",
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

// HTTP/HTTPS URI
#[test]
fn prop_content_location_http_uri() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let secure = noprop::sample_bool(ctx);
        let host = hostname(ctx);
        let p = path(ctx);
        let scheme = if secure { "https" } else { "http" };
        let uri = format!("{}://{}{}", scheme, host, p);
        let cl = ContentLocation::parse(&uri)
            .expect("Content-Location のパースは成功するはず (実装バグ)");

        assert!(
            cl.uri().as_str().contains(&host),
            "URI にホスト名が含まれること",
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

// IPv4 ホスト
#[test]
fn prop_content_location_ipv4_host() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let addr = ipv4(ctx);
        let p = path(ctx);
        let uri = format!("http://{}{}", addr, p);
        let cl = ContentLocation::parse(&uri)
            .expect("Content-Location のパースは成功するはず (実装バグ)");

        assert!(
            cl.uri().as_str().contains(&addr),
            "URI に IPv4 アドレスが含まれること",
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
// 相対 URI のテスト
// ========================================

// 相対 URI のラウンドトリップ
#[test]
fn prop_content_location_relative_uri_roundtrip() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let uri = relative_uri(ctx);
        let cl = ContentLocation::parse(&uri)
            .expect("Content-Location のパースは成功するはず (実装バグ)");

        // パスが正しく取得できる
        assert!(cl.uri().path().starts_with('/'), "パスが `/` で始まること",);
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

// パスのみの URI
#[test]
fn prop_content_location_path_only() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let p = path(ctx);
        let cl =
            ContentLocation::parse(&p).expect("Content-Location のパースは成功するはず (実装バグ)");
        assert_eq!(cl.uri().path(), p.as_str(), "パスが入力と一致すること",);
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

// パス + クエリ
#[test]
fn prop_content_location_path_with_query() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let p = path(ctx);
        let q = query_pair(ctx);
        let uri = format!("{}?{}", p, q);
        let cl = ContentLocation::parse(&uri)
            .expect("Content-Location のパースは成功するはず (実装バグ)");

        assert_eq!(cl.uri().path(), p.as_str(), "パスが入力と一致すること",);
        assert_eq!(
            cl.uri().query(),
            Some(q.as_str()),
            "クエリ文字列が入力と一致すること",
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

// フラグメント付き URI は拒否される (RFC 9110 Section 8.7)
#[test]
fn prop_content_location_fragment_rejected() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let p = path(ctx);
        let frag = fragment(ctx);
        let uri = format!("{}{}", p, frag);
        assert!(
            ContentLocation::parse(&uri).is_err(),
            "フラグメント付き URI は拒否されること",
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
// Display のテスト
// ========================================

// Display は元の URI を返す
#[test]
fn prop_content_location_display() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let uri = valid_uri(ctx);
        let cl = ContentLocation::parse(&uri)
            .expect("Content-Location のパースは成功するはず (実装バグ)");
        let display = cl.to_string();

        // Display 結果を再パースできる
        let reparsed = ContentLocation::parse(&display);
        assert!(reparsed.is_ok(), "Display 結果を再パースできること");
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
