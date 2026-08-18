//! Content-Type のプロパティテスト

use pbt::qdtext_value;
use shiguredo_http11::content_type::ContentType;

// ========================================
// Strategy 定義
// ========================================

// 小文字 [a-z] の 1 文字
fn lower_char(ctx: &mut noprop::TestCaseContext) -> char {
    char::from(b'a' + noprop::sample_usize_in(ctx, 0..26) as u8)
}

// 大文字 [A-Z] の 1 文字
fn upper_char(ctx: &mut noprop::TestCaseContext) -> char {
    char::from(b'A' + noprop::sample_usize_in(ctx, 0..26) as u8)
}

// 数字 [0-9] の 1 文字
fn digit_char(ctx: &mut noprop::TestCaseContext) -> char {
    char::from(b'0' + noprop::sample_usize_in(ctx, 0..10) as u8)
}

// ALPHA (A-Z / a-z) の 1 文字
fn alpha_char(ctx: &mut noprop::TestCaseContext) -> char {
    match noprop::sample_usize_in(ctx, 0..2) {
        0 => lower_char(ctx),
        _ => upper_char(ctx),
    }
}

// [a-zA-Z0-9] の 1 文字
fn alnum_char(ctx: &mut noprop::TestCaseContext) -> char {
    match noprop::sample_usize_in(ctx, 0..3) {
        0 => lower_char(ctx),
        1 => upper_char(ctx),
        _ => digit_char(ctx),
    }
}

// [a-zA-Z0-9-] の 1 文字
fn alpha_dash_char(ctx: &mut noprop::TestCaseContext) -> char {
    match noprop::sample_usize_in(ctx, 0..3) {
        0 => alpha_char(ctx),
        1 => digit_char(ctx),
        _ => '-',
    }
}

// [a-z0-9-] の 1 文字
fn lower_dash_digit_char(ctx: &mut noprop::TestCaseContext) -> char {
    match noprop::sample_usize_in(ctx, 0..3) {
        0 => lower_char(ctx),
        1 => digit_char(ctx),
        _ => '-',
    }
}

// [A-Z0-9-] の 1 文字
fn upper_dash_digit_char(ctx: &mut noprop::TestCaseContext) -> char {
    match noprop::sample_usize_in(ctx, 0..3) {
        0 => upper_char(ctx),
        1 => digit_char(ctx),
        _ => '-',
    }
}

// 有効なトークン文字列 (RFC 9110 Section 5.6.2)
// トークン文字: !#$%&'*+-.0-9A-Z^_`a-z|~
fn valid_token(ctx: &mut noprop::TestCaseContext) -> String {
    let len = noprop::sample_usize_in(ctx, 1..=8);
    let mut s = String::with_capacity(len);
    for _ in 0..len {
        s.push(token_char(ctx));
    }
    s
}

// トークン文字の 1 文字 (RFC 9110 Section 5.6.2)
fn token_char(ctx: &mut noprop::TestCaseContext) -> char {
    const SPECIAL: &[char] = &[
        '!', '#', '$', '%', '&', '\'', '*', '+', '.', '^', '_', '`', '|', '~', '-',
    ];
    match noprop::sample_usize_in(ctx, 0..3) {
        0 => SPECIAL[noprop::sample_usize_in(ctx, 0..SPECIAL.len())],
        1 => digit_char(ctx),
        _ => alpha_char(ctx),
    }
}

// boundary 値 (トークン文字のみ)
fn boundary_value(ctx: &mut noprop::TestCaseContext) -> String {
    let len = noprop::sample_usize_in(ctx, 1..=32);
    let mut s = String::with_capacity(len);
    for _ in 0..len {
        s.push(boundary_char(ctx));
    }
    s
}

// boundary 文字 [a-zA-Z0-9._-] の 1 文字
fn boundary_char(ctx: &mut noprop::TestCaseContext) -> char {
    match noprop::sample_usize_in(ctx, 0..6) {
        0 => lower_char(ctx),
        1 => upper_char(ctx),
        2 => digit_char(ctx),
        3 => '.',
        4 => '_',
        _ => '-',
    }
}

