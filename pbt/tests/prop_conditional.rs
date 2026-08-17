//! 条件付きリクエストのプロパティテスト

use shiguredo_http11::conditional::{
    IfMatch, IfModifiedSince, IfNoneMatch, IfRange, IfUnmodifiedSince,
};
use shiguredo_http11::etag::EntityTag;

// ========================================
// ジェネレータ定義
// ========================================

// ETag 値 (有効な文字のみ)
fn etag_value(ctx: &mut noprop::TestCaseContext) -> String {
    let len = noprop::sample_usize_in(ctx, 1..=16);
    let mut s = String::with_capacity(len);
    for _ in 0..len {
        match noprop::sample_usize_in(ctx, 0..5) {
            0 => s.push(char::from(b'a' + noprop::sample_usize_in(ctx, 0..26) as u8)),
            1 => s.push(char::from(b'A' + noprop::sample_usize_in(ctx, 0..26) as u8)),
            2 => s.push(char::from(b'0' + noprop::sample_usize_in(ctx, 0..10) as u8)),
            3 => s.push('-'),
            _ => s.push('_'),
        }
    }
    s
}

// ETag 値のリスト (1..=3 個)
fn etag_value_list(ctx: &mut noprop::TestCaseContext) -> Vec<String> {
    let len = noprop::sample_usize_in(ctx, 1..=3);
    let mut v = Vec::with_capacity(len);
    for _ in 0..len {
        v.push(etag_value(ctx));
    }
    v
}

// HTTP 日付文字列
fn http_date_str(ctx: &mut noprop::TestCaseContext) -> String {
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

    format!(
        "{}, {:02} {} {:04} {:02}:{:02}:{:02} GMT",
        dow, day, mon, year, hour, minute, second
    )
}

// ========================================
// IfMatch のテスト
// ========================================

/// 複数 ETag のラウンドトリップ
#[test]
fn prop_if_match_multiple_roundtrip() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let tags = etag_value_list(ctx);
        let etag_strs: Vec<String> = tags.iter().map(|t| format!("\"{}\"", t)).collect();
        let list_str = etag_strs.join(", ");

        let im =
            IfMatch::parse(&list_str).expect("条件付きリクエストのパースは成功するはず (実装バグ)");
        let displayed = im.to_string();
        let reparsed = IfMatch::parse(&displayed)
            .expect("条件付きリクエストのパースは成功するはず (実装バグ)");

        assert_eq!(im, reparsed);
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

/// matches 動作 (Strong 比較)
#[test]
fn prop_if_match_matches_strong() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let match_cases = std::cell::Cell::new(0usize);
    let no_match_cases = std::cell::Cell::new(0usize);
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let tags = etag_value_list(ctx);
        let etag_strs: Vec<String> = tags.iter().map(|t| format!("\"{}\"", t)).collect();
        let list_str = etag_strs.join(", ");

        let im =
            IfMatch::parse(&list_str).expect("条件付きリクエストのパースは成功するはず (実装バグ)");

        // 一致側を確実に探索するため、1/2 の確率で tags のいずれかを check_tag に使う。
        // 残り 1/2 は独立に生成したランダムなタグを使う。
        let check_tag = if noprop::sample_ratio(ctx, noprop::Ratio::one_nth(2)) {
            tags[noprop::sample_usize_in(ctx, 0..tags.len())].clone()
        } else {
            etag_value(ctx)
        };
        let etag = EntityTag::strong(&check_tag)
            .expect("条件付きリクエストのパースは成功するはず (実装バグ)");

        // Strong 比較なので、tags に check_tag が含まれていれば true
        let expected = tags.contains(&check_tag);
        let actual = im.matches(&etag);
        assert_eq!(actual, expected);

        // 比較の両側 (一致 / 不一致) が実行されたことを検証するゲート
        if actual {
            match_cases.set(match_cases.get() + 1);
        } else {
            no_match_cases.set(no_match_cases.get() + 1);
        }
        Ok(())
    })?;

    // ジェネレータは valid-by-construction であり、ケース棄却が発生しないことの検証
    assert_eq!(
        runner.stats().rejected_cases,
        0,
        "ジェネレータが valid-by-construction であること\n{runner}"
    );
    // 1/2 の確率で tags のいずれかを check_tag に使うため、
    // p(一致) = 1/2、p(不一致) = 1/2 となり、256 ケースでどちらもほぼ確実に実行される
    assert!(
        match_cases.get() > 0,
        "If-Match が一致するケースが 1 回も実行されていない\n{runner}"
    );
    assert!(
        no_match_cases.get() > 0,
        "If-Match が一致しないケースが 1 回も実行されていない\n{runner}"
    );
    Ok(())
}

