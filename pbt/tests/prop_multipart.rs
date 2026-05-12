//! multipart/form-data のプロパティテスト

use proptest::prelude::*;
use shiguredo_http11::content_type::ContentType;
use shiguredo_http11::multipart::{MultipartBuilder, MultipartParser, Part};

// ========================================
// Strategy 定義
// ========================================

// 有効なフィールド名 (RFC 7578)
fn valid_field_name() -> impl Strategy<Value = String> {
    "[a-zA-Z][a-zA-Z0-9_]{0,15}".prop_map(|s| s)
}

// 有効なファイル名
fn valid_filename() -> impl Strategy<Value = String> {
    "[a-zA-Z0-9_-]{1,16}\\.[a-z]{1,4}".prop_map(|s| s)
}

// 有効なテキスト値
fn valid_text_value() -> impl Strategy<Value = String> {
    "[a-zA-Z0-9 .,!?]{0,64}".prop_map(|s| s)
}

// 有効な境界文字列
fn valid_boundary() -> impl Strategy<Value = String> {
    "[a-zA-Z0-9]{8,32}".prop_map(|s| s)
}

// 有効な MIME タイプ
fn valid_mime_type() -> impl Strategy<Value = String> {
    prop_oneof![
        Just("text/plain".to_string()),
        Just("text/html".to_string()),
        Just("application/json".to_string()),
        Just("application/octet-stream".to_string()),
        Just("image/png".to_string()),
        Just("image/jpeg".to_string()),
    ]
}

// ========================================
// Part 構造体のテスト
// ========================================

// テキストフィールドのラウンドトリップ
proptest! {
    #[test]
    fn prop_multipart_text_field_roundtrip(name in valid_field_name(), value in valid_text_value()) {
        let body = MultipartBuilder::with_boundary("test-boundary")
            .text_field(&name, &value)
            .build();

        let mut parser = MultipartParser::new("test-boundary");
        parser.feed(&body).unwrap();

        let part = parser.next_part().unwrap().unwrap();
        prop_assert_eq!(part.name(), Some(name.as_str()));
        prop_assert_eq!(part.body_str(), Some(value.as_str()));
        prop_assert!(!part.is_file());

        prop_assert!(parser.next_part().unwrap().is_none());
    }
}

// 複数フィールドのラウンドトリップ
proptest! {
    #[test]
    fn prop_multipart_multiple_fields_roundtrip(
        name1 in valid_field_name(),
        value1 in "[a-zA-Z0-9]{0,16}",
        name2 in valid_field_name(),
        value2 in "[a-zA-Z0-9]{0,16}"
    ) {
        let body = MultipartBuilder::with_boundary("boundary")
            .text_field(&name1, &value1)
            .text_field(&name2, &value2)
            .build();

        let mut parser = MultipartParser::new("boundary");
        parser.feed(&body).unwrap();

        let part1 = parser.next_part().unwrap().unwrap();
        prop_assert_eq!(part1.name(), Some(name1.as_str()));
        prop_assert_eq!(part1.body_str(), Some(value1.as_str()));

        let part2 = parser.next_part().unwrap().unwrap();
        prop_assert_eq!(part2.name(), Some(name2.as_str()));
        prop_assert_eq!(part2.body_str(), Some(value2.as_str()));

        prop_assert!(parser.next_part().unwrap().is_none());
    }
}

// ファイルフィールドのラウンドトリップ
proptest! {
    #[test]
    fn prop_multipart_file_field_roundtrip(
        name in valid_field_name(),
        filename in valid_filename(),
        data in proptest::collection::vec(any::<u8>(), 0..64)
    ) {
        let body = MultipartBuilder::with_boundary("file-boundary")
            .file_field(&name, &filename, "application/octet-stream", &data)
            .build();

        let mut parser = MultipartParser::new("file-boundary");
        parser.feed(&body).unwrap();

        let part = parser.next_part().unwrap().unwrap();
        prop_assert_eq!(part.name(), Some(name.as_str()));
        prop_assert_eq!(part.filename(), Some(filename.as_str()));
        prop_assert!(part.is_file());
        prop_assert_eq!(part.body(), data.as_slice());

        prop_assert!(parser.next_part().unwrap().is_none());
    }
}

// Part::new のテスト
proptest! {
    #[test]
    fn prop_multipart_part_new(name in valid_field_name(), value in "[a-zA-Z0-9]{0,32}") {
        let part = Part::new(&name).with_body(value.as_bytes());

        prop_assert_eq!(part.name(), Some(name.as_str()));
        prop_assert_eq!(part.body_str(), Some(value.as_str()));
        prop_assert!(!part.is_file());
        prop_assert!(part.filename().is_none());
        prop_assert!(part.content_disposition().is_some());
    }
}