// 引用符が必要な文字を含む値
fn value_needing_quotes(ctx: &mut noprop::TestCaseContext) -> String {
    let word1 = word(ctx);
    let word2 = word(ctx);
    match noprop::sample_usize_in(ctx, 0..4) {
        // スペースを含む
        0 => format!("{} {}", word1, word2),
        // セミコロンを含む
        1 => format!("{};{}", word1, word2),
        // カンマを含む
        2 => format!("{},{}", word1, word2),
        // イコールを含む
        _ => format!("{}={}", word1, word2),
    }
}

// 小文字単語 [a-z]{1,4}
fn word(ctx: &mut noprop::TestCaseContext) -> String {
    let len = noprop::sample_usize_in(ctx, 1..=4);
    let mut s = String::with_capacity(len);
    for _ in 0..len {
        s.push(lower_char(ctx));
    }
    s
}

// ========================================
// 基本的なパースのテスト
// ========================================

/// Content-Type パースのラウンドトリップ
#[test]
fn prop_content_type_roundtrip() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        // "[a-z]{1,16}"
        let media_len = noprop::sample_usize_in(ctx, 1..=16);
        let mut media_type = String::with_capacity(media_len);
        for _ in 0..media_len {
            media_type.push(lower_char(ctx));
        }
        // "[a-z0-9-]{1,16}"
        let subtype_len = noprop::sample_usize_in(ctx, 1..=16);
        let mut subtype = String::with_capacity(subtype_len);
        for _ in 0..subtype_len {
            subtype.push(lower_dash_digit_char(ctx));
        }
        let ct_str = format!("{}/{}", media_type, subtype);
        let ct =
            ContentType::parse(&ct_str).expect("Content-Type のパースは成功するはず (実装バグ)");
        assert_eq!(ct.media_type(), media_type.as_str());
        assert_eq!(ct.subtype(), subtype.as_str());
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

/// トークン文字を使用したメディアタイプ
#[test]
fn prop_content_type_token_chars() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let media_type = valid_token(ctx);
        let subtype = valid_token(ctx);
        let ct_str = format!("{}/{}", media_type, subtype);
        let ct =
            ContentType::parse(&ct_str).expect("Content-Type のパースは成功するはず (実装バグ)");
        let expected_media = media_type.to_ascii_lowercase();
        let expected_sub = subtype.to_ascii_lowercase();
        assert_eq!(ct.media_type(), expected_media.as_str());
        assert_eq!(ct.subtype(), expected_sub.as_str());
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

/// mime_type() のテスト
#[test]
fn prop_content_type_mime_type() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let media_type = lower_string(ctx, 8);
        let subtype = lower_string(ctx, 8);
        let ct = ContentType::parse(&format!("{}/{}", media_type, subtype))
            .expect("Content-Type のパースは成功するはず (実装バグ)");
        let expected = format!("{}/{}", media_type, subtype);
        assert_eq!(ct.mime_type(), expected);
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
// charset パラメータのテスト
// ========================================

/// charset パラメータ付き Content-Type
#[test]
fn prop_content_type_with_charset() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let media_type = lower_string(ctx, 8);
        let subtype = lower_dash_digit_string(ctx, 8);
        let charset = alpha_dash_string(ctx, 16);
        let ct_str = format!("{}/{}; charset={}", media_type, subtype, charset);
        let ct =
            ContentType::parse(&ct_str).expect("Content-Type のパースは成功するはず (実装バグ)");
        assert_eq!(ct.charset(), Some(charset.as_str()));
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