/// Weak ETag は If-Match では一致しない
#[test]
fn prop_if_match_weak_not_match() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let tag = etag_value(ctx);
        let input = format!("W/\"{}\"", tag);
        let im =
            IfMatch::parse(&input).expect("条件付きリクエストのパースは成功するはず (実装バグ)");
        let strong_etag =
            EntityTag::strong(&tag).expect("条件付きリクエストのパースは成功するはず (実装バグ)");

        // If-Match は Strong 比較を使用するため、Weak ETag は一致しない
        assert!(!im.matches(&strong_etag));
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
// IfNoneMatch のテスト
// ========================================

/// 複数 ETag のラウンドトリップ
#[test]
fn prop_if_none_match_multiple_roundtrip() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let tags = etag_value_list(ctx);
        let etag_strs: Vec<String> = tags.iter().map(|t| format!("\"{}\"", t)).collect();
        let list_str = etag_strs.join(", ");

        let inm = IfNoneMatch::parse(&list_str)
            .expect("条件付きリクエストのパースは成功するはず (実装バグ)");
        let displayed = inm.to_string();
        let reparsed = IfNoneMatch::parse(&displayed)
            .expect("条件付きリクエストのパースは成功するはず (実装バグ)");

        assert_eq!(inm, reparsed);
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

/// matches 動作 (Weak 比較)
#[test]
fn prop_if_none_match_matches_weak() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let match_cases = std::cell::Cell::new(0usize);
    let no_match_cases = std::cell::Cell::new(0usize);
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let tags = etag_value_list(ctx);
        let etag_strs: Vec<String> = tags.iter().map(|t| format!("\"{}\"", t)).collect();
        let list_str = etag_strs.join(", ");

        let inm = IfNoneMatch::parse(&list_str)
            .expect("条件付きリクエストのパースは成功するはず (実装バグ)");

        // 一致側 (tags に含まれる) を確実に探索するため、1/2 の確率で tags の
        // いずれかを check_tag に使う。残り 1/2 は独立に生成したランダムなタグを使う。
        let check_tag = if noprop::sample_ratio(ctx, noprop::Ratio::one_nth(2)) {
            tags[noprop::sample_usize_in(ctx, 0..tags.len())].clone()
        } else {
            etag_value(ctx)
        };
        let etag = EntityTag::strong(&check_tag)
            .expect("条件付きリクエストのパースは成功するはず (実装バグ)");

        // matches が true = 処理すべき = tags に含まれていない
        let expected = !tags.contains(&check_tag);
        let actual = inm.matches(&etag);
        assert_eq!(actual, expected);

        // 比較の両側 (一致 / 不一致) が実行されたことを検証するゲート
        if actual {
            match_cases.set(match_cases.get() + 1);
        } else {
            no_match_cases.set(no_match_cases.get() + 1);
        }
        Ok(())
    })?;

    // ジェネレータは valid-by-construction であり、ケース棄却が発生しないことの検証
    assert_eq!(
        runner.stats().rejected_cases,
        0,
        "ジェネレータが valid-by-construction であること\n{runner}"
    );
    // 1/2 の確率で tags のいずれかを check_tag に使うため、
    // p(一致) = 1/2、p(不一致) = 1/2 となり、256 ケースでどちらもほぼ確実に実行される
    assert!(
        match_cases.get() > 0,
        "If-None-Match が一致するケースが 1 回も実行されていない\n{runner}"
    );
    assert!(
        no_match_cases.get() > 0,
        "If-None-Match が一致しないケースが 1 回も実行されていない\n{runner}"
    );
    Ok(())
}

/// Weak ETag は If-None-Match で一致する
#[test]
fn prop_if_none_match_weak_match() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let tag = etag_value(ctx);
        let input = format!("W/\"{}\"", tag);
        let inm = IfNoneMatch::parse(&input)
            .expect("条件付きリクエストのパースは成功するはず (実装バグ)");
        let strong_etag =
            EntityTag::strong(&tag).expect("条件付きリクエストのパースは成功するはず (実装バグ)");

        // If-None-Match は Weak 比較を使用するため、同じタグなら一致
        // matches が false = 一致するので処理しない
        assert!(!inm.matches(&strong_etag));
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
// IfModifiedSince のテスト
// ========================================

/// ラウンドトリップ
#[test]
fn prop_if_modified_since_roundtrip() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let date_str = http_date_str(ctx);
        let ims = IfModifiedSince::parse(&date_str, 2026)
            .expect("条件付きリクエストのパースは成功するはず (実装バグ)");
        let displayed = ims.to_string();
        let reparsed = IfModifiedSince::parse(&displayed, 2026)
            .expect("条件付きリクエストのパースは成功するはず (実装バグ)");

        assert_eq!(ims.date().day(), reparsed.date().day());
        assert_eq!(ims.date().month(), reparsed.date().month());
        assert_eq!(ims.date().year(), reparsed.date().year());
        assert_eq!(ims.date().hour(), reparsed.date().hour());
        assert_eq!(ims.date().minute(), reparsed.date().minute());
        assert_eq!(ims.date().second(), reparsed.date().second());
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

