//! Expect ヘッダーのプロパティテスト (expect.rs)

use pbt::qdtext_value;
use shiguredo_http11::expect::Expect;

// ========================================
// ジェネレータ定義
// ========================================

// トークン文字 (RFC 9110)
fn token_char(ctx: &mut noprop::TestCaseContext) -> char {
    match noprop::sample_usize_in(ctx, 0..18) {
        0 => '!',
        1 => '#',
        2 => '$',
        3 => '%',
        4 => '&',
        5 => '\'',
        6 => '*',
        7 => '+',
        8 => '-',
        9 => '.',
        10 => '^',
        11 => '_',
        12 => '`',
        13 => '|',
        14 => '~',
        // 残り 3 分岐は数字・英大文字・英小文字の範囲
        _ => {
            let idx = noprop::sample_usize_in(ctx, 0..62);
            if idx < 10 {
                char::from(b'0' + idx as u8)
            } else if idx < 36 {
                char::from(b'A' + (idx - 10) as u8)
            } else {
                char::from(b'a' + (idx - 36) as u8)
            }
        }
    }
}

// トークン
fn token(ctx: &mut noprop::TestCaseContext) -> String {
    let len = noprop::sample_usize_in(ctx, 1..=16);
    let mut s = String::with_capacity(len);
    for _ in 0..len {
        s.push(token_char(ctx));
    }
    s
}

// 引用符不要の値 (トークン)
fn token_value(ctx: &mut noprop::TestCaseContext) -> String {
    token(ctx)
}

// 引用符付き文字列の中身 (qdtext + quoted-pair)
fn quoted_string_char(ctx: &mut noprop::TestCaseContext) -> char {
    match noprop::sample_usize_in(ctx, 0..6) {
        0 => '\t',
        1 => ' ',
        2 => '!',
        // 0x23-0x5B (# から [) ただし \ を除く
        3 => {
            let idx = noprop::sample_usize_in(ctx, 0..57);
            char::from_u32(0x23 + idx as u32)
                .expect("ASCII 範囲内なので変換は必ず成功するはず (実装バグ)")
        }
        // \ (エスケープ対象)
        4 => '\\',
        // 0x5D-0x7E (] から ~)
        5 => {
            let idx = noprop::sample_usize_in(ctx, 0..34);
            char::from_u32(0x5D + idx as u32)
                .expect("ASCII 範囲内なので変換は必ず成功するはず (実装バグ)")
        }
        _ => unreachable!("分岐は 6 本なので (実装バグ)"),
    }
}

// 引用符付き文字列の中身
fn quoted_string_content(ctx: &mut noprop::TestCaseContext) -> String {
    let len = noprop::sample_usize_in(ctx, 0..=16);
    let mut s = String::with_capacity(len);
    for _ in 0..len {
        s.push(quoted_string_char(ctx));
    }
    s
}

// quoted-string 用エスケープ (\ → \\, " → \")
fn escape_for_quoted_string(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"")
}

// ========================================
// トークン=値 形式のテスト
// ========================================

/// トークン=値 形式のラウンドトリップ
#[test]
fn prop_expect_token_value_roundtrip() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let t = token(ctx);
        let v = token_value(ctx);
        let input = format!("{}={}", t, v);
        let expect = Expect::parse(&input).expect("Expect のパースは成功するはず (実装バグ)");

        assert_eq!(expect.items().len(), 1);
        assert_eq!(expect.items()[0].token(), t.to_ascii_lowercase());
        assert_eq!(expect.items()[0].value(), Some(v.as_str()));

        // ラウンドトリップ
        let displayed = expect.to_string();
        let reparsed = Expect::parse(&displayed).expect("Expect のパースは成功するはず (実装バグ)");
        assert_eq!(expect, reparsed);
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

