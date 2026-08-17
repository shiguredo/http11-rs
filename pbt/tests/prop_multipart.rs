//! multipart/form-data のプロパティテスト

use shiguredo_http11::content_type::ContentType;
use shiguredo_http11::multipart::{MultipartBuilder, MultipartParser, Part};

// ========================================
// Strategy 定義
// ========================================

/// 有効なフィールド名 (RFC 7578): [a-zA-Z][a-zA-Z0-9_]{0,15}
fn valid_field_name(ctx: &mut noprop::TestCaseContext) -> String {
    let mut s = String::with_capacity(16);
    s.push(match noprop::sample_usize_in(ctx, 0..2) {
        0 => char::from(b'a' + noprop::sample_usize_in(ctx, 0..26) as u8),
        _ => char::from(b'A' + noprop::sample_usize_in(ctx, 0..26) as u8),
    });
    let rest = noprop::sample_usize_in(ctx, 0..=15);
    for _ in 0..rest {
        s.push(match noprop::sample_usize_in(ctx, 0..3) {
            0 => char::from(b'a' + noprop::sample_usize_in(ctx, 0..26) as u8),
            1 => char::from(b'A' + noprop::sample_usize_in(ctx, 0..26) as u8),
            2 => char::from(b'0' + noprop::sample_usize_in(ctx, 0..10) as u8),
            _ => '_',
        });
    }
    s
}

/// 有効なファイル名: [a-zA-Z0-9_-]{1,16}.[a-z]{1,4}
fn valid_filename(ctx: &mut noprop::TestCaseContext) -> String {
    let stem = noprop::sample_usize_in(ctx, 1..=16);
    let ext = noprop::sample_usize_in(ctx, 1..=4);
    let mut s = String::with_capacity(stem + 1 + ext);
    for _ in 0..stem {
        s.push(match noprop::sample_usize_in(ctx, 0..4) {
            0 => char::from(b'a' + noprop::sample_usize_in(ctx, 0..26) as u8),
            1 => char::from(b'A' + noprop::sample_usize_in(ctx, 0..26) as u8),
            2 => char::from(b'0' + noprop::sample_usize_in(ctx, 0..10) as u8),
            _ => noprop::sample_choice(ctx, &['_', '-']),
        });
    }
    s.push('.');
    for _ in 0..ext {
        s.push(char::from(b'a' + noprop::sample_usize_in(ctx, 0..26) as u8));
    }
    s
}

/// 有効なテキスト値: [a-zA-Z0-9 .,!?]{0,64}
fn valid_text_value(ctx: &mut noprop::TestCaseContext) -> String {
    let len = noprop::sample_usize_in(ctx, 0..=64);
    let mut s = String::with_capacity(len);
    for _ in 0..len {
        s.push(match noprop::sample_usize_in(ctx, 0..7) {
            0 => char::from(b'a' + noprop::sample_usize_in(ctx, 0..26) as u8),
            1 => char::from(b'A' + noprop::sample_usize_in(ctx, 0..26) as u8),
            2 => char::from(b'0' + noprop::sample_usize_in(ctx, 0..10) as u8),
            3 => ' ',
            4 => '.',
            5 => ',',
            6 => '!',
            _ => '?',
        });
    }
    s
}

/// 有効な境界文字列: [a-zA-Z0-9]{8,32}
fn valid_boundary(ctx: &mut noprop::TestCaseContext) -> String {
    let len = noprop::sample_usize_in(ctx, 8..=32);
    let mut s = String::with_capacity(len);
    for _ in 0..len {
        s.push(match noprop::sample_usize_in(ctx, 0..3) {
            0 => char::from(b'a' + noprop::sample_usize_in(ctx, 0..26) as u8),
            1 => char::from(b'A' + noprop::sample_usize_in(ctx, 0..26) as u8),
            _ => char::from(b'0' + noprop::sample_usize_in(ctx, 0..10) as u8),
        });
    }
    s
}

/// 有効な MIME タイプ
fn valid_mime_type(ctx: &mut noprop::TestCaseContext) -> String {
    noprop::sample_choice(
        ctx,
        &[
            "text/plain".to_string(),
            "text/html".to_string(),
            "application/json".to_string(),
            "application/octet-stream".to_string(),
            "image/png".to_string(),
            "image/jpeg".to_string(),
        ],
    )
}

