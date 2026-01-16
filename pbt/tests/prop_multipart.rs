//! multipart/form-data のプロパティテスト

use proptest::prelude::*;
use shiguredo_http11::content_type::ContentType;
use shiguredo_http11::multipart::{MultipartBuilder, MultipartError, MultipartParser, Part};

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
// MultipartError のテスト
// ========================================

#[test]
fn multipart_error_display() {
    let errors = [
        (MultipartError::Empty, "empty multipart body"),
        (MultipartError::InvalidBoundary, "invalid boundary"),
        (MultipartError::InvalidHeader, "invalid part header"),
        (MultipartError::InvalidPart, "invalid part"),
        (MultipartError::Incomplete, "incomplete multipart data"),
    ];

    for (error, expected) in errors {
        assert_eq!(error.to_string(), expected);
    }
}

#[test]
fn multipart_error_is_error_trait() {
    let error: Box<dyn std::error::Error> = Box::new(MultipartError::Empty);
    assert_eq!(error.to_string(), "empty multipart body");
}

#[test]
fn multipart_error_clone_eq() {
    let error = MultipartError::InvalidBoundary;
    let cloned = error.clone();
    assert_eq!(error, cloned);
}

// ========================================
// Part 構造体のテスト
// ========================================

// テキストフィールドのラウンドトリップ
proptest! {
    #[test]
    fn multipart_text_field_roundtrip(name in valid_field_name(), value in valid_text_value()) {
        let body = MultipartBuilder::with_boundary("test-boundary")
            .text_field(&name, &value)
            .build();

        let mut parser = MultipartParser::new("test-boundary");
        parser.feed(&body);

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
    fn multipart_multiple_fields_roundtrip(
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
        parser.feed(&body);

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
    fn multipart_file_field_roundtrip(
        name in valid_field_name(),
        filename in valid_filename(),
        data in proptest::collection::vec(any::<u8>(), 0..64)
    ) {
        let body = MultipartBuilder::with_boundary("file-boundary")
            .file_field(&name, &filename, "application/octet-stream", &data)
            .build();

        let mut parser = MultipartParser::new("file-boundary");
        parser.feed(&body);

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
    fn multipart_part_new(name in valid_field_name(), value in "[a-zA-Z0-9]{0,32}") {
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
    fn multipart_part_file(
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
    fn multipart_part_with_content_type(name in valid_field_name(), mime_type in valid_mime_type()) {
        let ct = ContentType::parse(&mime_type).unwrap();
        let part = Part::new(&name)
            .with_body(b"test")
            .with_content_type(ct.clone());

        prop_assert!(part.content_type().is_some());
        prop_assert_eq!(part.content_type().unwrap().media_type(), ct.media_type());
    }
}

// Part::headers のテスト
#[test]
fn multipart_part_headers() {
    // Part を直接作成するのは難しいので、パース経由でテスト
    let body = b"--boundary\r\n\
        Content-Disposition: form-data; name=\"field\"\r\n\
        X-Custom-Header: custom-value\r\n\r\n\
        value\r\n\
        --boundary--\r\n";

    let mut parser = MultipartParser::new("boundary");
    parser.feed(body);

    let part = parser.next_part().unwrap().unwrap();
    assert_eq!(part.name(), Some("field"));
    assert_eq!(part.headers().len(), 1);
    assert_eq!(&part.headers()[0].0, "X-Custom-Header");
    assert_eq!(&part.headers()[0].1, "custom-value");
}

// Part::body_str が非 UTF-8 で None を返す
#[test]
fn multipart_part_body_str_non_utf8() {
    let body = b"--boundary\r\n\
        Content-Disposition: form-data; name=\"field\"\r\n\r\n\
        \xff\xfe\r\n\
        --boundary--\r\n";

    let mut parser = MultipartParser::new("boundary");
    parser.feed(body);

    let part = parser.next_part().unwrap().unwrap();
    assert!(part.body_str().is_none());
    assert!(!part.body().is_empty());
}

// Part Clone と PartialEq
proptest! {
    #[test]
    fn multipart_part_clone_eq(name in valid_field_name()) {
        let part = Part::new(&name).with_body(b"test");
        let cloned = part.clone();

        prop_assert_eq!(part, cloned);
    }
}

// ========================================
// MultipartParser のテスト
// ========================================

// パーサーの is_finished テスト
proptest! {
    #[test]
    fn multipart_parser_is_finished(name in valid_field_name(), value in valid_text_value()) {
        let body = MultipartBuilder::with_boundary("boundary")
            .text_field(&name, &value)
            .build();

        let mut parser = MultipartParser::new("boundary");
        parser.feed(&body);

        prop_assert!(!parser.is_finished());

        let _ = parser.next_part().unwrap();
        let _ = parser.next_part().unwrap(); // None を取得

        prop_assert!(parser.is_finished());
    }
}

// パーサーが完了後に None を返す
#[test]
fn multipart_parser_finished_returns_none() {
    let body = MultipartBuilder::with_boundary("boundary")
        .text_field("field", "value")
        .build();

    let mut parser = MultipartParser::new("boundary");
    parser.feed(&body);

    let _ = parser.next_part().unwrap(); // part を取得
    let _ = parser.next_part().unwrap(); // None で完了

    // 完了後も None を返す
    assert!(parser.next_part().unwrap().is_none());
    assert!(parser.next_part().unwrap().is_none());
}

// 空のパーサー
#[test]
fn multipart_parser_empty() {
    let mut parser = MultipartParser::new("boundary");

    // データを feed しないと Incomplete
    assert!(matches!(
        parser.next_part(),
        Err(MultipartError::Incomplete)
    ));
}

// 不正なヘッダー (非 UTF-8)
#[test]
fn multipart_parser_invalid_header() {
    let body = b"--boundary\r\n\xff\xfe: value\r\n\r\ntest\r\n--boundary--\r\n";

    let mut parser = MultipartParser::new("boundary");
    parser.feed(body);

    assert!(matches!(
        parser.next_part(),
        Err(MultipartError::InvalidHeader)
    ));
}

// 終了境界のみ
#[test]
fn multipart_parser_end_boundary_only() {
    let body = b"--boundary--\r\n";

    let mut parser = MultipartParser::new("boundary");
    parser.feed(body);

    assert!(parser.next_part().unwrap().is_none());
    assert!(parser.is_finished());
}

// 任意のバイト列でパニックしない
proptest! {
    #[test]
    fn multipart_parse_no_panic(data in proptest::collection::vec(any::<u8>(), 0..128)) {
        let mut parser = MultipartParser::new("boundary");
        parser.feed(&data);

        // パニックしなければ OK
        while let Ok(Some(_part)) = parser.next_part() {
            // パースできたらループ継続
        }
    }
}

// 任意の境界でパニックしない
proptest! {
    #[test]
    fn multipart_parser_any_boundary_no_panic(boundary in "[ -~]{0,64}") {
        let mut parser = MultipartParser::new(&boundary);
        parser.feed(b"--test\r\nContent-Disposition: form-data; name=\"f\"\r\n\r\nval\r\n--test--\r\n");

        let _ = parser.next_part();
    }
}

// Clone のテスト
#[test]
fn multipart_parser_clone() {
    let mut parser = MultipartParser::new("boundary");
    parser.feed(
        b"--boundary\r\nContent-Disposition: form-data; name=\"f\"\r\n\r\nval\r\n--boundary--\r\n",
    );

    let cloned = parser.clone();
    assert!(!cloned.is_finished());
}

// ========================================
// MultipartBuilder のテスト
// ========================================

// MultipartBuilder::new のテスト
#[test]
fn multipart_builder_new() {
    let builder = MultipartBuilder::new();

    // ランダムな境界が生成される
    assert!(builder.boundary().starts_with("----FormBoundary"));
    assert!(builder.content_type().contains("multipart/form-data"));
}

// MultipartBuilder::with_boundary のテスト
proptest! {
    #[test]
    fn multipart_builder_with_boundary(boundary in valid_boundary()) {
        let builder = MultipartBuilder::with_boundary(&boundary);

        prop_assert_eq!(builder.boundary(), boundary.as_str());
        prop_assert!(builder.content_type().contains(&boundary));
    }
}

// MultipartBuilder::content_type のテスト
proptest! {
    #[test]
    fn multipart_builder_content_type(boundary in valid_boundary()) {
        let builder = MultipartBuilder::with_boundary(&boundary);
        let content_type = builder.content_type();
        let expected_boundary = format!("boundary={}", boundary);

        prop_assert!(content_type.starts_with("multipart/form-data"));
        prop_assert!(content_type.contains(&expected_boundary));
    }
}

// MultipartBuilder::part のテスト
proptest! {
    #[test]
    fn multipart_builder_part(name in valid_field_name(), value in valid_text_value()) {
        let part = Part::new(&name).with_body(value.as_bytes());
        let body = MultipartBuilder::with_boundary("boundary")
            .part(part)
            .build();

        let mut parser = MultipartParser::new("boundary");
        parser.feed(&body);

        let parsed_part = parser.next_part().unwrap().unwrap();
        prop_assert_eq!(parsed_part.name(), Some(name.as_str()));
        prop_assert_eq!(parsed_part.body_str(), Some(value.as_str()));
    }
}

// MultipartBuilder::Default のテスト
#[test]
fn multipart_builder_default() {
    let builder = MultipartBuilder::default();
    assert!(builder.boundary().starts_with("----FormBoundary"));
}

// ========================================
// ラウンドトリップテスト
// ========================================

// 動的な境界でのラウンドトリップ
proptest! {
    #[test]
    fn multipart_dynamic_boundary_roundtrip(
        boundary in valid_boundary(),
        name in valid_field_name(),
        value in valid_text_value()
    ) {
        let body = MultipartBuilder::with_boundary(&boundary)
            .text_field(&name, &value)
            .build();

        let mut parser = MultipartParser::new(&boundary);
        parser.feed(&body);

        let part = parser.next_part().unwrap().unwrap();
        prop_assert_eq!(part.name(), Some(name.as_str()));
        prop_assert_eq!(part.body_str(), Some(value.as_str()));
    }
}

// テキストとファイルの混合
proptest! {
    #[test]
    fn multipart_mixed_fields_roundtrip(
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
        parser.feed(&body);

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
    fn multipart_multiple_files_roundtrip(
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
        parser.feed(&body);

        let part1 = parser.next_part().unwrap().unwrap();
        prop_assert_eq!(part1.filename(), Some(filename1.as_str()));

        let part2 = parser.next_part().unwrap().unwrap();
        prop_assert_eq!(part2.filename(), Some(filename2.as_str()));
    }
}

// 空のフィールド
proptest! {
    #[test]
    fn multipart_empty_value_roundtrip(name in valid_field_name()) {
        let body = MultipartBuilder::with_boundary("boundary")
            .text_field(&name, "")
            .build();

        let mut parser = MultipartParser::new("boundary");
        parser.feed(&body);

        let part = parser.next_part().unwrap().unwrap();
        prop_assert_eq!(part.name(), Some(name.as_str()));
        prop_assert_eq!(part.body_str(), Some(""));
    }
}

// 空のファイル
proptest! {
    #[test]
    fn multipart_empty_file_roundtrip(name in valid_field_name(), filename in valid_filename()) {
        let body = MultipartBuilder::with_boundary("boundary")
            .file_field(&name, &filename, "application/octet-stream", &[])
            .build();

        let mut parser = MultipartParser::new("boundary");
        parser.feed(&body);

        let part = parser.next_part().unwrap().unwrap();
        prop_assert_eq!(part.filename(), Some(filename.as_str()));
        prop_assert!(part.body().is_empty());
    }
}

// バイナリデータ
proptest! {
    #[test]
    fn multipart_binary_data_roundtrip(
        name in valid_field_name(),
        filename in valid_filename(),
        data in proptest::collection::vec(any::<u8>(), 1..128)
    ) {
        // 境界文字列がデータに含まれないようにする
        prop_assume!(!data.windows(8).any(|w| w == b"boundary"));

        let body = MultipartBuilder::with_boundary("boundary")
            .file_field(&name, &filename, "application/octet-stream", &data)
            .build();

        let mut parser = MultipartParser::new("boundary");
        parser.feed(&body);

        let part = parser.next_part().unwrap().unwrap();
        prop_assert_eq!(part.body(), data.as_slice());
    }
}