/// is_modified
#[test]
fn prop_if_modified_since_is_modified() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let date_str = http_date_str(ctx);
        let ims = IfModifiedSince::parse(&date_str, 2026)
            .expect("条件付きリクエストのパースは成功するはず (実装バグ)");
        let same_date = ims.date();

        // 同じ日付なら modified ではない
        assert!(!ims.is_modified(same_date));
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
// IfUnmodifiedSince のテスト
// ========================================

/// ラウンドトリップ
#[test]
fn prop_if_unmodified_since_roundtrip() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let date_str = http_date_str(ctx);
        let ius = IfUnmodifiedSince::parse(&date_str, 2026)
            .expect("条件付きリクエストのパースは成功するはず (実装バグ)");
        let displayed = ius.to_string();
        let reparsed = IfUnmodifiedSince::parse(&displayed, 2026)
            .expect("条件付きリクエストのパースは成功するはず (実装バグ)");

        assert_eq!(ius.date().day(), reparsed.date().day());
        assert_eq!(ius.date().month(), reparsed.date().month());
        assert_eq!(ius.date().year(), reparsed.date().year());
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
// IfRange のテスト
// ========================================

/// ETag ラウンドトリップ (Strong)
#[test]
fn prop_if_range_strong_etag_roundtrip() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let tag = etag_value(ctx);
        let input = format!("\"{}\"", tag);
        let ir = IfRange::parse(&input, 2026)
            .expect("条件付きリクエストのパースは成功するはず (実装バグ)");

        assert!(ir.is_etag());
        assert!(!ir.is_date());
        assert_eq!(
            ir.etag()
                .expect("条件付きリクエストのパースは成功するはず (実装バグ)")
                .tag(),
            tag.as_str()
        );
        assert!(ir.date().is_none());

        let displayed = ir.to_string();
        let reparsed = IfRange::parse(&displayed, 2026)
            .expect("条件付きリクエストのパースは成功するはず (実装バグ)");
        assert!(reparsed.is_etag());
        assert_eq!(
            ir.etag()
                .expect("条件付きリクエストのパースは成功するはず (実装バグ)")
                .tag(),
            reparsed
                .etag()
                .expect("条件付きリクエストのパースは成功するはず (実装バグ)")
                .tag()
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

/// ETag ラウンドトリップ (Weak)
#[test]
fn prop_if_range_weak_etag_roundtrip() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let tag = etag_value(ctx);
        let input = format!("W/\"{}\"", tag);
        let ir = IfRange::parse(&input, 2026)
            .expect("条件付きリクエストのパースは成功するはず (実装バグ)");

        assert!(ir.is_etag());
        assert!(
            ir.etag()
                .expect("条件付きリクエストのパースは成功するはず (実装バグ)")
                .is_weak()
        );
        assert_eq!(
            ir.etag()
                .expect("条件付きリクエストのパースは成功するはず (実装バグ)")
                .tag(),
            tag.as_str()
        );

        let displayed = ir.to_string();
        let reparsed = IfRange::parse(&displayed, 2026)
            .expect("条件付きリクエストのパースは成功するはず (実装バグ)");
        assert!(reparsed.is_etag());
        assert!(
            reparsed
                .etag()
                .expect("条件付きリクエストのパースは成功するはず (実装バグ)")
                .is_weak()
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

/// 日付ラウンドトリップ
#[test]
fn prop_if_range_date_roundtrip() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let date_str = http_date_str(ctx);
        let ir = IfRange::parse(&date_str, 2026)
            .expect("条件付きリクエストのパースは成功するはず (実装バグ)");

        assert!(ir.is_date());
        assert!(!ir.is_etag());
        assert!(ir.etag().is_none());
        assert!(ir.date().is_some());

        let displayed = ir.to_string();
        let reparsed = IfRange::parse(&displayed, 2026)
            .expect("条件付きリクエストのパースは成功するはず (実装バグ)");
        assert!(reparsed.is_date());
        assert_eq!(
            ir.date()
                .expect("条件付きリクエストのパースは成功するはず (実装バグ)")
                .day(),
            reparsed
                .date()
                .expect("条件付きリクエストのパースは成功するはず (実装バグ)")
                .day()
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

/// RFC 9110 Section 8.8.3: W/ は case-sensitive (小文字 w/ は拒否)
#[test]
fn prop_if_range_weak_lowercase_rejected() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let tag = etag_value(ctx);
        let input = format!("w/\"{}\"", tag);
        // 小文字 w/ は RFC 非準拠のため拒否される
        assert!(IfRange::parse(&input, 2026).is_err());
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