/// 英数字文字列
fn alnum_string(
    ctx: &mut noprop::TestCaseContext,
    len_range: core::ops::RangeInclusive<usize>,
) -> String {
    let len = noprop::sample_usize_in(ctx, len_range);
    let mut s = String::with_capacity(len);
    for _ in 0..len {
        s.push(match noprop::sample_usize_in(ctx, 0..3) {
            0 => char::from(b'a' + noprop::sample_usize_in(ctx, 0..26) as u8),
            1 => char::from(b'A' + noprop::sample_usize_in(ctx, 0..26) as u8),
            _ => char::from(b'0' + noprop::sample_usize_in(ctx, 0..10) as u8),
        });
    }
    s
}

// ========================================
// Part 構造体のテスト
// ========================================

/// テキストフィールドのラウンドトリップ
#[test]
fn prop_multipart_text_field_roundtrip() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let name = valid_field_name(ctx);
        let value = valid_text_value(ctx);
        let body = MultipartBuilder::with_boundary("test-boundary")
            .expect("マルチパートのパースは成功するはず (実装バグ)")
            .text_field(&name, &value)
            .build();

        let mut parser = MultipartParser::new("test-boundary")
            .expect("マルチパートのパースは成功するはず (実装バグ)");
        parser
            .feed(&body)
            .expect("マルチパートのパースは成功するはず (実装バグ)");

        let part = parser
            .next_part()
            .expect("結果は存在するはず (実装バグ)")
            .expect("マルチパートのパースは成功するはず (実装バグ)");
        assert_eq!(part.name(), Some(name.as_str()));
        assert_eq!(part.body_str(), Some(value.as_str()));
        assert!(!part.is_file());

        assert!(
            parser
                .next_part()
                .expect("マルチパートのパースは成功するはず (実装バグ)")
                .is_none()
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

/// 複数フィールドのラウンドトリップ
#[test]
fn prop_multipart_multiple_fields_roundtrip() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let name1 = valid_field_name(ctx);
        let value1 = alnum_string(ctx, 0..=16);
        let name2 = valid_field_name(ctx);
        let value2 = alnum_string(ctx, 0..=16);
        let body = MultipartBuilder::with_boundary("boundary")
            .expect("マルチパートのパースは成功するはず (実装バグ)")
            .text_field(&name1, &value1)
            .text_field(&name2, &value2)
            .build();

        let mut parser = MultipartParser::new("boundary")
            .expect("マルチパートのパースは成功するはず (実装バグ)");
        parser
            .feed(&body)
            .expect("マルチパートのパースは成功するはず (実装バグ)");

        let part1 = parser
            .next_part()
            .expect("結果は存在するはず (実装バグ)")
            .expect("マルチパートのパースは成功するはず (実装バグ)");
        assert_eq!(part1.name(), Some(name1.as_str()));
        assert_eq!(part1.body_str(), Some(value1.as_str()));

        let part2 = parser
            .next_part()
            .expect("結果は存在するはず (実装バグ)")
            .expect("マルチパートのパースは成功するはず (実装バグ)");
        assert_eq!(part2.name(), Some(name2.as_str()));
        assert_eq!(part2.body_str(), Some(value2.as_str()));

        assert!(
            parser
                .next_part()
                .expect("マルチパートのパースは成功するはず (実装バグ)")
                .is_none()
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

/// ファイルフィールドのラウンドトリップ
#[test]
fn prop_multipart_file_field_roundtrip() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let name = valid_field_name(ctx);
        let filename = valid_filename(ctx);
        let data_len = noprop::sample_usize_in(ctx, 0..=64);
        let data = noprop::sample_bytes_vec(ctx, data_len);
        let body = MultipartBuilder::with_boundary("file-boundary")
            .expect("マルチパートのパースは成功するはず (実装バグ)")
            .file_field(&name, &filename, "application/octet-stream", &data)
            .build();

        let mut parser = MultipartParser::new("file-boundary")
            .expect("マルチパートのパースは成功するはず (実装バグ)");
        parser
            .feed(&body)
            .expect("マルチパートのパースは成功するはず (実装バグ)");

        let part = parser
            .next_part()
            .expect("結果は存在するはず (実装バグ)")
            .expect("マルチパートのパースは成功するはず (実装バグ)");
        assert_eq!(part.name(), Some(name.as_str()));
        assert_eq!(part.filename(), Some(filename.as_str()));
        assert!(part.is_file());
        assert_eq!(part.body(), data.as_slice());

        assert!(
            parser
                .next_part()
                .expect("マルチパートのパースは成功するはず (実装バグ)")
                .is_none()
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

/// Part::new のテスト
#[test]
fn prop_multipart_part_new() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let name = valid_field_name(ctx);
        let value = alnum_string(ctx, 0..=32);
        let part = Part::new(&name).with_body(value.as_bytes());

        assert_eq!(part.name(), Some(name.as_str()));
        assert_eq!(part.body_str(), Some(value.as_str()));
        assert!(!part.is_file());
        assert!(part.filename().is_none());
        assert!(part.content_disposition().is_some());
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

/// Part::file のテスト
#[test]
fn prop_multipart_part_file() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let name = valid_field_name(ctx);
        let filename = valid_filename(ctx);
        let mime_type = valid_mime_type(ctx);
        let part = Part::file(&name, &filename, &mime_type).with_body(b"content");

        assert_eq!(part.name(), Some(name.as_str()));
        assert_eq!(part.filename(), Some(filename.as_str()));
        assert!(part.is_file());
        assert!(part.content_type().is_some());
        assert!(part.content_disposition().is_some());
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

/// Part::with_content_type のテスト
#[test]
fn prop_multipart_part_with_content_type() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let name = valid_field_name(ctx);
        let mime_type = valid_mime_type(ctx);
        let ct =
            ContentType::parse(&mime_type).expect("マルチパートのパースは成功するはず (実装バグ)");
        let part = Part::new(&name)
            .with_body(b"test")
            .with_content_type(ct.clone());

        assert!(part.content_type().is_some());
        assert_eq!(
            part.content_type()
                .expect("マルチパートのパースは成功するはず (実装バグ)")
                .media_type(),
            ct.media_type()
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

// ========================================
// MultipartParser のテスト
// ========================================

/// パーサーの is_finished テスト
#[test]
fn prop_multipart_parser_is_finished() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let name = valid_field_name(ctx);
        let value = valid_text_value(ctx);
        let body = MultipartBuilder::with_boundary("boundary")
            .expect("マルチパートのパースは成功するはず (実装バグ)")
            .text_field(&name, &value)
            .build();

        let mut parser = MultipartParser::new("boundary")
            .expect("マルチパートのパースは成功するはず (実装バグ)");
        parser
            .feed(&body)
            .expect("マルチパートのパースは成功するはず (実装バグ)");

        assert!(!parser.is_finished());

        let _ = parser
            .next_part()
            .expect("マルチパートのパースは成功するはず (実装バグ)");
        let _ = parser
            .next_part()
            .expect("マルチパートのパースは成功するはず (実装バグ)"); // None を取得

        assert!(parser.is_finished());
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
// MultipartBuilder のテスト
// ========================================

/// MultipartBuilder::new のテスト
#[test]
fn prop_multipart_builder_new() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let random_value = noprop::sample_u64(ctx);
        let builder = MultipartBuilder::new(random_value);

        // 境界が正しいフォーマットで生成される
        let expected_boundary = format!("----FormBoundary{}", random_value);
        assert_eq!(builder.boundary(), expected_boundary.as_str());
        assert!(builder.content_type().contains("multipart/form-data"));
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

/// MultipartBuilder::with_boundary のテスト
#[test]
fn prop_multipart_builder_with_boundary() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let boundary = valid_boundary(ctx);
        let builder = MultipartBuilder::with_boundary(&boundary)
            .expect("マルチパートのパースは成功するはず (実装バグ)");

        assert_eq!(builder.boundary(), boundary.as_str());
        assert!(builder.content_type().contains(&boundary));
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

/// MultipartBuilder::content_type のテスト
#[test]
fn prop_multipart_builder_content_type() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let boundary = valid_boundary(ctx);
        let builder = MultipartBuilder::with_boundary(&boundary)
            .expect("マルチパートのパースは成功するはず (実装バグ)");
        let content_type = builder.content_type();
        let expected_boundary = format!("boundary={}", boundary);

        assert!(content_type.starts_with("multipart/form-data"));
        assert!(content_type.contains(&expected_boundary));
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
// ラウンドトリップテスト
// ========================================

/// 動的な境界でのラウンドトリップ
#[test]
fn prop_multipart_dynamic_boundary_roundtrip() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let boundary = valid_boundary(ctx);
        let name = valid_field_name(ctx);
        let value = valid_text_value(ctx);
        let body = MultipartBuilder::with_boundary(&boundary)
            .expect("マルチパートのパースは成功するはず (実装バグ)")
            .text_field(&name, &value)
            .build();

        let mut parser =
            MultipartParser::new(&boundary).expect("マルチパートのパースは成功するはず (実装バグ)");
        parser
            .feed(&body)
            .expect("マルチパートのパースは成功するはず (実装バグ)");

        let part = parser
            .next_part()
            .expect("結果は存在するはず (実装バグ)")
            .expect("マルチパートのパースは成功するはず (実装バグ)");
        assert_eq!(part.name(), Some(name.as_str()));
        assert_eq!(part.body_str(), Some(value.as_str()));
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

/// テキストとファイルの混合
#[test]
fn prop_multipart_mixed_fields_roundtrip() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let text_name = valid_field_name(ctx);
        let text_value = valid_text_value(ctx);
        let file_name = valid_field_name(ctx);
        let filename = valid_filename(ctx);
        let data_len = noprop::sample_usize_in(ctx, 0..=32);
        let data = noprop::sample_bytes_vec(ctx, data_len);
        let body = MultipartBuilder::with_boundary("mixed-boundary")
            .expect("マルチパートのパースは成功するはず (実装バグ)")
            .text_field(&text_name, &text_value)
            .file_field(&file_name, &filename, "application/octet-stream", &data)
            .build();

        let mut parser = MultipartParser::new("mixed-boundary")
            .expect("マルチパートのパースは成功するはず (実装バグ)");
        parser
            .feed(&body)
            .expect("マルチパートのパースは成功するはず (実装バグ)");

        let part1 = parser
            .next_part()
            .expect("結果は存在するはず (実装バグ)")
            .expect("マルチパートのパースは成功するはず (実装バグ)");
        assert_eq!(part1.name(), Some(text_name.as_str()));
        assert!(!part1.is_file());

        let part2 = parser
            .next_part()
            .expect("結果は存在するはず (実装バグ)")
            .expect("マルチパートのパースは成功するはず (実装バグ)");
        assert_eq!(part2.name(), Some(file_name.as_str()));
        assert!(part2.is_file());
        assert_eq!(part2.filename(), Some(filename.as_str()));
        assert_eq!(part2.body(), data.as_slice());
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

/// 複数ファイル
#[test]
fn prop_multipart_multiple_files_roundtrip() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let name1 = valid_field_name(ctx);
        let filename1 = valid_filename(ctx);
        let name2 = valid_field_name(ctx);
        let filename2 = valid_filename(ctx);
        let body = MultipartBuilder::with_boundary("files-boundary")
            .expect("マルチパートのパースは成功するはず (実装バグ)")
            .file_field(&name1, &filename1, "text/plain", b"content1")
            .file_field(&name2, &filename2, "image/png", b"content2")
            .build();

        let mut parser = MultipartParser::new("files-boundary")
            .expect("マルチパートのパースは成功するはず (実装バグ)");
        parser
            .feed(&body)
            .expect("マルチパートのパースは成功するはず (実装バグ)");

        let part1 = parser
            .next_part()
            .expect("結果は存在するはず (実装バグ)")
            .expect("マルチパートのパースは成功するはず (実装バグ)");
        assert_eq!(part1.filename(), Some(filename1.as_str()));

        let part2 = parser
            .next_part()
            .expect("結果は存在するはず (実装バグ)")
            .expect("マルチパートのパースは成功するはず (実装バグ)");
        assert_eq!(part2.filename(), Some(filename2.as_str()));
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

/// 空のフィールド
#[test]
fn prop_multipart_empty_value_roundtrip() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let name = valid_field_name(ctx);
        let body = MultipartBuilder::with_boundary("boundary")
            .expect("マルチパートのパースは成功するはず (実装バグ)")
            .text_field(&name, "")
            .build();

        let mut parser = MultipartParser::new("boundary")
            .expect("マルチパートのパースは成功するはず (実装バグ)");
        parser
            .feed(&body)
            .expect("マルチパートのパースは成功するはず (実装バグ)");

        let part = parser
            .next_part()
            .expect("結果は存在するはず (実装バグ)")
            .expect("マルチパートのパースは成功するはず (実装バグ)");
        assert_eq!(part.name(), Some(name.as_str()));
        assert_eq!(part.body_str(), Some(""));
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

/// 空のファイル
#[test]
fn prop_multipart_empty_file_roundtrip() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let name = valid_field_name(ctx);
        let filename = valid_filename(ctx);
        let body = MultipartBuilder::with_boundary("boundary")
            .expect("マルチパートのパースは成功するはず (実装バグ)")
            .file_field(&name, &filename, "application/octet-stream", &[])
            .build();

        let mut parser = MultipartParser::new("boundary")
            .expect("マルチパートのパースは成功するはず (実装バグ)");
        parser
            .feed(&body)
            .expect("マルチパートのパースは成功するはず (実装バグ)");

        let part = parser
            .next_part()
            .expect("結果は存在するはず (実装バグ)")
            .expect("マルチパートのパースは成功するはず (実装バグ)");
        assert_eq!(part.filename(), Some(filename.as_str()));
        assert!(part.body().is_empty());
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

/// `--<boundary>` の直後に CRLF / `--` / SP / HTAB 以外のバイトが続く入力は
/// `InvalidPart` で reject される (RFC 2046 Section 5.1.1 違反)
#[test]
fn prop_multipart_dash_boundary_invalid_byte_is_rejected() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        // CRLF / `-` / SP / HTAB 以外のバイトを生成する
        // 有効バイト率は 1 - 4/256 ≈ 0.984 なので max_attempts は 3 で十分。
        // sample_with_rejection を使うため、このテストでは valid-by-construction の
        // 検証 (rejected_cases == 0) は行わない。
        let invalid_byte = noprop::sample_with_rejection(ctx, 3, |ctx| {
            let b = noprop::sample_u8(ctx);
            if matches!(b, b'\r' | b'-' | b' ' | b'\t') {
                None
            } else {
                Some(b)
            }
        });

        let mut parser =
            MultipartParser::new("b").expect("マルチパートのパースは成功するはず (実装バグ)");
        let mut input: Vec<u8> = b"--b".to_vec();
        input.push(invalid_byte);
        // ダミーの後続データ (boundary 直後判定が走るのに十分な長さを確保)
        input.extend_from_slice(
            b"XContent-Disposition: form-data; name=\"a\"\r\n\r\nhello\r\n--b--\r\n",
        );
        parser
            .feed(&input)
            .expect("マルチパートのパースは成功するはず (実装バグ)");
        let result = parser.next_part();
        assert!(
            matches!(
                result,
                Err(shiguredo_http11::multipart::MultipartError::InvalidPart)
            ),
            "invalid_byte=0x{:02x}: 期待 Err(InvalidPart) / 実際 {:?}",
            invalid_byte,
            result
        );
        Ok(())
    })?;

    Ok(())
}

/// 任意の境界で chunk 分割した入力でも、bulk feed と同じパース結果を得る上に
/// 終端まで feed すれば `is_finished` が true になる
#[test]
fn prop_multipart_chunk_split_roundtrip() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let name1 = valid_field_name(ctx);
        let value1 = valid_text_value(ctx);
        let name2 = valid_field_name(ctx);
        let value2 = valid_text_value(ctx);
        let split = noprop::sample_usize_in(ctx, 1..200);
        let body = MultipartBuilder::with_boundary("boundary")
            .expect("マルチパートのパースは成功するはず (実装バグ)")
            .text_field(&name1, &value1)
            .text_field(&name2, &value2)
            .build();

        // bulk feed
        let mut bulk = MultipartParser::new("boundary")
            .expect("マルチパートのパースは成功するはず (実装バグ)");
        bulk.feed(&body)
            .expect("マルチパートのパースは成功するはず (実装バグ)");
        let mut bulk_parts: Vec<Vec<u8>> = Vec::new();
        while let Some(part) = bulk
            .next_part()
            .expect("マルチパートのパースは成功するはず (実装バグ)")
        {
            bulk_parts.push(part.body().to_vec());
        }
        assert!(bulk.is_finished());

        // chunk-split feed
        let split = split.min(body.len().saturating_sub(1)).max(1);
        let mut split_parser = MultipartParser::new("boundary")
            .expect("マルチパートのパースは成功するはず (実装バグ)");
        split_parser
            .feed(&body[..split])
            .expect("マルチパートのパースは成功するはず (実装バグ)");
        let mut split_parts: Vec<Vec<u8>> = Vec::new();
        loop {
            match split_parser.next_part() {
                Ok(Some(part)) => split_parts.push(part.body().to_vec()),
                Ok(None) => break,
                Err(shiguredo_http11::multipart::MultipartError::Incomplete) => break,
                Err(e) => panic!("予期しないエラー: {:?}", e),
            }
        }
        split_parser
            .feed(&body[split..])
            .expect("マルチパートのパースは成功するはず (実装バグ)");
        while let Some(part) = split_parser
            .next_part()
            .expect("マルチパートのパースは成功するはず (実装バグ)")
        {
            split_parts.push(part.body().to_vec());
        }

        assert_eq!(&bulk_parts, &split_parts);
        assert!(
            split_parser.is_finished(),
            "chunk-split 経路でも close-delimiter まで feed すれば is_finished() == true"
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
