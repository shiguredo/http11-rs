//! Content-Language ヘッダーのプロパティテスト (content_language.rs)

use pbt::language_tag;
use shiguredo_http11::content_language::ContentLanguage;

// Content-Language のラウンドトリップ
#[test]
fn prop_content_language_roundtrip() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let tag_count = noprop::sample_usize_in(ctx, 1..4);
        let mut tags = Vec::new();
        for _ in 0..tag_count {
            tags.push(language_tag(ctx));
        }

        let header = tags.join(", ");
        let parsed = ContentLanguage::parse(&header)
            .expect("Content-Language のパースは成功するはず (実装バグ)");
        let displayed = parsed.to_string();
        let reparsed = ContentLanguage::parse(&displayed)
            .expect("Content-Language のパースは成功するはず (実装バグ)");
        assert_eq!(parsed, reparsed);
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
