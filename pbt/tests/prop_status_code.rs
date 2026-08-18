//! StatusClass / StatusCode の Property-Based Test
//!
//! `StatusClass::from_status_code` のパーティション性と、
//! `StatusCode::class` が `from_status_code` と整合することを検証する。

use shiguredo_http11::{StatusClass, StatusCode};

/// 任意の u16 に対する from_status_code のパーティション性
#[test]
fn prop_status_class_partition() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let classified = std::cell::Cell::new(0usize);
    let unclassified = std::cell::Cell::new(0usize);
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        // 各クラス境界 (100/199/200/...) と範囲外境界 (0/99/600/65535) を
        // 1/5 の確率で選びつつ、残りは u16 一様にサンプリングする
        let code = noprop::sample_with_boundaries(
            ctx,
            &[
                100u16, 199, 200, 299, 300, 399, 400, 499, 500, 599, 0, 99, 600, 65535,
            ],
            noprop::Ratio::one_nth(5),
            |ctx| noprop::sample_u16(ctx),
        );

        match StatusClass::from_status_code(code) {
            Some(StatusClass::Informational) => {
                assert!((100..=199).contains(&code));
                classified.set(classified.get() + 1);
            }
            Some(StatusClass::Successful) => {
                assert!((200..=299).contains(&code));
                classified.set(classified.get() + 1);
            }
            Some(StatusClass::Redirection) => {
                assert!((300..=399).contains(&code));
                classified.set(classified.get() + 1);
            }
            Some(StatusClass::ClientError) => {
                assert!((400..=499).contains(&code));
                classified.set(classified.get() + 1);
            }
            Some(StatusClass::ServerError) => {
                assert!((500..=599).contains(&code));
                classified.set(classified.get() + 1);
            }
            None => {
                assert!(!(100..=599).contains(&code));
                unclassified.set(unclassified.get() + 1);
            }
        }
        Ok(())
    })?;

    // 分類境界 10 値と範囲外境界 4 値を 1/5 の確率で選び、残り 4/5 は u16 一様のため、
    // p(分類あり) ≈ 0.15、p(分類なし) ≈ 0.85 でどちらもほぼ確実に実行される
    assert!(
        classified.get() > 0,
        "分類済みコードのアームが 1 回も実行されていない\n{runner}"
    );
    assert!(
        unclassified.get() > 0,
        "未分類コードのアームが 1 回も実行されていない\n{runner}"
    );
    Ok(())
}

/// IANA 登録済み code は必ず class が定まり、from_status_code と一致する
#[test]
fn prop_status_code_class_consistency() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let registered = std::cell::Cell::new(0usize);
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        // 各クラスの先頭 (100/200/300/400/500) は IANA 登録済みのため、
        // 境界を 1/5 の確率で選びつつ、残りは 100..=599 一様にサンプリングする
        let code = noprop::sample_with_boundaries(
            ctx,
            &[100u16, 200, 300, 400, 500, 599],
            noprop::Ratio::one_nth(5),
            |ctx| noprop::sample_usize_in(ctx, 100..=599) as u16,
        );

        if let Some(sc) = StatusCode::from_code(code) {
            let expected = StatusClass::from_status_code(code)
                .expect("100..=599 は常に分類可能なはず (実装バグ)");
            assert_eq!(sc.class(), expected);
            registered.set(registered.get() + 1);
        }
        Ok(())
    })?;

    // 境界 6 値中 5 値が登録済みであり、内部も 500 個中 62 個が登録済み (p ≈ 0.27) のため、
    // 256 ケースで未実行確率は (1 - 0.27)^256 ≈ 0
    assert!(
        registered.get() > 0,
        "IANA 登録済み code の分岐が 1 回も実行されていない\n{runner}"
    );
    Ok(())
}