// Part::file のテスト
proptest! {
    #[test]
    fn prop_multipart_part_file(
        name in valid_field_name(),
        filename in valid_filename(),
        mime_type in valid_mime_type()
    ) {
        let part = Part::file(&name, &filename, &mime_type).with_body(b"content");

        prop_assert_eq!(part.name(), Some(name.as_str()));
        prop_assert_eq!(part.filename(), Some(filename.as_str()));
        prop_assert!(part.is_file());
        prop_assert!(part.content_type().is_some());
        prop_assert!(part.content_disposition().is_some());
    }
}

// Part::with_content_type のテスト
proptest! {
    #[test]
    fn prop_multipart_part_with_content_type(name in valid_field_name(), mime_type in valid_mime_type()) {
        let ct = ContentType::parse(&mime_type).unwrap();
        let part = Part::new(&name)
            .with_body(b"test")
            .with_content_type(ct.clone());

        prop_assert!(part.content_type().is_some());
        prop_assert_eq!(part.content_type().unwrap().media_type(), ct.media_type());
    }
}

// ========================================
// MultipartParser のテスト
// ========================================

// パーサーの is_finished テスト
proptest! {
    #[test]
    fn prop_multipart_parser_is_finished(name in valid_field_name(), value in valid_text_value()) {
        let body = MultipartBuilder::with_boundary("boundary")
            .text_field(&name, &value)
            .build();

        let mut parser = MultipartParser::new("boundary");
        parser.feed(&body).unwrap();

        prop_assert!(!parser.is_finished());

        let _ = parser.next_part().unwrap();
        let _ = parser.next_part().unwrap(); // None を取得

        prop_assert!(parser.is_finished());
    }
}

// ========================================
// MultipartBuilder のテスト
// ========================================

// MultipartBuilder::new のテスト
proptest! {
    #[test]
    fn prop_multipart_builder_new(random_value: u64) {
        let builder = MultipartBuilder::new(random_value);

        // 境界が正しいフォーマットで生成される
        let expected_boundary = format!("----FormBoundary{}", random_value);
        prop_assert_eq!(builder.boundary(), expected_boundary.as_str());
        prop_assert!(builder.content_type().contains("multipart/form-data"));
    }
}

// MultipartBuilder::with_boundary のテスト
proptest! {
    #[test]
    fn prop_multipart_builder_with_boundary(boundary in valid_boundary()) {
        let builder = MultipartBuilder::with_boundary(&boundary);

        prop_assert_eq!(builder.boundary(), boundary.as_str());
        prop_assert!(builder.content_type().contains(&boundary));
    }
}

// MultipartBuilder::content_type のテスト
proptest! {
    #[test]
    fn prop_multipart_builder_content_type(boundary in valid_boundary()) {
        let builder = MultipartBuilder::with_boundary(&boundary);
        let content_type = builder.content_type();
        let expected_boundary = format!("boundary={}", boundary);

        prop_assert!(content_type.starts_with("multipart/form-data"));
        prop_assert!(content_type.contains(&expected_boundary));
    }
}

// ========================================
// ラウンドトリップテスト
// ========================================

// 動的な境界でのラウンドトリップ
proptest! {
    #[test]
    fn prop_multipart_dynamic_boundary_roundtrip(
        boundary in valid_boundary(),
        name in valid_field_name(),
        value in valid_text_value()
    ) {
        let body = MultipartBuilder::with_boundary(&boundary)
            .text_field(&name, &value)
            .build();

        let mut parser = MultipartParser::new(&boundary);
        parser.feed(&body).unwrap();

        let part = parser.next_part().unwrap().unwrap();
        prop_assert_eq!(part.name(), Some(name.as_str()));
        prop_assert_eq!(part.body_str(), Some(value.as_str()));
    }
}