/// 引用符付き charset
#[test]
fn prop_content_type_quoted_charset() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let charset = alpha_dash_string(ctx, 16);
        let ct_str = format!("text/html; charset=\"{}\"", charset);
        let ct =
            ContentType::parse(&ct_str).expect("Content-Type のパースは成功するはず (実装バグ)");
        assert_eq!(ct.charset(), Some(charset.as_str()));
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
// boundary パラメータのテスト
// ========================================

/// boundary パラメータ付き multipart
#[test]
fn prop_content_type_multipart_boundary() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let boundary = boundary_value(ctx);
        let ct_str = format!("multipart/form-data; boundary={}", boundary);
        let ct =
            ContentType::parse(&ct_str).expect("Content-Type のパースは成功するはず (実装バグ)");
        assert!(ct.is_form_data());
        assert_eq!(ct.boundary(), Some(boundary.as_str()));
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

/// 引用符付き boundary
#[test]
fn prop_content_type_quoted_boundary() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let boundary = boundary_value(ctx);
        let ct_str = format!("multipart/form-data; boundary=\"{}\"", boundary);
        let ct =
            ContentType::parse(&ct_str).expect("Content-Type のパースは成功するはず (実装バグ)");
        assert_eq!(ct.boundary(), Some(boundary.as_str()));
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

/// multipart/mixed
#[test]
fn prop_content_type_multipart_mixed() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let boundary = boundary_value(ctx);
        let ct_str = format!("multipart/mixed; boundary={}", boundary);
        let ct =
            ContentType::parse(&ct_str).expect("Content-Type のパースは成功するはず (実装バグ)");
        assert!(ct.is_multipart());
        assert!(!ct.is_form_data());
        assert_eq!(ct.boundary(), Some(boundary.as_str()));
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
// 複数パラメータのテスト
// ========================================

/// charset と boundary の両方
#[test]
fn prop_content_type_multiple_params() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let charset = alpha_dash_string(ctx, 8);
        let boundary = boundary_value(ctx);
        let ct_str = format!(
            "multipart/form-data; charset={}; boundary={}",
            charset, boundary
        );
        let ct =
            ContentType::parse(&ct_str).expect("Content-Type のパースは成功するはず (実装バグ)");
        assert_eq!(ct.charset(), Some(charset.as_str()));
        assert_eq!(ct.boundary(), Some(boundary.as_str()));
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

/// パラメータの順序が異なる場合
#[test]
fn prop_content_type_params_order() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let charset = alpha_dash_string(ctx, 8);
        let boundary = boundary_value(ctx);
        let ct_str = format!(
            "multipart/form-data; boundary={}; charset={}",
            boundary, charset
        );
        let ct =
            ContentType::parse(&ct_str).expect("Content-Type のパースは成功するはず (実装バグ)");
        assert_eq!(ct.charset(), Some(charset.as_str()));
        assert_eq!(ct.boundary(), Some(boundary.as_str()));
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

/// カスタムパラメータ
#[test]
fn prop_content_type_custom_param() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let name = lower_string(ctx, 8);
        let value = alpha_dash_string(ctx, 16);
        let ct_str = format!("text/plain; {}={}", name, value);
        let ct =
            ContentType::parse(&ct_str).expect("Content-Type のパースは成功するはず (実装バグ)");
        assert_eq!(ct.parameter(&name), Some(value.as_str()));
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
// 引用符付き値のテスト
// ========================================

/// スペースを含む引用符付き値
#[test]
fn prop_content_type_quoted_value_with_space() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let word1 = word(ctx);
        let word2 = word(ctx);
        let value = format!("{} {}", word1, word2);
        let ct_str = format!("text/plain; name=\"{}\"", value);
        let ct =
            ContentType::parse(&ct_str).expect("Content-Type のパースは成功するはず (実装バグ)");
        assert_eq!(ct.parameter("name"), Some(value.as_str()));
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

