//! Content-Encoding ヘッダーのプロパティテスト

use shiguredo_http11::content_encoding::ContentEncoding;

// ========================================
// Strategy 定義
// ========================================

// 標準的なエンコーディング
fn standard_encoding(ctx: &mut noprop::TestCaseContext) -> &'static str {
    noprop::sample_choice(ctx, &["gzip", "deflate", "compress", "identity"])
}

// ALPHA (A-Z / a-z) の 1 文字
fn alpha_char(ctx: &mut noprop::TestCaseContext) -> char {
    let index = noprop::sample_usize_in(ctx, 0..52);
    if index < 26 {
        char::from(b'A' + index as u8)
    } else {
        char::from(b'a' + (index - 26) as u8)
    }
}

// カスタムエンコーディング (token 文字のみ)
fn custom_encoding(ctx: &mut noprop::TestCaseContext) -> String {
    // 元の戦略: "[a-zA-Z][a-zA-Z0-9-]{0,15}"
    let len = noprop::sample_usize_in(ctx, 1..=16);
    let mut s = String::with_capacity(len);
    for i in 0..len {
        let c = if i == 0 {
            alpha_char(ctx)
        } else {
            match noprop::sample_usize_in(ctx, 0..3) {
                0 => alpha_char(ctx),
                1 => char::from(b'0' + noprop::sample_usize_in(ctx, 0..10) as u8),
                _ => '-',
            }
        };
        s.push(c);
    }
    s
}

// ========================================
// 単一エンコーディングのテスト
// ========================================

// カスタムエンコーディングのラウンドトリップ
#[test]
fn prop_content_encoding_custom_roundtrip() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let enc = custom_encoding(ctx);
        let ce = ContentEncoding::parse(&enc)
            .expect("Content-Encoding のパースは成功するはず (実装バグ)");

        assert_eq!(ce.encodings().len(), 1);

        // Display で小文字に正規化される
        let display = ce.to_string();
        assert_eq!(display, enc.to_lowercase());
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
// 複数エンコーディングのテスト
// ========================================

// 複数の標準エンコーディング
#[test]
fn prop_content_encoding_multiple() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let count = noprop::sample_usize_in(ctx, 1..5);
        let mut encodings = Vec::new();
        for _ in 0..count {
            encodings.push(standard_encoding(ctx));
        }

        let input = encodings.join(", ");
        let ce = ContentEncoding::parse(&input)
            .expect("Content-Encoding のパースは成功するはず (実装バグ)");

        assert_eq!(ce.encodings().len(), encodings.len());
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
