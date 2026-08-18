//! Cookie のプロパティテスト (cookie.rs)

use shiguredo_http11::cookie::{Cookie, SameSite, SetCookie};

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

// [a-zA-Z0-9_-] の 1 文字
fn name_value_char(ctx: &mut noprop::TestCaseContext) -> char {
    match noprop::sample_usize_in(ctx, 0..5) {
        0 => char::from(b'a' + noprop::sample_usize_in(ctx, 0..26) as u8),
        1 => char::from(b'A' + noprop::sample_usize_in(ctx, 0..26) as u8),
        2 => char::from(b'0' + noprop::sample_usize_in(ctx, 0..10) as u8),
        3 => '_',
        _ => '-',
    }
}

// [a-zA-Z0-9] の 1 文字
fn alnum_char(ctx: &mut noprop::TestCaseContext) -> char {
    match noprop::sample_usize_in(ctx, 0..3) {
        0 => char::from(b'a' + noprop::sample_usize_in(ctx, 0..26) as u8),
        1 => char::from(b'A' + noprop::sample_usize_in(ctx, 0..26) as u8),
        _ => char::from(b'0' + noprop::sample_usize_in(ctx, 0..10) as u8),
    }
}

/// 元の戦略: "[a-zA-Z][a-zA-Z0-9_-]{0,15}"
fn cookie_name(ctx: &mut noprop::TestCaseContext) -> String {
    let mut s = String::with_capacity(16);
    s.push(alpha_char(ctx));
    let rest_len = noprop::sample_usize_in(ctx, 0..=15);
    for _ in 0..rest_len {
        s.push(name_value_char(ctx));
    }
    s
}

/// 元の戦略: "[a-zA-Z0-9_-]{0,32}"
fn cookie_value(ctx: &mut noprop::TestCaseContext) -> String {
    let len = noprop::sample_usize_in(ctx, 0..=32);
    let mut s = String::with_capacity(len);
    for _ in 0..len {
        s.push(name_value_char(ctx));
    }
    s
}

/// 元の戦略: "[a-zA-Z][a-zA-Z0-9]{0,7}"
fn cookie_name_alnum(ctx: &mut noprop::TestCaseContext) -> String {
    let mut s = String::with_capacity(8);
    s.push(alpha_char(ctx));
    let rest_len = noprop::sample_usize_in(ctx, 0..=7);
    for _ in 0..rest_len {
        s.push(alnum_char(ctx));
    }
    s
}

/// 元の戦略: "[a-zA-Z0-9]{0,16}"
fn cookie_value_alnum(ctx: &mut noprop::TestCaseContext) -> String {
    let len = noprop::sample_usize_in(ctx, 0..=16);
    let mut s = String::with_capacity(len);
    for _ in 0..len {
        s.push(alnum_char(ctx));
    }
    s
}

/// 元の戦略: "/[a-zA-Z0-9_-]{0,16}"
fn cookie_path(ctx: &mut noprop::TestCaseContext) -> String {
    let len = noprop::sample_usize_in(ctx, 0..=16);
    let mut s = String::with_capacity(len + 1);
    s.push('/');
    for _ in 0..len {
        s.push(name_value_char(ctx));
    }
    s
}

/// 元の戦略: "[a-z]{1,8}\.[a-z]{2,4}"
fn cookie_domain(ctx: &mut noprop::TestCaseContext) -> String {
    let first_label_len = noprop::sample_usize_in(ctx, 1..=8);
    let second_label_len = noprop::sample_usize_in(ctx, 2..=4);
    let mut s = String::with_capacity(first_label_len + 1 + second_label_len);
    for _ in 0..first_label_len {
        s.push(char::from(b'a' + noprop::sample_usize_in(ctx, 0..26) as u8));
    }
    s.push('.');
    for _ in 0..second_label_len {
        s.push(char::from(b'a' + noprop::sample_usize_in(ctx, 0..26) as u8));
    }
    s
}

// ========================================
// Cookie パースのテスト
// ========================================