/// セミコロンを含む引用符付き値
#[test]
fn prop_content_type_quoted_value_with_semicolon() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let part1 = word(ctx);
        let part2 = word(ctx);
        let value = format!("{};{}", part1, part2);
        let ct_str = format!("text/plain; name=\"{}\"", value);
        let ct =
            ContentType::parse(&ct_str).expect("Content-Type のパースは成功するはず (実装バグ)");
        assert_eq!(ct.parameter("name"), Some(value.as_str()));
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

/// エスケープされた引用符を含む値
#[test]
fn prop_content_type_escaped_quote() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let word = lower_string(ctx, 8);
        let ct_str = format!("text/plain; name=\"{}\\\"{}\"", word, word);
        let ct =
            ContentType::parse(&ct_str).expect("Content-Type のパースは成功するはず (実装バグ)");
        let expected = format!("{}\"{}", word, word);
        assert_eq!(ct.parameter("name"), Some(expected.as_str()));
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

/// エスケープされたバックスラッシュ
#[test]
fn prop_content_type_escaped_backslash() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let word = lower_string(ctx, 8);
        let ct_str = format!("text/plain; name=\"{}\\\\{}\"", word, word);
        let ct =
            ContentType::parse(&ct_str).expect("Content-Type のパースは成功するはず (実装バグ)");
        let expected = format!("{}\\{}", word, word);
        assert_eq!(ct.parameter("name"), Some(expected.as_str()));
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
// Display のテスト
// ========================================

/// Content-Type 表示のラウンドトリップ
#[test]
fn prop_content_type_display_roundtrip() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let media_type = lower_string(ctx, 8);
        let subtype = lower_dash_digit_string(ctx, 8);
        let ct = ContentType::new(&media_type, &subtype);
        let displayed = ct.to_string();
        let reparsed =
            ContentType::parse(&displayed).expect("Content-Type のパースは成功するはず (実装バグ)");
        assert_eq!(ct.media_type(), reparsed.media_type());
        assert_eq!(ct.subtype(), reparsed.subtype());
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

/// パラメータ付き Display のラウンドトリップ
#[test]
fn prop_content_type_display_with_param_roundtrip() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let media_type = lower_string(ctx, 8);
        let subtype = lower_string(ctx, 8);
        let param_value = alpha_dash_string(ctx, 16);
        let ct = ContentType::new(&media_type, &subtype).with_parameter("charset", &param_value);
        let displayed = ct.to_string();
        let reparsed =
            ContentType::parse(&displayed).expect("Content-Type のパースは成功するはず (実装バグ)");
        assert_eq!(ct.media_type(), reparsed.media_type());
        assert_eq!(ct.subtype(), reparsed.subtype());
        assert_eq!(ct.charset(), reparsed.charset());
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

/// 引用符が必要な値の Display ラウンドトリップ
#[test]
fn prop_content_type_display_quoted_roundtrip() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let media_type = lower_string(ctx, 8);
        let subtype = lower_string(ctx, 8);
        let value = value_needing_quotes(ctx);
        let ct = ContentType::new(&media_type, &subtype).with_parameter("name", &value);
        let displayed = ct.to_string();
        let reparsed =
            ContentType::parse(&displayed).expect("Content-Type のパースは成功するはず (実装バグ)");
        assert_eq!(ct.parameter("name"), reparsed.parameter("name"));
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
// is_* メソッドのテスト
// ========================================

/// is_text()
#[test]
fn prop_content_type_is_text() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let subtype = lower_string(ctx, 8);
        let ct = ContentType::parse(&format!("text/{}", subtype))
            .expect("Content-Type のパースは成功するはず (実装バグ)");
        assert!(ct.is_text());
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

/// is_multipart()
#[test]
fn prop_content_type_is_multipart() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let subtype = lower_string(ctx, 8);
        let ct = ContentType::parse(&format!("multipart/{}", subtype))
            .expect("Content-Type のパースは成功するはず (実装バグ)");
        assert!(ct.is_multipart());
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
// 大文字小文字の正規化テスト
// ========================================

