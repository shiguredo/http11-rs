//! Content-Disposition ヘッダーのプロパティテスト

use pbt::qdtext_value;
use shiguredo_http11::content_disposition::{ContentDisposition, DispositionType};

// ========================================
// Strategy 定義
// ========================================

/// disposition-type を一様に選ぶ
fn disposition_type_str(ctx: &mut noprop::TestCaseContext) -> &'static str {
    noprop::sample_choice(ctx, &["inline", "attachment", "form-data"])
}

/// qdtext_char / qdtext_value は pbt クレートで共通化済み。
/// content_disposition では空文字列を含む 0..=32 を使う。
fn quoted_string_content(ctx: &mut noprop::TestCaseContext) -> String {
    qdtext_value(ctx, 0..=32)
}

/// ASCII ファイル名: [a-zA-Z0-9_.-]{1,32}
fn ascii_filename(ctx: &mut noprop::TestCaseContext) -> String {
    let len = noprop::sample_usize_in(ctx, 1..=32);
    let mut s = String::with_capacity(len);
    for _ in 0..len {
        s.push(match noprop::sample_usize_in(ctx, 0..4) {
            0 => char::from(b'a' + noprop::sample_usize_in(ctx, 0..26) as u8),
            1 => char::from(b'A' + noprop::sample_usize_in(ctx, 0..26) as u8),
            2 => char::from(b'0' + noprop::sample_usize_in(ctx, 0..10) as u8),
            _ => noprop::sample_choice(ctx, &['_', '.', '-']),
        });
    }
    s
}

/// UTF-8 ファイル名 (日本語を含む)
fn utf8_filename(ctx: &mut noprop::TestCaseContext) -> String {
    match noprop::sample_usize_in(ctx, 0..5) {
        0 => ascii_filename(ctx),
        1 => "日本語.txt".to_string(),
        2 => "файл.txt".to_string(),
        3 => "文件.txt".to_string(),
        _ => "ファイル名.pdf".to_string(),
    }
}

/// パーセントエンコードされた UTF-8 値
fn percent_encoded_utf8(ctx: &mut noprop::TestCaseContext) -> (String, String) {
    let filename = utf8_filename(ctx);
    let encoded = encode_ext_value_for_test(&filename);
    (filename, encoded)
}

/// テスト用のエンコード関数
fn encode_ext_value_for_test(s: &str) -> String {
    let mut result = String::new();
    for byte in s.bytes() {
        if is_attr_char_for_test(byte) {
            result.push(byte as char);
        } else {
            result.push('%');
            result.push_str(&format!("{:02X}", byte));
        }
    }
    result
}

/// attr-char (RFC 8187 Section 3.2.1) かどうか
fn is_attr_char_for_test(b: u8) -> bool {
    matches!(
        b,
        b'A'..=b'Z'
            | b'a'..=b'z'
            | b'0'..=b'9'
            | b'!'
            | b'#'
            | b'$'
            | b'&'
            | b'+'
            | b'-'
            | b'.'
            | b'^'
            | b'_'
            | b'`'
            | b'|'
            | b'~'
    )
}

/// カスタムパラメータ名: [a-z]{1,8}
fn custom_param_name(ctx: &mut noprop::TestCaseContext) -> String {
    let len = noprop::sample_usize_in(ctx, 1..=8);
    let mut s = String::with_capacity(len);
    for _ in 0..len {
        s.push(char::from(b'a' + noprop::sample_usize_in(ctx, 0..26) as u8));
    }
    s
}

// ========================================
// 単純なパースのテスト
// ========================================

/// disposition-type のみのラウンドトリップ
#[test]
fn prop_content_disposition_type_only_roundtrip() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let dtype = disposition_type_str(ctx);
        let cd = ContentDisposition::parse(dtype)
            .expect("Content-Disposition のパースは成功するはず (実装バグ)");

        match dtype {
            "inline" => {
                assert!(cd.is_inline());
                assert_eq!(cd.disposition_type(), DispositionType::Inline);
            }
            "attachment" => {
                assert!(cd.is_attachment());
                assert_eq!(cd.disposition_type(), DispositionType::Attachment);
            }
            "form-data" => {
                assert!(cd.is_form_data());
                assert_eq!(cd.disposition_type(), DispositionType::FormData);
            }
            _ => unreachable!("分岐は 3 本なので (実装バグ)"),
        }

        // Display で正規化される
        let display = cd.to_string();
        assert_eq!(display, dtype);
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
// filename パラメータのテスト
// ========================================

