//! キャッシュヘッダーのプロパティテスト

use shiguredo_http11::cache::{Age, CacheControl, Expires};

// ========================================
// ジェネレータ定義
// ========================================

// 秒数 (0 から 1 年)
fn seconds(ctx: &mut noprop::TestCaseContext) -> u64 {
    noprop::sample_u64_in(ctx, 0..=31_536_000) // 1 年 + 1
}

// ========================================
// CacheControl のテスト
// ========================================

/// 全ディレクティブのラウンドトリップ
#[test]
fn prop_cache_control_all_directives_roundtrip() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        // 空の CacheControl もラウンドトリップ可能
        let mut cc = CacheControl::new();
        if noprop::sample_bool(ctx) {
            cc = cc.with_max_age(seconds(ctx));
        }
        if noprop::sample_bool(ctx) {
            cc = cc.with_s_maxage(seconds(ctx));
        }
        if noprop::sample_bool(ctx) {
            cc = cc.with_no_cache();
        }
        if noprop::sample_bool(ctx) {
            cc = cc.with_no_store();
        }
        if noprop::sample_bool(ctx) {
            cc = cc.with_no_transform();
        }
        if noprop::sample_bool(ctx) {
            cc = cc.with_must_revalidate();
        }
        if noprop::sample_bool(ctx) {
            cc = cc.with_proxy_revalidate();
        }
        if noprop::sample_bool(ctx) {
            cc = cc.with_public();
        }
        if noprop::sample_bool(ctx) {
            cc = cc.with_private();
        }
        if noprop::sample_bool(ctx) {
            cc = cc.with_immutable();
        }

        let header = cc.to_string();
        let reparsed =
            CacheControl::parse(&header).expect("キャッシュ制御のパースは成功するはず (実装バグ)");

        assert_eq!(cc.max_age(), reparsed.max_age());
        assert_eq!(cc.s_maxage(), reparsed.s_maxage());
        assert_eq!(cc.is_no_cache(), reparsed.is_no_cache());
        assert_eq!(cc.is_no_store(), reparsed.is_no_store());
        assert_eq!(cc.is_no_transform(), reparsed.is_no_transform());
        assert_eq!(cc.is_must_revalidate(), reparsed.is_must_revalidate());
        assert_eq!(cc.is_proxy_revalidate(), reparsed.is_proxy_revalidate());
        assert_eq!(cc.is_public(), reparsed.is_public());
        assert_eq!(cc.is_private(), reparsed.is_private());
        assert_eq!(cc.is_immutable(), reparsed.is_immutable());
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

/// is_cacheable の正確性
#[test]
fn prop_cache_control_is_cacheable() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let cacheable_cases = std::cell::Cell::new(0usize);
    let not_cacheable_cases = std::cell::Cell::new(0usize);
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let mut cc = CacheControl::new();
        let has_max_age = noprop::sample_bool(ctx);
        if has_max_age {
            cc = cc.with_max_age(seconds(ctx));
        }
        let has_s_maxage = noprop::sample_bool(ctx);
        if has_s_maxage {
            cc = cc.with_s_maxage(seconds(ctx));
        }
        let has_no_store = noprop::sample_bool(ctx);
        if has_no_store {
            cc = cc.with_no_store();
        }
        let has_public = noprop::sample_bool(ctx);
        if has_public {
            cc = cc.with_public();
        }

        // no-store があれば cacheable ではない
        // そうでなければ public または max-age または s-maxage があれば cacheable
        let expected = !has_no_store && (has_public || has_max_age || has_s_maxage);
        let actual = cc.is_cacheable();
        assert_eq!(actual, expected);

        // 比較の両側 (cacheable / not cacheable) が実行されたことを検証するゲート
        if actual {
            cacheable_cases.set(cacheable_cases.get() + 1);
        } else {
            not_cacheable_cases.set(not_cacheable_cases.get() + 1);
        }
        Ok(())
    })?;

    // ジェネレータは valid-by-construction であり、ケース棄却が発生しないことの検証
    assert_eq!(
        runner.stats().rejected_cases,
        0,
        "ジェネレータが valid-by-construction であること\n{runner}"
    );
    // p(cacheable) = P(no-store なし) * P(public/max-age/s-maxage いずれか)
    //               = 1/2 * (1 - (1/2)^3) = 7/16 ≈ 0.44
    // p(not cacheable) ≈ 0.56 のため、256 ケースでどちらもほぼ確実に実行される
    assert!(
        cacheable_cases.get() > 0,
        "cacheable が true になるケースが 1 回も実行されていない\n{runner}"
    );
    assert!(
        not_cacheable_cases.get() > 0,
        "cacheable が false になるケースが 1 回も実行されていない\n{runner}"
    );
    Ok(())
}