/// メディアタイプは大文字小文字を正規化
#[test]
fn prop_content_type_case_insensitive() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let media_type = upper_string(ctx, 8);
        let subtype = upper_dash_digit_string(ctx, 8);
        let ct_str = format!("{}/{}", media_type, subtype);
        let ct =
            ContentType::parse(&ct_str).expect("Content-Type のパースは成功するはず (実装バグ)");
        let expected_media_type = media_type.to_ascii_lowercase();
        let expected_subtype = subtype.to_ascii_lowercase();
        assert_eq!(ct.media_type(), expected_media_type.as_str());
        assert_eq!(ct.subtype(), expected_subtype.as_str());
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

/// パラメータ名は大文字小文字を正規化、値は保持
#[test]
fn prop_content_type_param_case() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let param_name = upper_string(ctx, 8);
        let param_value = alnum_string(ctx, 8);
        let ct_str = format!("text/plain; {}={}", param_name, param_value);
        let ct =
            ContentType::parse(&ct_str).expect("Content-Type のパースは成功するはず (実装バグ)");
        // パラメータ名は小文字で取得できる
        let lower_name = param_name.to_ascii_lowercase();
        assert_eq!(ct.parameter(&lower_name), Some(param_value.as_str()));
        // 大文字でも取得できる
        assert_eq!(ct.parameter(&param_name), Some(param_value.as_str()));
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
// 空白処理のテスト
// ========================================

/// 前後の空白
#[test]
fn prop_content_type_trim_whitespace() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let media_type = lower_string(ctx, 8);
        let subtype = lower_string(ctx, 8);
        let ct_str = format!("  {}/{}  ", media_type, subtype);
        let ct =
            ContentType::parse(&ct_str).expect("Content-Type のパースは成功するはず (実装バグ)");
        assert_eq!(ct.media_type(), media_type.as_str());
        assert_eq!(ct.subtype(), subtype.as_str());
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

/// パラメータ周りの空白
#[test]
fn prop_content_type_param_whitespace() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let param_value = alnum_string(ctx, 8);
        let ct_str = format!("text/plain  ;  charset  =  {}", param_value);
        let ct =
            ContentType::parse(&ct_str).expect("Content-Type のパースは成功するはず (実装バグ)");
        assert_eq!(ct.charset(), Some(param_value.as_str()));
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
// エラーケースのテスト
// ========================================

/// 不正な文字を含むメディアタイプ
#[test]
fn prop_content_type_invalid_media_type_char() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let media_type = lower_string(ctx, 4);
        // 元の戦略: prop_oneof![Just(' '), Just('/'), Just('('), Just(')'), Just('<'), Just('>')]
        let invalid_char = match noprop::sample_usize_in(ctx, 0..6) {
            0 => ' ',
            1 => '/',
            2 => '(',
            3 => ')',
            4 => '<',
            _ => '>',
        };
        let subtype = lower_string(ctx, 4);
        let ct_str = format!("{}{}{}/{}", media_type, invalid_char, media_type, subtype);
        let result = ContentType::parse(&ct_str);
        assert!(result.is_err());
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
// new() と with_parameter() のテスト
// ========================================

/// new() + with_parameter() で構築した Content-Type のアクセサが期待値と一致する
#[test]
fn prop_content_type_new_with_params() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let media_type = lower_string(ctx, 8);
        let subtype = lower_string(ctx, 8);
        let charset = lower_string(ctx, 8);
        let boundary = boundary_value(ctx);
        let ct = ContentType::new(&media_type, &subtype)
            .with_parameter("charset", &charset)
            .with_parameter("boundary", &boundary);

        assert_eq!(ct.media_type(), media_type.as_str());
        assert_eq!(ct.subtype(), subtype.as_str());
        assert_eq!(ct.charset(), Some(charset.as_str()));
        assert_eq!(ct.boundary(), Some(boundary.as_str()));
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