// Cookie のラウンドトリップ
#[test]
fn prop_cookie_roundtrip() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let name = cookie_name(ctx);
        let value = cookie_value(ctx);
        let cookie = Cookie::new(&name, &value).expect("Cookie のパースは成功するはず (実装バグ)");
        let displayed = cookie.to_string();
        let cookies = Cookie::parse(&displayed).expect("Cookie のパースは成功するはず (実装バグ)");
        assert_eq!(cookies.len(), 1);
        assert_eq!(cookies[0].name(), name.as_str());
        assert_eq!(cookies[0].value(), value.as_str());
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

// 複数 Cookie のパース
#[test]
fn prop_cookie_parse_multiple() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let name1 = cookie_name_alnum(ctx);
        let value1 = cookie_value_alnum(ctx);
        let name2 = cookie_name_alnum(ctx);
        let value2 = cookie_value_alnum(ctx);

        let cookie_str = format!("{}={}; {}={}", name1, value1, name2, value2);
        let cookies = Cookie::parse(&cookie_str).expect("Cookie のパースは成功するはず (実装バグ)");
        assert_eq!(cookies.len(), 2);
        assert_eq!(cookies[0].name(), name1.as_str());
        assert_eq!(cookies[0].value(), value1.as_str());
        assert_eq!(cookies[1].name(), name2.as_str());
        assert_eq!(cookies[1].value(), value2.as_str());
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

// SetCookie のラウンドトリップ
#[test]
fn prop_set_cookie_roundtrip() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let name = cookie_name(ctx);
        let value = cookie_value(ctx);
        let cookie =
            SetCookie::new(&name, &value).expect("Cookie のパースは成功するはず (実装バグ)");
        let displayed = cookie.to_string();
        let reparsed =
            SetCookie::parse(&displayed, 2026).expect("Cookie のパースは成功するはず (実装バグ)");
        assert_eq!(reparsed.name(), name.as_str());
        assert_eq!(reparsed.value(), value.as_str());
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

// SetCookie 属性付きラウンドトリップ
#[test]
fn prop_set_cookie_with_attributes() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let name = cookie_name_alnum(ctx);
        let value = cookie_value_alnum(ctx);
        let path = cookie_path(ctx);
        let max_age = noprop::sample_u64_in(ctx, 0..=86400) as i64;

        let cookie = SetCookie::new(&name, &value)
            .expect("Cookie のパースは成功するはず (実装バグ)")
            .with_path(&path)
            .with_max_age(max_age)
            .with_secure(true)
            .with_http_only(true);

        let displayed = cookie.to_string();
        let reparsed =
            SetCookie::parse(&displayed, 2026).expect("Cookie のパースは成功するはず (実装バグ)");

        assert_eq!(reparsed.name(), name.as_str());
        assert_eq!(reparsed.value(), value.as_str());
        assert_eq!(reparsed.path(), Some(path.as_str()));
        assert_eq!(reparsed.max_age(), Some(max_age));
        assert!(reparsed.secure());
        assert!(reparsed.http_only());
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

// SameSite 属性ラウンドトリップ
#[test]
fn prop_set_cookie_same_site() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let name = cookie_name_alnum(ctx);
        let value = cookie_value_alnum(ctx);
        let same_site = match noprop::sample_usize_in(ctx, 0..3) {
            0 => SameSite::Strict,
            1 => SameSite::Lax,
            _ => SameSite::None,
        };

        let cookie = SetCookie::new(&name, &value)
            .expect("Cookie のパースは成功するはず (実装バグ)")
            .with_same_site(same_site);

        let displayed = cookie.to_string();
        let reparsed =
            SetCookie::parse(&displayed, 2026).expect("Cookie のパースは成功するはず (実装バグ)");

        assert_eq!(reparsed.same_site(), Some(same_site));
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

// Domain 属性付き SetCookie
#[test]
fn prop_set_cookie_with_domain() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let name = cookie_name_alnum(ctx);
        let value = cookie_value_alnum(ctx);
        let domain = cookie_domain(ctx);

        let cookie = SetCookie::new(&name, &value)
            .expect("Cookie のパースは成功するはず (実装バグ)")
            .with_domain(&domain);

        let displayed = cookie.to_string();
        let reparsed =
            SetCookie::parse(&displayed, 2026).expect("Cookie のパースは成功するはず (実装バグ)");

        assert_eq!(reparsed.domain(), Some(domain.as_str()));
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
