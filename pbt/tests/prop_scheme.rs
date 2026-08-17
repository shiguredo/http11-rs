//! Scheme 型のプロパティテスト

use pbt::{invalid_scheme, valid_scheme};
use shiguredo_http11::Scheme;

/// valid なバイト列は Scheme::new が Ok を返す
#[test]
fn new_accepts_valid_schemes() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let scheme = valid_scheme(ctx);
        assert!(Scheme::new(&scheme).is_ok());
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

/// invalid なバイト列は Scheme::new が Err を返す
#[test]
fn new_rejects_invalid_schemes() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let scheme = invalid_scheme(ctx);
        assert!(Scheme::new(&scheme).is_err());
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

/// 受理された値の as_bytes() は入力バイト列と一致する（非破壊性）
#[test]
fn as_bytes_returns_original() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let scheme = valid_scheme(ctx);
        let s = Scheme::new(&scheme).expect("スキームのパースは成功するはず (実装バグ)");
        assert_eq!(s.as_bytes(), scheme.as_slice());
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

/// case-insensitive な Eq: 大文字小文字の違いを無視する
#[test]
fn eq_is_case_insensitive() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let scheme = valid_scheme(ctx);
        let lower: Vec<u8> = scheme.iter().map(|b| b.to_ascii_lowercase()).collect();
        let upper: Vec<u8> = scheme.iter().map(|b| b.to_ascii_uppercase()).collect();
        let h1 = Scheme::new(&lower).expect("スキームのパースは成功するはず (実装バグ)");
        let h2 = Scheme::new(&upper).expect("スキームのパースは成功するはず (実装バグ)");
        assert_eq!(h1, h2);
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