/// parameters() アクセサ
#[test]
fn prop_content_type_parameters_accessor() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let media_type = lower_string(ctx, 8);
        let subtype = lower_string(ctx, 8);
        let value1 = lower_string(ctx, 8);
        let value2 = lower_string(ctx, 8);
        let ct = ContentType::new(&media_type, &subtype)
            .with_parameter("param1", &value1)
            .with_parameter("param2", &value2);

        let params = ct.parameters();
        assert_eq!(params.len(), 2);
        assert_eq!(&params[0].0, "param1");
        assert_eq!(&params[0].1, &value1);
        assert_eq!(&params[1].0, "param2");
        assert_eq!(&params[1].1, &value2);
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

/// qdtext (obs-text を含む) を quoted parameter として往復できる
#[test]
fn prop_content_type_quoted_obs_text_roundtrip() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let value = qdtext_value(ctx, 0..=16);
        let ct_str = format!("text/plain; ext=\"{}\"", value);
        let ct =
            ContentType::parse(&ct_str).expect("Content-Type のパースは成功するはず (実装バグ)");
        assert_eq!(ct.parameter("ext"), Some(value.as_str()));

        // Display 出力は obs-text / 制御文字以外をエスケープしないため、
        // value がそのまま埋め込まれる。直接 assert で実体化する。
        let displayed = ct.to_string();
        assert!(
            displayed.contains(&value) || (value.is_empty() && displayed.contains("ext=\"\"")),
            "Display 出力 {:?} に value {:?} が含まれない",
            displayed,
            value,
        );
        let reparsed =
            ContentType::parse(&displayed).expect("Content-Type のパースは成功するはず (実装バグ)");
        assert_eq!(reparsed.parameter("ext"), Some(value.as_str()));
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
// 文字列ヘルパー
// ========================================

// 小文字のみの文字列 (1..=max_len)
fn lower_string(ctx: &mut noprop::TestCaseContext, max_len: usize) -> String {
    let len = noprop::sample_usize_in(ctx, 1..=max_len);
    let mut s = String::with_capacity(len);
    for _ in 0..len {
        s.push(lower_char(ctx));
    }
    s
}

// 大文字のみの文字列 (1..=max_len)
fn upper_string(ctx: &mut noprop::TestCaseContext, max_len: usize) -> String {
    let len = noprop::sample_usize_in(ctx, 1..=max_len);
    let mut s = String::with_capacity(len);
    for _ in 0..len {
        s.push(upper_char(ctx));
    }
    s
}

// 英数字のみの文字列 (1..=max_len)
fn alnum_string(ctx: &mut noprop::TestCaseContext, max_len: usize) -> String {
    let len = noprop::sample_usize_in(ctx, 1..=max_len);
    let mut s = String::with_capacity(len);
    for _ in 0..len {
        s.push(alnum_char(ctx));
    }
    s
}

// [a-z0-9-] のみの文字列 (1..=max_len)
fn lower_dash_digit_string(ctx: &mut noprop::TestCaseContext, max_len: usize) -> String {
    let len = noprop::sample_usize_in(ctx, 1..=max_len);
    let mut s = String::with_capacity(len);
    for _ in 0..len {
        s.push(lower_dash_digit_char(ctx));
    }
    s
}

// [a-zA-Z0-9-] のみの文字列 (1..=max_len)
fn alpha_dash_string(ctx: &mut noprop::TestCaseContext, max_len: usize) -> String {
    let len = noprop::sample_usize_in(ctx, 1..=max_len);
    let mut s = String::with_capacity(len);
    for _ in 0..len {
        s.push(alpha_dash_char(ctx));
    }
    s
}

// [A-Z0-9-] のみの文字列 (1..=max_len)
fn upper_dash_digit_string(ctx: &mut noprop::TestCaseContext, max_len: usize) -> String {
    let len = noprop::sample_usize_in(ctx, 1..=max_len);
    let mut s = String::with_capacity(len);
    for _ in 0..len {
        s.push(upper_dash_digit_char(ctx));
    }
    s
}