/// 引用符付き値のラウンドトリップ
#[test]
fn prop_expect_quoted_value_roundtrip() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let t = token(ctx);
        let v = quoted_string_content(ctx);
        let escaped = escape_for_quoted_string(&v);
        let input = format!("{}=\"{}\"", t, escaped);
        let expect = Expect::parse(&input).expect("Expect のパースは成功するはず (実装バグ)");

        assert_eq!(expect.items().len(), 1);
        assert_eq!(expect.items()[0].token(), t.to_ascii_lowercase());
        assert_eq!(expect.items()[0].value(), Some(v.as_str()));

        // ラウンドトリップ
        let displayed = expect.to_string();
        let reparsed = Expect::parse(&displayed).expect("Expect のパースは成功するはず (実装バグ)");
        assert_eq!(expect, reparsed);
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
// 複数 expectation のテスト
// ========================================

/// 複数 expectation のラウンドトリップ
#[test]
fn prop_expect_multiple_items() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let t1 = token(ctx);
        let v1 = token_value(ctx);
        let t2 = token(ctx);
        let input = format!("{}={}, {}", t1, v1, t2);
        let expect = Expect::parse(&input).expect("Expect のパースは成功するはず (実装バグ)");

        assert_eq!(expect.items().len(), 2);
        assert_eq!(expect.items()[0].token(), t1.to_ascii_lowercase());
        assert_eq!(expect.items()[0].value(), Some(v1.as_str()));
        assert_eq!(expect.items()[1].token(), t2.to_ascii_lowercase());
        assert_eq!(expect.items()[1].value(), None);

        // ラウンドトリップ
        let displayed = expect.to_string();
        let reparsed = Expect::parse(&displayed).expect("Expect のパースは成功するはず (実装バグ)");
        assert_eq!(expect, reparsed);
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

/// 100-continue を含む複数 expectation
#[test]
fn prop_expect_with_100_continue() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let t = token(ctx);
        let v = token_value(ctx);
        let input = format!("{}={}, 100-continue", t, v);
        let expect = Expect::parse(&input).expect("Expect のパースは成功するはず (実装バグ)");

        assert!(expect.has_100_continue());
        assert_eq!(expect.items().len(), 2);
        assert!(!expect.items()[0].is_100_continue());
        assert!(expect.items()[1].is_100_continue());

        // ラウンドトリップ
        let displayed = expect.to_string();
        let reparsed = Expect::parse(&displayed).expect("Expect のパースは成功するはず (実装バグ)");
        assert_eq!(expect, reparsed);
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
// 複合テスト
// ========================================

/// 引用符付き値 + トークン値 + 100-continue の複合ラウンドトリップ
#[test]
fn prop_expect_complex_roundtrip() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let t1 = token(ctx);
        let v1 = quoted_string_content(ctx);
        let t2 = token(ctx);
        let v2 = token_value(ctx);

        // 引用符付き値 + トークン値 + 100-continue
        let escaped_v1 = escape_for_quoted_string(&v1);
        let input = format!("{}=\"{}\", {}={}, 100-continue", t1, escaped_v1, t2, v2);
        let expect = Expect::parse(&input).expect("Expect のパースは成功するはず (実装バグ)");

        assert_eq!(expect.items().len(), 3);
        assert!(expect.has_100_continue());

        // ラウンドトリップ
        let displayed = expect.to_string();
        let reparsed = Expect::parse(&displayed).expect("Expect のパースは成功するはず (実装バグ)");
        assert_eq!(expect, reparsed);
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
// obs-text 含む quoted-string の PBT
// ========================================

// qdtext (obs-text を含む) を quoted value として往復できる
#[test]
fn prop_expect_quoted_obs_text_roundtrip() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let t = token(ctx);
        let value = qdtext_value(ctx, 0..=16);
        let header = format!("{}=\"{}\"", t, value);
        let expect = Expect::parse(&header).expect("Expect のパースは成功するはず (実装バグ)");
        let item = &expect.items()[0];
        assert_eq!(item.value(), Some(value.as_str()));

        // Display 出力は obs-text / 制御文字以外をエスケープしないため、
        // value がそのまま埋め込まれる。直接 assert で実体化する。
        let displayed = expect.to_string();
        assert!(
            displayed.contains(&value) || (value.is_empty() && displayed.contains("=\"\"")),
            "Display 出力 {displayed:?} に value {value:?} が含まれない"
        );
        let reparsed = Expect::parse(&displayed).expect("Expect のパースは成功するはず (実装バグ)");
        assert_eq!(reparsed.items()[0].value(), Some(value.as_str()));
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
