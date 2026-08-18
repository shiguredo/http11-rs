//! Method 型のプロパティテスト

use pbt::{invalid_method, valid_method};
use shiguredo_http11::Method;

/// valid なバイト列は Method::new が Ok を返す
#[test]
fn new_accepts_valid_methods() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let method = valid_method(ctx);
        assert!(Method::new(&method).is_ok());
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

/// invalid なバイト列は Method::new が Err を返す
#[test]
fn new_rejects_invalid_methods() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let method = invalid_method(ctx);
        assert!(Method::new(&method).is_err());
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
        let method = valid_method(ctx);
        let m = Method::new(&method).expect("メソッドのパースは成功するはず (実装バグ)");
        assert_eq!(m.as_bytes(), method.as_slice());
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

/// case-sensitive な Eq: 大文字小文字を区別する
#[test]
fn eq_is_case_sensitive() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let method = valid_method(ctx);
        let lower: Vec<u8> = method.iter().map(|b| b.to_ascii_lowercase()).collect();
        let upper: Vec<u8> = method.iter().map(|b| b.to_ascii_uppercase()).collect();
        let m1 = Method::new(&lower).expect("メソッドのパースは成功するはず (実装バグ)");
        let m2 = Method::new(&upper).expect("メソッドのパースは成功するはず (実装バグ)");

        // 大文字小文字の変換で変化があった場合のみ NE になる
        if lower == upper {
            assert_eq!(m1, m2);
        } else {
            assert_ne!(m1, m2);
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

/// TryFrom<&'static [u8]> と new() の受理集合が一致する
#[test]
fn try_from_static_bytes_acceptance_equals_new() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let method = valid_method(ctx);
        let static_bytes: &'static [u8] = Box::leak(method.clone().into_boxed_slice());
        let r1 = Method::new(&method);
        let r2: Result<Method, _> = static_bytes.try_into();
        assert!(r1.is_ok());
        assert!(r2.is_ok());
        let b1 = r1
            .expect("メソッドのパースは成功するはず (実装バグ)")
            .as_bytes()
            .to_vec();
        let b2 = r2
            .expect("メソッドのパースは成功するはず (実装バグ)")
            .as_bytes()
            .to_vec();
        assert_eq!(b1, b2);
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

/// TryFrom<&'static str> と new() の受理集合が一致する（valid な入力）
#[test]
fn try_from_static_str_acceptance_equals_new() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let method = valid_method(ctx);
        if method.is_empty() {
            return Ok(());
        }
        let method_str = String::from_utf8_lossy(&method).into_owned();
        let static_str: &'static str = Box::leak(method_str.into_boxed_str());
        let r1 = Method::new(&method);
        let r2: Result<Method, _> = static_str.try_into();
        assert!(r1.is_ok());
        assert!(r2.is_ok());
        let b1 = r1
            .expect("メソッドのパースは成功するはず (実装バグ)")
            .as_bytes()
            .to_vec();
        let b2 = r2
            .expect("メソッドのパースは成功するはず (実装バグ)")
            .as_bytes()
            .to_vec();
        assert_eq!(b1, b2);
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
