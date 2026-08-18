//! ETag のプロパティテスト

use shiguredo_http11::etag::{EntityTag, parse_etag_list};

// ========================================
// Strategy 定義
// ========================================

// 元の戦略 "[a-zA-Z0-9_-]" の 1 文字
fn etag_symbol_char(ctx: &mut noprop::TestCaseContext) -> char {
    match noprop::sample_usize_in(ctx, 0..5) {
        0 => char::from(b'a' + noprop::sample_usize_in(ctx, 0..26) as u8),
        1 => char::from(b'A' + noprop::sample_usize_in(ctx, 0..26) as u8),
        2 => char::from(b'0' + noprop::sample_usize_in(ctx, 0..10) as u8),
        3 => '_',
        _ => '-',
    }
}

// 元の戦略 "[a-zA-Z0-9]" の 1 文字
fn etag_alnum_char(ctx: &mut noprop::TestCaseContext) -> char {
    match noprop::sample_usize_in(ctx, 0..3) {
        0 => char::from(b'a' + noprop::sample_usize_in(ctx, 0..26) as u8),
        1 => char::from(b'A' + noprop::sample_usize_in(ctx, 0..26) as u8),
        _ => char::from(b'0' + noprop::sample_usize_in(ctx, 0..10) as u8),
    }
}

/// 指定長範囲の ETag タグ文字列を生成する
///
/// symbols が true なら `[a-zA-Z0-9_-]`、false なら `[a-zA-Z0-9]` を使う。
fn etag_tag(
    ctx: &mut noprop::TestCaseContext,
    min_len: usize,
    max_len: usize,
    symbols: bool,
) -> String {
    let len = noprop::sample_usize_in(ctx, min_len..=max_len);
    let mut s = String::with_capacity(len);
    for _ in 0..len {
        if symbols {
            s.push(etag_symbol_char(ctx));
        } else {
            s.push(etag_alnum_char(ctx));
        }
    }
    s
}

// ========================================
// ETag パースのテスト
// ========================================

// Strong ETag のラウンドトリップ
#[test]
fn prop_etag_strong_roundtrip() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let tag = etag_tag(ctx, 0, 32, true);
        let etag = EntityTag::strong(&tag).expect("ETag のパースは成功するはず (実装バグ)");
        let displayed = etag.to_string();
        let reparsed =
            EntityTag::parse(&displayed).expect("ETag のパースは成功するはず (実装バグ)");

        assert!(reparsed.is_strong());
        assert_eq!(reparsed.tag(), tag.as_str());
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

// Weak ETag のラウンドトリップ
#[test]
fn prop_etag_weak_roundtrip() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let tag = etag_tag(ctx, 0, 32, true);
        let etag = EntityTag::weak(&tag).expect("ETag のパースは成功するはず (実装バグ)");
        let displayed = etag.to_string();
        let reparsed =
            EntityTag::parse(&displayed).expect("ETag のパースは成功するはず (実装バグ)");

        assert!(reparsed.is_weak());
        assert_eq!(reparsed.tag(), tag.as_str());
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

