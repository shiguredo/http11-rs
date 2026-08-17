//! Vary ヘッダーのプロパティテスト (vary.rs)

use shiguredo_http11::vary::Vary;

// HTTP トークン文字 (RFC 9110)
fn token_char(ctx: &mut noprop::TestCaseContext) -> char {
    match noprop::sample_usize_in(ctx, 0..6) {
        0 => char::from(b'a' + noprop::sample_usize_in(ctx, 0..26) as u8),
        1 => char::from(b'A' + noprop::sample_usize_in(ctx, 0..26) as u8),
        2 => char::from(b'0' + noprop::sample_usize_in(ctx, 0..10) as u8),
        3 => '-',
        4 => '_',
        _ => '.',
    }
}

fn token_string(ctx: &mut noprop::TestCaseContext, max_len: usize) -> String {
    let len = noprop::sample_usize_in(ctx, 1..=max_len);
    let mut s = String::with_capacity(len);
    for _ in 0..len {
        s.push(token_char(ctx));
    }
    s
}

/// Vary のラウンドトリップ
#[test]
fn prop_vary_roundtrip() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let star_cases = std::cell::Cell::new(0usize);
    let token_cases = std::cell::Cell::new(0usize);
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        // "*" (ワイルドカード) とフィールド名リストの 2 分岐を等確率で選ぶ
        let is_star = noprop::sample_bool(ctx);
        let value = if is_star {
            "*".to_string()
        } else {
            let tokens = noprop::sample_usize_in(ctx, 1..=3);
            let mut parts = Vec::with_capacity(tokens);
            for _ in 0..tokens {
                parts.push(token_string(ctx, 8));
            }
            parts.join(", ")
        };

        let parsed = Vary::parse(&value).expect("Vary のパースは成功するはず (実装バグ)");
        let displayed = parsed.to_string();
        let reparsed = Vary::parse(&displayed).expect("Vary のパースは成功するはず (実装バグ)");
        assert_eq!(parsed, reparsed);

        // 不変条件の評価後に分岐種別を記録するゲート
        if is_star {
            star_cases.set(star_cases.get() + 1);
        } else {
            token_cases.set(token_cases.get() + 1);
        }
        Ok(())
    })?;

    // ジェネレータは valid-by-construction であり、ケース棄却が発生しないことの検証
    assert_eq!(
        runner.stats().rejected_cases,
        0,
        "ジェネレータが valid-by-construction であること\n{runner}"
    );
    // "*" とフィールド名リストは各 p = 1/2 で選ばれるため、どちらもほぼ確実に実行される
    assert!(
        star_cases.get() > 0,
        "\"*\" 分岐が 1 回も実行されていない\n{runner}"
    );
    assert!(
        token_cases.get() > 0,
        "フィールド名リスト分岐が 1 回も実行されていない\n{runner}"
    );
    Ok(())
}
