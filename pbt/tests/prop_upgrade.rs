//! Upgrade ヘッダーのプロパティテスト (upgrade.rs)

use shiguredo_http11::upgrade::Upgrade;

// HTTP トークン文字
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

// Upgrade のラウンドトリップ
#[test]
fn prop_upgrade_roundtrip() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let item_count = noprop::sample_usize_in(ctx, 1..4);
        let mut items = Vec::new();
        for _ in 0..item_count {
            let protocol = token_string(ctx, 8);
            // バージョンは半分程度の確率で付与する
            let version = if noprop::sample_bool(ctx) {
                Some(token_string(ctx, 8))
            } else {
                None
            };
            items.push((protocol, version));
        }

        let first_protocol = items[0].0.clone();
        let mut parts = Vec::new();

        for (protocol, version) in items {
            let part = match version {
                Some(version) => format!("{}/{}", protocol, version),
                None => protocol,
            };
            parts.push(part);
        }

        let header = parts.join(", ");
        let parsed = Upgrade::parse(&header).expect("Upgrade のパースは成功するはず (実装バグ)");
        let displayed = parsed.to_string();
        let reparsed =
            Upgrade::parse(&displayed).expect("Upgrade のパースは成功するはず (実装バグ)");
        assert_eq!(&parsed, &reparsed);
        assert!(parsed.has_protocol(&first_protocol));
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