/// 引用符付き filename のラウンドトリップ
#[test]
fn prop_content_disposition_filename_quoted_roundtrip() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let dtype = disposition_type_str(ctx);
        let filename = ascii_filename(ctx);
        let input = format!("{}; filename=\"{}\"", dtype, filename);
        let cd = ContentDisposition::parse(&input)
            .expect("Content-Disposition のパースは成功するはず (実装バグ)");

        assert_eq!(cd.filename(), Some(filename.as_str()));
        assert_eq!(cd.filename_ascii(), Some(filename.as_str()));
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

/// 引用符なし filename のパース
#[test]
fn prop_content_disposition_filename_unquoted_roundtrip() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let dtype = disposition_type_str(ctx);
        let filename = ascii_filename(ctx);
        let input = format!("{}; filename={}", dtype, filename);
        let cd = ContentDisposition::parse(&input)
            .expect("Content-Disposition のパースは成功するはず (実装バグ)");

        assert_eq!(cd.filename(), Some(filename.as_str()));
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

/// 特殊文字を含む引用符付き文字列
#[test]
fn prop_content_disposition_quoted_string_content() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let dtype = disposition_type_str(ctx);
        let content = quoted_string_content(ctx);
        let input = format!("{}; filename=\"{}\"", dtype, content);
        let cd = ContentDisposition::parse(&input)
            .expect("Content-Disposition のパースは成功するはず (実装バグ)");

        assert_eq!(cd.filename(), Some(content.as_str()));
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
// filename* (RFC 8187 ext-value) のテスト
// ========================================

/// filename* のラウンドトリップ
#[test]
fn prop_content_disposition_filename_ext_roundtrip() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let dtype = disposition_type_str(ctx);
        let (original, encoded) = percent_encoded_utf8(ctx);
        let input = format!("{}; filename*=UTF-8''{}", dtype, encoded);
        let cd = ContentDisposition::parse(&input)
            .expect("Content-Disposition のパースは成功するはず (実装バグ)");

        assert_eq!(cd.filename_ext(), Some(original.as_str()));
        assert_eq!(cd.filename(), Some(original.as_str()));
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

/// filename* は filename より優先される
#[test]
fn prop_content_disposition_filename_ext_priority() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let dtype = disposition_type_str(ctx);
        let ascii_name = ascii_filename(ctx);
        let (utf8_name, encoded) = percent_encoded_utf8(ctx);
        let input = format!(
            "{}; filename=\"{}\"; filename*=UTF-8''{}",
            dtype, ascii_name, encoded
        );
        let cd = ContentDisposition::parse(&input)
            .expect("Content-Disposition のパースは成功するはず (実装バグ)");

        // filename* が優先
        assert_eq!(cd.filename(), Some(utf8_name.as_str()));
        // 個別アクセスも可能
        assert_eq!(cd.filename_ascii(), Some(ascii_name.as_str()));
        assert_eq!(cd.filename_ext(), Some(utf8_name.as_str()));
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
// name パラメータのテスト (form-data)
// ========================================

/// form-data with name のラウンドトリップ
#[test]
fn prop_content_disposition_form_data_name_roundtrip() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let name = ascii_filename(ctx);
        let input = format!("form-data; name=\"{}\"", name);
        let cd = ContentDisposition::parse(&input)
            .expect("Content-Disposition のパースは成功するはず (実装バグ)");

        assert!(cd.is_form_data());
        assert_eq!(cd.name(), Some(name.as_str()));
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

/// form-data with name and filename
#[test]
fn prop_content_disposition_form_data_name_and_filename() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let name = ascii_filename(ctx);
        let filename = ascii_filename(ctx);
        let input = format!("form-data; name=\"{}\"; filename=\"{}\"", name, filename);
        let cd = ContentDisposition::parse(&input)
            .expect("Content-Disposition のパースは成功するはず (実装バグ)");

        assert!(cd.is_form_data());
        assert_eq!(cd.name(), Some(name.as_str()));
        assert_eq!(cd.filename(), Some(filename.as_str()));
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
// カスタムパラメータのテスト
// ========================================

/// カスタムパラメータの取得
#[test]
fn prop_content_disposition_custom_parameter() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let dtype = disposition_type_str(ctx);
        let param_value = ascii_filename(ctx);
        // 予約済みパラメータ名 "filename" / "name" を除外する
        // 拒否率は 2 / (26^1 + ... + 26^8) ≈ 0 なので max_attempts は 8 で十分。
        // sample_with_rejection を使うため、このテストでは valid-by-construction の
        // 検証 (rejected_cases == 0) は行わない。
        let param_name = noprop::sample_with_rejection(ctx, 8, |ctx| {
            let name = custom_param_name(ctx);
            if matches!(name.as_str(), "filename" | "name") {
                None
            } else {
                Some(name)
            }
        });

        let input = format!("{}; {}=\"{}\"", dtype, param_name, param_value);
        let cd = ContentDisposition::parse(&input)
            .expect("Content-Disposition のパースは成功するはず (実装バグ)");

        assert_eq!(cd.parameter(&param_name), Some(param_value.as_str()));
        Ok(())
    })?;

    Ok(())
}

