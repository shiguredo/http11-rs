//! Digest Fields のプロパティテスト (RFC 9530)

use shiguredo_http11::digest_fields::{
    ContentDigest, ReprDigest, WantContentDigest, WantReprDigest,
};

// ========================================
// ジェネレータ定義
// ========================================

// 有効なアルゴリズム名
fn valid_algorithm(ctx: &mut noprop::TestCaseContext) -> String {
    noprop::sample_choice(
        ctx,
        &[
            "sha-256",
            "sha-512",
            "sha-384",
            "md5",
            "unixsum",
            "unixcksum",
            "adler32",
            "crc32c",
        ],
    )
    .to_string()
}

// 有効な優先度 (0-10)
fn valid_weight(ctx: &mut noprop::TestCaseContext) -> u8 {
    noprop::sample_usize_in(ctx, 0..=10) as u8
}

// 任意のバイト列 (digest 値)
fn digest_bytes(ctx: &mut noprop::TestCaseContext) -> Vec<u8> {
    let len = noprop::sample_usize_in(ctx, 1..64);
    let mut v = Vec::with_capacity(len);
    for _ in 0..len {
        v.push(noprop::sample_u8(ctx));
    }
    v
}

// Base64 エンコード用の関数 (テスト用)
fn base64_encode(input: &[u8]) -> String {
    const BASE64_ALPHABET: &[u8; 64] =
        b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

    let mut result = String::new();
    let mut i = 0;

    while i < input.len() {
        let b0 = input[i];
        let b1 = if i + 1 < input.len() { input[i + 1] } else { 0 };
        let b2 = if i + 2 < input.len() { input[i + 2] } else { 0 };

        let n = ((b0 as u32) << 16) | ((b1 as u32) << 8) | (b2 as u32);

        result.push(BASE64_ALPHABET[(n >> 18 & 0x3F) as usize] as char);
        result.push(BASE64_ALPHABET[(n >> 12 & 0x3F) as usize] as char);

        if i + 1 < input.len() {
            result.push(BASE64_ALPHABET[(n >> 6 & 0x3F) as usize] as char);
        } else {
            result.push('=');
        }

        if i + 2 < input.len() {
            result.push(BASE64_ALPHABET[(n & 0x3F) as usize] as char);
        } else {
            result.push('=');
        }

        i += 3;
    }

    result
}

// ========================================
// ContentDigest のテスト
// ========================================

