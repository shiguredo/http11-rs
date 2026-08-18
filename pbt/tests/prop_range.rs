//! Range 関連のプロパティテスト

use shiguredo_http11::range::{ContentRange, Range, RangeSpec};

// ========================================
// RangeSpec のテスト
// ========================================

// RangeSpec::Range の Display
#[test]
fn prop_range_spec_range_display() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let mut start = noprop::sample_u64_in(ctx, 0..10000);
        let mut end = noprop::sample_u64_in(ctx, 0..10000);
        if start > end {
            core::mem::swap(&mut start, &mut end);
        }
        let spec = RangeSpec::Range { start, end };
        let display = spec.to_string();

        assert!(display.contains('-'), "Display に `-` が含まれること");
        let expected = format!("{}-{}", start, end);
        assert_eq!(display, expected, "Display が `start-end` 形式であること");
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

// RangeSpec::FromStart の Display
#[test]
fn prop_range_spec_from_start_display() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let start = noprop::sample_u64_in(ctx, 0..10000);
        let spec = RangeSpec::FromStart { start };
        let display = spec.to_string();

        assert!(display.ends_with('-'), "Display が `-` で終わること");
        let expected = format!("{}-", start);
        assert_eq!(display, expected, "Display が `start-` 形式であること");
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

// RangeSpec::Suffix の Display
#[test]
fn prop_range_spec_suffix_display() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let length = noprop::sample_u64_in(ctx, 1..10000);
        let spec = RangeSpec::Suffix { length };
        let display = spec.to_string();

        assert!(display.starts_with('-'), "Display が `-` で始まること");
        let expected = format!("-{}", length);
        assert_eq!(display, expected, "Display が `-length` 形式であること");
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

// RangeSpec::Range の to_bounds
#[test]
fn prop_range_spec_range_to_bounds() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);
    let bounds_reached = std::cell::Cell::new(0usize);

    runner.run(256, |ctx| {
        let mut start = noprop::sample_u64_in(ctx, 0..1000);
        let mut end = noprop::sample_u64_in(ctx, 0..1000);
        if start > end {
            core::mem::swap(&mut start, &mut end);
        }
        let total = noprop::sample_u64_in(ctx, 1..2000);
        let spec = RangeSpec::Range { start, end };

        if let Some((s, e)) = spec.to_bounds(total) {
            // to_bounds が Some を返すときのみ到達するため、
            // カバレッジゲートで Some 分岐が実際に実行されたことを検証する
            // (start >= total の場合は to_bounds が None を返し、この分岐はスキップされる)。
            // start は 0..1000、total は 1..2000 の一様分布なので
            // Some 分岐に到達する確率 p ≈ 0.75。
            bounds_reached.set(bounds_reached.get() + 1);
            assert!(s <= e, "開始位置が終了位置以下であること");
            assert!(e < total, "終了位置が total 未満であること");
            assert_eq!(s, start, "開始位置が入力と一致すること");
        }
        Ok(())
    })?;

    // to_bounds が常に None を返す実装バグを検出するためのカバレッジゲート
    assert!(
        bounds_reached.get() > 0,
        "to_bounds が Some を返す分岐が一度も実行されていない\n{runner}"
    );
    // ジェネレータは valid-by-construction であり、ケース棄却が発生しないことの検証
    assert_eq!(
        runner.stats().rejected_cases,
        0,
        "ジェネレータが valid-by-construction であること\n{runner}"
    );
    Ok(())
}