// ========================================
// ビルダーパターンのテスト
// ========================================

/// ContentDisposition::new + with_filename
#[test]
fn prop_content_disposition_builder_filename() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let filename = ascii_filename(ctx);
        let cd = ContentDisposition::new(DispositionType::Attachment).with_filename(&filename);

        assert!(cd.is_attachment());
        assert_eq!(cd.filename_ascii(), Some(filename.as_str()));
        assert_eq!(cd.filename(), Some(filename.as_str()));
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

/// ContentDisposition::new + with_filename_ext
#[test]
fn prop_content_disposition_builder_filename_ext() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let filename = utf8_filename(ctx);
        let cd = ContentDisposition::new(DispositionType::Attachment).with_filename_ext(&filename);

        assert_eq!(cd.filename_ext(), Some(filename.as_str()));
        assert_eq!(cd.filename(), Some(filename.as_str()));
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

/// ContentDisposition::new + with_name
#[test]
fn prop_content_disposition_builder_name() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let name = ascii_filename(ctx);
        let cd = ContentDisposition::new(DispositionType::FormData).with_name(&name);

        assert!(cd.is_form_data());
        assert_eq!(cd.name(), Some(name.as_str()));
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

/// 複合ビルダー
#[test]
fn prop_content_disposition_builder_combined() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let name = ascii_filename(ctx);
        let ascii_fn = ascii_filename(ctx);
        let utf8_fn = utf8_filename(ctx);
        let cd = ContentDisposition::new(DispositionType::FormData)
            .with_name(&name)
            .with_filename(&ascii_fn)
            .with_filename_ext(&utf8_fn);

        assert!(cd.is_form_data());
        assert_eq!(cd.name(), Some(name.as_str()));
        assert_eq!(cd.filename_ascii(), Some(ascii_fn.as_str()));
        assert_eq!(cd.filename_ext(), Some(utf8_fn.as_str()));
        // filename() は filename* を優先
        assert_eq!(cd.filename(), Some(utf8_fn.as_str()));
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

/// attachment + filename の Display
#[test]
fn prop_content_disposition_display_filename() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let fname = ascii_filename(ctx);
        let cd = ContentDisposition::new(DispositionType::Attachment).with_filename(&fname);
        let display = cd.to_string();

        assert!(display.starts_with("attachment"));
        let expected = format!("filename=\"{}\"", fname);
        assert!(display.contains(&expected));
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

/// form-data + name + filename の Display
#[test]
fn prop_content_disposition_display_form_data() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let nm = ascii_filename(ctx);
        let fname = ascii_filename(ctx);
        let cd = ContentDisposition::new(DispositionType::FormData)
            .with_name(&nm)
            .with_filename(&fname);
        let display = cd.to_string();

        assert!(display.starts_with("form-data"));
        let expected_name = format!("name=\"{}\"", nm);
        let expected_filename = format!("filename=\"{}\"", fname);
        assert!(display.contains(&expected_name));
        assert!(display.contains(&expected_filename));
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

/// filename* の Display (パーセントエンコーディング)
#[test]
fn prop_content_disposition_display_filename_ext() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let filename = utf8_filename(ctx);
        let cd = ContentDisposition::new(DispositionType::Attachment).with_filename_ext(&filename);
        let display = cd.to_string();

        assert!(display.starts_with("attachment"));
        assert!(display.contains("filename*=UTF-8''"));
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
// パラメータ数の hard cap
// ========================================

/// 33..=200 個のパラメータは TooManyParameters を返す
#[test]
fn prop_content_disposition_too_many_params() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let count = noprop::sample_usize_in(ctx, 33..=200);
        let mut s = String::from("attachment");
        for i in 0..count {
            s.push_str(&format!("; p{}=\"v\"", i));
        }
        let result = shiguredo_http11::content_disposition::ContentDisposition::parse(&s);
        assert!(matches!(
            result,
            Err(shiguredo_http11::content_disposition::ContentDispositionError::TooManyParameters)
        ));
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

/// 0..=32 個のパラメータは正常に parse される
#[test]
fn prop_content_disposition_at_most_32_params_ok() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let count = noprop::sample_usize_in(ctx, 0..=32);
        let mut s = String::from("attachment");
        for i in 0..count {
            s.push_str(&format!("; p{}=\"v\"", i));
        }
        let result = shiguredo_http11::content_disposition::ContentDisposition::parse(&s);
        assert!(result.is_ok(), "count={}: {:?}", count, result);
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