// 単一のダイジェストパース
#[test]
fn prop_content_digest_parse_single() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let algorithm = valid_algorithm(ctx);
        let data = digest_bytes(ctx);
        let b64 = base64_encode(&data);
        let input = format!("{}=:{}:", algorithm, b64);

        let digest = ContentDigest::parse(&input)
            .expect("Digest フィールドのパースは成功するはず (実装バグ)");
        assert_eq!(digest.items().len(), 1, "アイテム数が 1 であること");
        assert_eq!(
            digest.items()[0].algorithm(),
            algorithm.as_str(),
            "アルゴリズム名が一致すること",
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

// 複数のダイジェストパース
#[test]
fn prop_content_digest_parse_multiple() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let data1 = digest_bytes(ctx);
        let data2 = digest_bytes(ctx);
        let b64_1 = base64_encode(&data1);
        let b64_2 = base64_encode(&data2);
        let input = format!("sha-256=:{}:, sha-512=:{}:", b64_1, b64_2);

        let digest = ContentDigest::parse(&input)
            .expect("Digest フィールドのパースは成功するはず (実装バグ)");
        assert_eq!(digest.items().len(), 2, "アイテム数が 2 であること");
        assert_eq!(
            digest.items()[0].algorithm(),
            "sha-256",
            "先頭のアルゴリズム名が一致すること",
        );
        assert_eq!(
            digest.items()[1].algorithm(),
            "sha-512",
            "2 番目のアルゴリズム名が一致すること",
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

// get メソッド
#[test]
fn prop_content_digest_get() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let algorithm = valid_algorithm(ctx);
        let data = digest_bytes(ctx);
        let b64 = base64_encode(&data);
        let input = format!("{}=:{}:", algorithm, b64);

        let digest = ContentDigest::parse(&input)
            .expect("Digest フィールドのパースは成功するはず (実装バグ)");

        // 大文字小文字を無視して取得
        assert!(
            digest.get(&algorithm).is_some(),
            "アルゴリズム名で取得できること"
        );
        assert!(
            digest.get(&algorithm.to_uppercase()).is_some(),
            "大文字のアルゴリズム名でも取得できること",
        );

        // 存在しないアルゴリズム
        assert!(
            digest.get("nonexistent").is_none(),
            "存在しないアルゴリズムは None になること",
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

// Display ラウンドトリップ
#[test]
fn prop_content_digest_display_roundtrip() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let data = digest_bytes(ctx);
        let b64 = base64_encode(&data);
        let input = format!("sha-256=:{}:", b64);

        let digest = ContentDigest::parse(&input)
            .expect("Digest フィールドのパースは成功するはず (実装バグ)");
        let display = digest.to_string();

        // 再パース可能
        let reparsed = ContentDigest::parse(&display);
        assert!(reparsed.is_ok(), "再パースは成功するはず (実装バグ)");
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

// DigestValue::bytes
#[test]
fn prop_digest_value_bytes() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let data = digest_bytes(ctx);
        let b64 = base64_encode(&data);
        let input = format!("sha-256=:{}:", b64);

        let digest = ContentDigest::parse(&input)
            .expect("Digest フィールドのパースは成功するはず (実装バグ)");
        let value = digest.items()[0].value();

        assert_eq!(value.bytes(), data.as_slice(), "バイト列が一致すること");
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
// ReprDigest のテスト
// ========================================

// 単一のダイジェストパース
#[test]
fn prop_repr_digest_parse_single() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let algorithm = valid_algorithm(ctx);
        let data = digest_bytes(ctx);
        let b64 = base64_encode(&data);
        let input = format!("{}=:{}:", algorithm, b64);

        let digest =
            ReprDigest::parse(&input).expect("Digest フィールドのパースは成功するはず (実装バグ)");
        assert_eq!(digest.items().len(), 1, "アイテム数が 1 であること");
        assert_eq!(
            digest.items()[0].algorithm(),
            algorithm.as_str(),
            "アルゴリズム名が一致すること",
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

// 複数のダイジェストパース
#[test]
fn prop_repr_digest_parse_multiple() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let data1 = digest_bytes(ctx);
        let data2 = digest_bytes(ctx);
        let b64_1 = base64_encode(&data1);
        let b64_2 = base64_encode(&data2);
        let input = format!("sha-256=:{}:, sha-512=:{}:", b64_1, b64_2);

        let digest =
            ReprDigest::parse(&input).expect("Digest フィールドのパースは成功するはず (実装バグ)");
        assert_eq!(digest.items().len(), 2, "アイテム数が 2 であること");
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

// get メソッド
#[test]
fn prop_repr_digest_get() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let algorithm = valid_algorithm(ctx);
        let data = digest_bytes(ctx);
        let b64 = base64_encode(&data);
        let input = format!("{}=:{}:", algorithm, b64);

        let digest =
            ReprDigest::parse(&input).expect("Digest フィールドのパースは成功するはず (実装バグ)");
        assert!(
            digest.get(&algorithm).is_some(),
            "アルゴリズム名で取得できること"
        );
        assert!(
            digest.get("nonexistent").is_none(),
            "存在しないアルゴリズムは None になること",
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

// Display ラウンドトリップ
#[test]
fn prop_repr_digest_display_roundtrip() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let data = digest_bytes(ctx);
        let b64 = base64_encode(&data);
        let input = format!("sha-256=:{}:", b64);

        let digest =
            ReprDigest::parse(&input).expect("Digest フィールドのパースは成功するはず (実装バグ)");
        let display = digest.to_string();

        let reparsed = ReprDigest::parse(&display);
        assert!(reparsed.is_ok(), "再パースは成功するはず (実装バグ)");
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
// WantContentDigest のテスト
// ========================================

// 単一の優先度パース
#[test]
fn prop_want_content_digest_parse_single() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let algorithm = valid_algorithm(ctx);
        let weight = valid_weight(ctx);
        let input = format!("{}={}", algorithm, weight);

        let want = WantContentDigest::parse(&input)
            .expect("Digest フィールドのパースは成功するはず (実装バグ)");
        assert_eq!(want.items().len(), 1, "アイテム数が 1 であること");
        assert_eq!(
            want.items()[0].algorithm(),
            algorithm.as_str(),
            "アルゴリズム名が一致すること",
        );
        assert_eq!(want.items()[0].weight(), weight, "優先度が一致すること");
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

// 複数の優先度パース
#[test]
fn prop_want_content_digest_parse_multiple() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let weight1 = valid_weight(ctx);
        let weight2 = valid_weight(ctx);
        let input = format!("sha-256={}, sha-512={}", weight1, weight2);

        let want = WantContentDigest::parse(&input)
            .expect("Digest フィールドのパースは成功するはず (実装バグ)");
        assert_eq!(want.items().len(), 2, "アイテム数が 2 であること");
        assert_eq!(
            want.items()[0].weight(),
            weight1,
            "先頭の優先度が一致すること"
        );
        assert_eq!(
            want.items()[1].weight(),
            weight2,
            "2 番目の優先度が一致すること"
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

// get メソッド
#[test]
fn prop_want_content_digest_get() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let algorithm = valid_algorithm(ctx);
        let weight = valid_weight(ctx);
        let input = format!("{}={}", algorithm, weight);

        let want = WantContentDigest::parse(&input)
            .expect("Digest フィールドのパースは成功するはず (実装バグ)");
        assert_eq!(
            want.get(&algorithm),
            Some(weight),
            "アルゴリズム名で取得できること"
        );
        assert_eq!(
            want.get(&algorithm.to_uppercase()),
            Some(weight),
            "大文字のアルゴリズム名でも取得できること",
        );
        assert!(
            want.get("nonexistent").is_none(),
            "存在しないアルゴリズムは None になること",
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

// Display ラウンドトリップ
#[test]
fn prop_want_content_digest_display_roundtrip() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let algorithm = valid_algorithm(ctx);
        let weight = valid_weight(ctx);
        let input = format!("{}={}", algorithm, weight);

        let want = WantContentDigest::parse(&input)
            .expect("Digest フィールドのパースは成功するはず (実装バグ)");
        let display = want.to_string();

        let reparsed = WantContentDigest::parse(&display);
        assert!(reparsed.is_ok(), "再パースは成功するはず (実装バグ)");
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
// WantReprDigest のテスト
// ========================================

// 単一の優先度パース
#[test]
fn prop_want_repr_digest_parse_single() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let algorithm = valid_algorithm(ctx);
        let weight = valid_weight(ctx);
        let input = format!("{}={}", algorithm, weight);

        let want = WantReprDigest::parse(&input)
            .expect("Digest フィールドのパースは成功するはず (実装バグ)");
        assert_eq!(want.items().len(), 1, "アイテム数が 1 であること");
        assert_eq!(
            want.items()[0].algorithm(),
            algorithm.as_str(),
            "アルゴリズム名が一致すること",
        );
        assert_eq!(want.items()[0].weight(), weight, "優先度が一致すること");
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

// 複数の優先度パース
#[test]
fn prop_want_repr_digest_parse_multiple() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let weight1 = valid_weight(ctx);
        let weight2 = valid_weight(ctx);
        let input = format!("sha-256={}, sha-512={}", weight1, weight2);

        let want = WantReprDigest::parse(&input)
            .expect("Digest フィールドのパースは成功するはず (実装バグ)");
        assert_eq!(want.items().len(), 2, "アイテム数が 2 であること");
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

// get メソッド
#[test]
fn prop_want_repr_digest_get() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let algorithm = valid_algorithm(ctx);
        let weight = valid_weight(ctx);
        let input = format!("{}={}", algorithm, weight);

        let want = WantReprDigest::parse(&input)
            .expect("Digest フィールドのパースは成功するはず (実装バグ)");
        assert_eq!(
            want.get(&algorithm),
            Some(weight),
            "アルゴリズム名で取得できること"
        );
        assert!(
            want.get("nonexistent").is_none(),
            "存在しないアルゴリズムは None になること",
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

// Display ラウンドトリップ
#[test]
fn prop_want_repr_digest_display_roundtrip() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let algorithm = valid_algorithm(ctx);
        let weight = valid_weight(ctx);
        let input = format!("{}={}", algorithm, weight);

        let want = WantReprDigest::parse(&input)
            .expect("Digest フィールドのパースは成功するはず (実装バグ)");
        let display = want.to_string();

        let reparsed = WantReprDigest::parse(&display);
        assert!(reparsed.is_ok(), "再パースは成功するはず (実装バグ)");
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