// RangeSpec::FromStart の to_bounds
#[test]
fn prop_range_spec_from_start_to_bounds() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let start = noprop::sample_u64_in(ctx, 0..1000);
        let total = noprop::sample_u64_in(ctx, 1..2000);
        let spec = RangeSpec::FromStart { start };

        if start < total {
            let bounds = spec.to_bounds(total);
            assert!(
                bounds.is_some(),
                "start < total なら to_bounds が Some を返すこと"
            );
            let (s, e) = bounds.expect("Range のパースは成功するはず (実装バグ)");
            assert_eq!(s, start, "開始位置が入力と一致すること");
            assert_eq!(e, total - 1, "終了位置が total-1 であること");
        } else {
            assert!(
                spec.to_bounds(total).is_none(),
                "start >= total なら to_bounds が None を返すこと",
            );
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

// RangeSpec::Suffix の to_bounds
#[test]
fn prop_range_spec_suffix_to_bounds() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let length = noprop::sample_u64_in(ctx, 1..1000);
        let total = noprop::sample_u64_in(ctx, 1..2000);
        let spec = RangeSpec::Suffix { length };
        let bounds = spec.to_bounds(total);

        assert!(
            bounds.is_some(),
            "length >= 1 なら to_bounds が Some を返すこと"
        );
        let (s, e) = bounds.expect("Range のパースは成功するはず (実装バグ)");
        assert!(s <= e, "開始位置が終了位置以下であること");
        assert_eq!(e, total - 1, "終了位置が total-1 であること");
        // 長さがトータルを超える場合は 0 から開始
        if length >= total {
            assert_eq!(s, 0, "length >= total なら開始位置が 0 であること");
        } else {
            assert_eq!(s, total - length, "開始位置が total-length であること");
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

// total_length=0 のケース
#[test]
fn prop_range_spec_to_bounds_zero_total() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let mut start = noprop::sample_u64_in(ctx, 0..1000);
        let mut end = noprop::sample_u64_in(ctx, 0..1000);
        if start > end {
            core::mem::swap(&mut start, &mut end);
        }

        let spec1 = RangeSpec::Range { start, end };
        let spec2 = RangeSpec::FromStart { start };
        let spec3 = RangeSpec::Suffix { length: 100 };

        assert!(
            spec1.to_bounds(0).is_none(),
            "total=0 なら Range は None を返すこと",
        );
        assert!(
            spec2.to_bounds(0).is_none(),
            "total=0 なら FromStart は None を返すこと",
        );
        assert!(
            spec3.to_bounds(0).is_none(),
            "total=0 なら Suffix は None を返すこと",
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
// Range のテスト
// ========================================

// Range ヘッダーラウンドトリップ
#[test]
fn prop_range_roundtrip() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        // start <= end のみ有効
        let mut start = noprop::sample_u64_in(ctx, 0..10000);
        let mut end = noprop::sample_u64_in(ctx, 0..10000);
        if start > end {
            core::mem::swap(&mut start, &mut end);
        }

        let input = format!("bytes={}-{}", start, end);
        let range = Range::parse(&input).expect("Range のパースは成功するはず (実装バグ)");
        let displayed = range.to_string();
        let reparsed = Range::parse(&displayed).expect("Range のパースは成功するはず (実装バグ)");

        assert_eq!(range.unit(), reparsed.unit(), "単位が一致すること");
        assert_eq!(
            range.ranges().len(),
            reparsed.ranges().len(),
            "範囲数が一致すること",
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

// Range suffix ラウンドトリップ
#[test]
fn prop_range_suffix_roundtrip() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let length = noprop::sample_u64_in(ctx, 1..10000);
        let input = format!("bytes=-{}", length);
        let range = Range::parse(&input).expect("Range のパースは成功するはず (実装バグ)");

        match range
            .first()
            .expect("Range のパースは成功するはず (実装バグ)")
        {
            RangeSpec::Suffix { length: l } => {
                assert_eq!(*l, length, "Suffix の長さが一致すること")
            }
            _ => panic!("Suffix を期待"),
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

// Range from-start ラウンドトリップ
#[test]
fn prop_range_from_start_roundtrip() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let start = noprop::sample_u64_in(ctx, 0..10000);
        let input = format!("bytes={}-", start);
        let range = Range::parse(&input).expect("Range のパースは成功するはず (実装バグ)");

        match range
            .first()
            .expect("Range のパースは成功するはず (実装バグ)")
        {
            RangeSpec::FromStart { start: s } => {
                assert_eq!(*s, start, "FromStart の開始位置が一致すること")
            }
            _ => panic!("FromStart を期待"),
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

// Content-Range ラウンドトリップ
#[test]
fn prop_content_range_roundtrip() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let mut start = noprop::sample_u64(ctx);
        let mut end = noprop::sample_u64(ctx);
        if start > end {
            core::mem::swap(&mut start, &mut end);
        }
        let total = noprop::sample_u64_in(ctx, 1..=20000);
        // end = u64::MAX のとき complete_length を表現できないため None にする
        let complete_length = end.checked_add(1).map(|min| total.max(min));

        let cr = ContentRange::new_bytes(start, end, complete_length);
        let displayed = cr.to_string();
        let reparsed =
            ContentRange::parse(&displayed).expect("Range のパースは成功するはず (実装バグ)");

        assert_eq!(cr.start(), reparsed.start(), "開始位置が一致すること");
        assert_eq!(cr.end(), reparsed.end(), "終了位置が一致すること");
        assert_eq!(
            cr.complete_length(),
            reparsed.complete_length(),
            "完全な長さが一致すること",
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

// Range::is_bytes のテスト
#[test]
fn prop_range_is_bytes() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let mut start = noprop::sample_u64_in(ctx, 0..1000);
        let mut end = noprop::sample_u64_in(ctx, 0..1000);
        if start > end {
            core::mem::swap(&mut start, &mut end);
        }

        // bytes の場合
        let input = format!("bytes={}-{}", start, end);
        let range = Range::parse(&input).expect("Range のパースは成功するはず (実装バグ)");
        assert!(range.is_bytes(), "bytes は is_bytes() が true になること");

        // BYTES (大文字) の場合も true
        let input2 = format!("BYTES={}-{}", start, end);
        let range2 = Range::parse(&input2).expect("Range のパースは成功するはず (実装バグ)");
        assert!(
            range2.is_bytes(),
            "大文字 BYTES も is_bytes() が true になること"
        );

        // 他の単位の場合は false
        let input3 = format!("custom={}-{}", start, end);
        let range3 = Range::parse(&input3).expect("Range のパースは成功するはず (実装バグ)");
        assert!(
            !range3.is_bytes(),
            "他の単位は is_bytes() が false になること"
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

// Range::first のテスト
#[test]
fn prop_range_first() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let mut start = noprop::sample_u64_in(ctx, 0..1000);
        let mut end = noprop::sample_u64_in(ctx, 0..1000);
        if start > end {
            core::mem::swap(&mut start, &mut end);
        }

        let input = format!("bytes={}-{}", start, end);
        let range = Range::parse(&input).expect("Range のパースは成功するはず (実装バグ)");

        assert!(range.first().is_some(), "first() が Some を返すこと");
        match range
            .first()
            .expect("Range のパースは成功するはず (実装バグ)")
        {
            RangeSpec::Range { start: s, end: e } => {
                assert_eq!(*s, start, "開始位置が一致すること");
                assert_eq!(*e, end, "終了位置が一致すること");
            }
            _ => panic!("Range を期待"),
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

// 複数範囲のテスト
#[test]
fn prop_range_multiple_ranges() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let mut start1 = noprop::sample_u64_in(ctx, 0..1000);
        let mut end1 = noprop::sample_u64_in(ctx, 0..1000);
        if start1 > end1 {
            core::mem::swap(&mut start1, &mut end1);
        }
        let mut start2 = noprop::sample_u64_in(ctx, 0..1000);
        let mut end2 = noprop::sample_u64_in(ctx, 0..1000);
        if start2 > end2 {
            core::mem::swap(&mut start2, &mut end2);
        }

        let input = format!("bytes={}-{}, {}-{}", start1, end1, start2, end2);
        let range = Range::parse(&input).expect("Range のパースは成功するはず (実装バグ)");

        assert_eq!(range.ranges().len(), 2, "範囲数が 2 であること");
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
// ContentRange のテスト
// ========================================

// ContentRange::length のテスト
#[test]
fn prop_content_range_length() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let mut start = noprop::sample_u64(ctx);
        let mut end = noprop::sample_u64(ctx);
        if start > end {
            core::mem::swap(&mut start, &mut end);
        }

        let complete_length = end.checked_add(100);
        let cr = ContentRange::new_bytes(start, end, complete_length);

        let expected = end.checked_sub(start).and_then(|d| d.checked_add(1));
        assert_eq!(
            cr.length(),
            expected,
            "length が end-start+1 と一致すること"
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

// ContentRange::is_unsatisfied のテスト
#[test]
fn prop_content_range_is_unsatisfied() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let total = noprop::sample_u64_in(ctx, 100..10000);

        // 満たせる場合
        let cr = ContentRange::new_bytes(0, 99, Some(total));
        assert!(
            !cr.is_unsatisfied(),
            "満たせる範囲は is_unsatisfied() が false になること"
        );

        // 満たせない場合
        let cr_unsatisfied = ContentRange::unsatisfied("bytes", total);
        assert!(
            cr_unsatisfied.is_unsatisfied(),
            "満たせない範囲は is_unsatisfied() が true になること",
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

// ContentRange::unsatisfied のテスト
#[test]
fn prop_content_range_unsatisfied() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let total = noprop::sample_u64_in(ctx, 100..10000);
        let cr = ContentRange::unsatisfied("bytes", total);

        assert_eq!(cr.unit(), "bytes", "単位が bytes であること");
        assert!(cr.start().is_none(), "開始位置がないこと");
        assert!(cr.end().is_none(), "終了位置がないこと");
        assert_eq!(
            cr.complete_length(),
            Some(total),
            "完全な長さが一致すること"
        );
        assert!(cr.is_unsatisfied(), "is_unsatisfied() が true になること");
        assert!(cr.length().is_none(), "length() が None になること");
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

// ContentRange Display ラウンドトリップ (unsatisfied)
#[test]
fn prop_content_range_unsatisfied_display_roundtrip() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let total = noprop::sample_u64_in(ctx, 100..10000);
        let cr = ContentRange::unsatisfied("bytes", total);
        let displayed = cr.to_string();
        let reparsed =
            ContentRange::parse(&displayed).expect("Range のパースは成功するはず (実装バグ)");

        assert!(
            reparsed.is_unsatisfied(),
            "再パース結果が unsatisfied であること"
        );
        assert_eq!(
            reparsed.complete_length(),
            Some(total),
            "完全な長さが一致すること"
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

// ContentRange パース (不明な長さ)
#[test]
fn prop_content_range_unknown_length() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let mut start = noprop::sample_u64(ctx);
        let mut end = noprop::sample_u64(ctx);
        if start > end {
            core::mem::swap(&mut start, &mut end);
        }

        let input = format!("bytes {}-{}/*", start, end);
        let cr = ContentRange::parse(&input).expect("Range のパースは成功するはず (実装バグ)");

        assert_eq!(cr.start(), Some(start), "開始位置が一致すること");
        assert_eq!(cr.end(), Some(end), "終了位置が一致すること");
        assert!(cr.complete_length().is_none(), "完全な長さがないこと");
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