// テキストとファイルの混合
proptest! {
    #[test]
    fn prop_multipart_mixed_fields_roundtrip(
        text_name in valid_field_name(),
        text_value in valid_text_value(),
        file_name in valid_field_name(),
        filename in valid_filename(),
        data in proptest::collection::vec(any::<u8>(), 0..32)
    ) {
        let body = MultipartBuilder::with_boundary("mixed-boundary")
            .text_field(&text_name, &text_value)
            .file_field(&file_name, &filename, "application/octet-stream", &data)
            .build();

        let mut parser = MultipartParser::new("mixed-boundary");
        parser.feed(&body).unwrap();

        let part1 = parser.next_part().unwrap().unwrap();
        prop_assert_eq!(part1.name(), Some(text_name.as_str()));
        prop_assert!(!part1.is_file());

        let part2 = parser.next_part().unwrap().unwrap();
        prop_assert_eq!(part2.name(), Some(file_name.as_str()));
        prop_assert!(part2.is_file());
        prop_assert_eq!(part2.filename(), Some(filename.as_str()));
        prop_assert_eq!(part2.body(), data.as_slice());
    }
}

// 複数ファイル
proptest! {
    #[test]
    fn prop_multipart_multiple_files_roundtrip(
        name1 in valid_field_name(),
        filename1 in valid_filename(),
        name2 in valid_field_name(),
        filename2 in valid_filename()
    ) {
        let body = MultipartBuilder::with_boundary("files-boundary")
            .file_field(&name1, &filename1, "text/plain", b"content1")
            .file_field(&name2, &filename2, "image/png", b"content2")
            .build();

        let mut parser = MultipartParser::new("files-boundary");
        parser.feed(&body).unwrap();

        let part1 = parser.next_part().unwrap().unwrap();
        prop_assert_eq!(part1.filename(), Some(filename1.as_str()));

        let part2 = parser.next_part().unwrap().unwrap();
        prop_assert_eq!(part2.filename(), Some(filename2.as_str()));
    }
}

// 空のフィールド
proptest! {
    #[test]
    fn prop_multipart_empty_value_roundtrip(name in valid_field_name()) {
        let body = MultipartBuilder::with_boundary("boundary")
            .text_field(&name, "")
            .build();

        let mut parser = MultipartParser::new("boundary");
        parser.feed(&body).unwrap();

        let part = parser.next_part().unwrap().unwrap();
        prop_assert_eq!(part.name(), Some(name.as_str()));
        prop_assert_eq!(part.body_str(), Some(""));
    }
}

// 空のファイル
proptest! {
    #[test]
    fn prop_multipart_empty_file_roundtrip(name in valid_field_name(), filename in valid_filename()) {
        let body = MultipartBuilder::with_boundary("boundary")
            .file_field(&name, &filename, "application/octet-stream", &[])
            .build();

        let mut parser = MultipartParser::new("boundary");
        parser.feed(&body).unwrap();

        let part = parser.next_part().unwrap().unwrap();
        prop_assert_eq!(part.filename(), Some(filename.as_str()));
        prop_assert!(part.body().is_empty());
    }
}

// 任意の境界で chunk 分割した入力でも、bulk feed と同じパース結果を得る上に
// 終端まで feed すれば `is_finished()` が true になる (issue 0042)
proptest! {
    #[test]
    fn prop_multipart_chunk_split_roundtrip(
        name1 in valid_field_name(),
        value1 in valid_text_value(),
        name2 in valid_field_name(),
        value2 in valid_text_value(),
        split in 1usize..200,
    ) {
        let body = MultipartBuilder::with_boundary("boundary")
            .text_field(&name1, &value1)
            .text_field(&name2, &value2)
            .build();

        // bulk feed
        let mut bulk = MultipartParser::new("boundary");
        bulk.feed(&body).unwrap();
        let mut bulk_parts: Vec<Vec<u8>> = Vec::new();
        while let Some(part) = bulk.next_part().unwrap() {
            bulk_parts.push(part.body().to_vec());
        }
        prop_assert!(bulk.is_finished());

        // chunk-split feed
        let split = split.min(body.len().saturating_sub(1)).max(1);
        let mut split_parser = MultipartParser::new("boundary");
        split_parser.feed(&body[..split]).unwrap();
        let mut split_parts: Vec<Vec<u8>> = Vec::new();
        loop {
            match split_parser.next_part() {
                Ok(Some(part)) => split_parts.push(part.body().to_vec()),
                Ok(None) => break,
                Err(shiguredo_http11::multipart::MultipartError::Incomplete) => break,
                Err(e) => prop_assert!(false, "予期しないエラー: {:?}", e),
            }
        }
        split_parser.feed(&body[split..]).unwrap();
        while let Some(part) = split_parser.next_part().unwrap() {
            split_parts.push(part.body().to_vec());
        }

        prop_assert_eq!(&bulk_parts, &split_parts);
        prop_assert!(
            split_parser.is_finished(),
            "chunk-split 経路でも close-delimiter まで feed すれば is_finished() == true"
        );
    }
}