// Strong 比較の正確性
#[test]
fn prop_etag_strong_compare() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    // 等値側 (両方 strong かつ tag 一致) と非等値側の両方を踏むためのカバレッジゲート。
    // 等値側に到達する確率 p は 1/16 (tag2 を tag1 と同一にする確率 1/4 ×
    // weak1/weak2 が両方 false の確率 1/4) で、256 ケースで全滅する確率は
    // (15/16)^256 ≈ 7e-8。
    let strong_equal = std::cell::Cell::new(0usize);
    let strong_not_equal = std::cell::Cell::new(0usize);

    runner.run(256, |ctx| {
        let tag1 = etag_tag(ctx, 1, 16, false);
        // tag2 は 1/4 の確率で tag1 と同一にし、等値側の比較を確実に踏む
        let tag2 = match noprop::sample_usize_in(ctx, 0..4) {
            0 => tag1.clone(),
            _ => etag_tag(ctx, 1, 16, false),
        };
        let weak1 = noprop::sample_bool(ctx);
        let weak2 = noprop::sample_bool(ctx);

        let e1 = if weak1 {
            EntityTag::weak(&tag1)
        } else {
            EntityTag::strong(&tag1)
        }
        .expect("ETag のパースは成功するはず (実装バグ)");
        let e2 = if weak2 {
            EntityTag::weak(&tag2)
        } else {
            EntityTag::strong(&tag2)
        }
        .expect("ETag のパースは成功するはず (実装バグ)");

        // Strong 比較: 両方 strong で tag が同じ場合のみ true
        let expected = !weak1 && !weak2 && tag1 == tag2;
        assert_eq!(e1.strong_compare(&e2), expected);
        if expected {
            strong_equal.set(strong_equal.get() + 1);
        } else {
            strong_not_equal.set(strong_not_equal.get() + 1);
        }
        Ok(())
    })?;

    // ジェネレータは valid-by-construction であり、ケース棄却が発生しないことの検証
    assert_eq!(
        runner.stats().rejected_cases,
        0,
        "ジェネレータが valid-by-construction であること\n{runner}"
    );
    assert!(
        strong_equal.get() > 0,
        "strong 比較で true になるケースが一度も実行されていない\n{runner}"
    );
    assert!(
        strong_not_equal.get() > 0,
        "strong 比較で false になるケースが一度も実行されていない\n{runner}"
    );
    Ok(())
}

// Weak 比較の正確性
#[test]
fn prop_etag_weak_compare() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    // 等値側 (tag 一致) と非等値側の両方を踏むためのカバレッジゲート。
    // 等値側に到達する確率 p は 1/4 (tag2 を tag1 と同一にする確率) で、
    // 256 ケースで全滅する確率は (3/4)^256 ≈ 4e-33。
    let weak_equal = std::cell::Cell::new(0usize);
    let weak_not_equal = std::cell::Cell::new(0usize);

    runner.run(256, |ctx| {
        let tag1 = etag_tag(ctx, 1, 16, false);
        // tag2 は 1/4 の確率で tag1 と同一にし、等値側の比較を確実に踏む
        let tag2 = match noprop::sample_usize_in(ctx, 0..4) {
            0 => tag1.clone(),
            _ => etag_tag(ctx, 1, 16, false),
        };
        let weak1 = noprop::sample_bool(ctx);
        let weak2 = noprop::sample_bool(ctx);

        let e1 = if weak1 {
            EntityTag::weak(&tag1)
        } else {
            EntityTag::strong(&tag1)
        }
        .expect("ETag のパースは成功するはず (実装バグ)");
        let e2 = if weak2 {
            EntityTag::weak(&tag2)
        } else {
            EntityTag::strong(&tag2)
        }
        .expect("ETag のパースは成功するはず (実装バグ)");

        // Weak 比較: tag が同じ場合は true (weak フラグは無視)
        let expected = tag1 == tag2;
        assert_eq!(e1.weak_compare(&e2), expected);
        if expected {
            weak_equal.set(weak_equal.get() + 1);
        } else {
            weak_not_equal.set(weak_not_equal.get() + 1);
        }
        Ok(())
    })?;

    // ジェネレータは valid-by-construction であり、ケース棄却が発生しないことの検証
    assert_eq!(
        runner.stats().rejected_cases,
        0,
        "ジェネレータが valid-by-construction であること\n{runner}"
    );
    assert!(
        weak_equal.get() > 0,
        "weak 比較で true になるケースが一度も実行されていない\n{runner}"
    );
    assert!(
        weak_not_equal.get() > 0,
        "weak 比較で false になるケースが一度も実行されていない\n{runner}"
    );
    Ok(())
}

// ETag リストのパース
#[test]
fn prop_etag_list_roundtrip() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let tag_count = noprop::sample_usize_in(ctx, 1..5);
        let mut etag_strs = Vec::new();
        for _ in 0..tag_count {
            let tag = etag_tag(ctx, 1, 8, false);
            etag_strs.push(format!("\"{}\"", tag));
        }
        let list_str = etag_strs.join(", ");

        let list = parse_etag_list(&list_str).expect("ETag のパースは成功するはず (実装バグ)");
        let displayed = list.to_string();

        // 再パース
        let reparsed = parse_etag_list(&displayed).expect("ETag のパースは成功するはず (実装バグ)");
        assert_eq!(list, reparsed);
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