/// 大文字小文字混在
#[test]
fn prop_cache_control_case_insensitive() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let max_age = noprop::sample_u64_in(ctx, 0..86_400);
        let inputs = [
            format!("MAX-AGE={}", max_age),
            format!("Max-Age={}", max_age),
            format!("PUBLIC, max-age={}", max_age),
            format!("public, MAX-AGE={}", max_age),
        ];

        for input in inputs {
            let cc = CacheControl::parse(&input)
                .expect("キャッシュ制御のパースは成功するはず (実装バグ)");
            assert_eq!(cc.max_age(), Some(max_age));
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

/// max-stale 値あり
#[test]
fn prop_cache_control_max_stale_with_value() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let seconds = noprop::sample_u64_in(ctx, 0..86_400);
        let input = format!("max-stale={}", seconds);
        let cc =
            CacheControl::parse(&input).expect("キャッシュ制御のパースは成功するはず (実装バグ)");
        assert_eq!(cc.max_stale(), Some(seconds));
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

/// min-fresh
#[test]
fn prop_cache_control_min_fresh() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let seconds = noprop::sample_u64_in(ctx, 0..86_400);
        let input = format!("min-fresh={}", seconds);
        let cc =
            CacheControl::parse(&input).expect("キャッシュ制御のパースは成功するはず (実装バグ)");
        assert_eq!(cc.min_fresh(), Some(seconds));
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

/// stale-while-revalidate
#[test]
fn prop_cache_control_stale_while_revalidate() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let seconds = noprop::sample_u64_in(ctx, 0..86_400);
        let input = format!("stale-while-revalidate={}", seconds);
        let cc =
            CacheControl::parse(&input).expect("キャッシュ制御のパースは成功するはず (実装バグ)");
        assert_eq!(cc.stale_while_revalidate(), Some(seconds));
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

/// stale-if-error
#[test]
fn prop_cache_control_stale_if_error() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let seconds = noprop::sample_u64_in(ctx, 0..86_400);
        let input = format!("stale-if-error={}", seconds);
        let cc =
            CacheControl::parse(&input).expect("キャッシュ制御のパースは成功するはず (実装バグ)");
        assert_eq!(cc.stale_if_error(), Some(seconds));
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
// Age のテスト
// ========================================

/// Age ラウンドトリップ
#[test]
fn prop_age_roundtrip() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let secs = seconds(ctx);
        let age = Age::new(secs);
        let header = age.to_string();
        let reparsed =
            Age::parse(&header).expect("キャッシュ制御のパースは成功するはず (実装バグ)");

        assert_eq!(age.seconds(), reparsed.seconds());
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
// Expires のテスト
// ========================================

/// Expires ラウンドトリップ
#[test]
fn prop_expires_roundtrip() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let day = noprop::sample_usize_in(ctx, 1..=28) as u8;
        let month = noprop::sample_usize_in(ctx, 1..=12) as u8;
        let year = noprop::sample_usize_in(ctx, 1990..=2100) as u16;
        let hour = noprop::sample_usize_in(ctx, 0..=23) as u8;
        let minute = noprop::sample_usize_in(ctx, 0..=59) as u8;
        let second = noprop::sample_usize_in(ctx, 0..=59) as u8;
        let dow_names = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];
        let month_names = [
            "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
        ];
        let dow_idx = ((day as usize) + (month as usize) + (year as usize)) % 7;
        let dow = dow_names[dow_idx];
        let mon = month_names[(month - 1) as usize];

        let date_str = format!(
            "{}, {:02} {} {:04} {:02}:{:02}:{:02} GMT",
            dow, day, mon, year, hour, minute, second
        );

        let expires = Expires::parse(&date_str, 2026)
            .expect("キャッシュ制御のパースは成功するはず (実装バグ)");
        let displayed = expires.to_string();
        let reparsed = Expires::parse(&displayed, 2026)
            .expect("キャッシュ制御のパースは成功するはず (実装バグ)");

        assert_eq!(expires.date().day(), reparsed.date().day());
        assert_eq!(expires.date().month(), reparsed.date().month());
        assert_eq!(expires.date().year(), reparsed.date().year());
        assert_eq!(expires.date().hour(), reparsed.date().hour());
        assert_eq!(expires.date().minute(), reparsed.date().minute());
        assert_eq!(expires.date().second(), reparsed.date().second());
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
