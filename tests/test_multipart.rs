//! multipart のユニットテスト

use shiguredo_http11::multipart::{MultipartBuilder, MultipartError, MultipartParser};

// ========================================
// MultipartError のテスト
// ========================================

#[test]
fn test_multipart_error_display() {
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

// ========================================
// Part 構造体のテスト
// ========================================

// Part::headers のテスト
#[test]
fn test_multipart_part_headers() {
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
fn test_multipart_part_body_str_non_utf8() {
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

// ========================================
// MultipartParser のテスト
// ========================================

// パーサーが完了後に None を返す
#[test]
fn test_multipart_parser_finished_returns_none() {
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
fn test_multipart_parser_empty() {
    let mut parser = MultipartParser::new("boundary");

    // データを feed しないと Incomplete
    assert!(matches!(
        parser.next_part(),
        Err(MultipartError::Incomplete)
    ));
}

// 不正なヘッダー (非 UTF-8)
#[test]
fn test_multipart_parser_invalid_header() {
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
fn test_multipart_parser_end_boundary_only() {
    let body = b"--boundary--\r\n";

    let mut parser = MultipartParser::new("boundary");
    parser.feed(body);

    assert!(parser.next_part().unwrap().is_none());
    assert!(parser.is_finished());
}

// Clone のテスト
#[test]
fn test_multipart_parser_clone() {
    let mut parser = MultipartParser::new("boundary");
    parser.feed(
        b"--boundary\r\nContent-Disposition: form-data; name=\"f\"\r\n\r\nval\r\n--boundary--\r\n",
    );

    let cloned = parser.clone();
    assert!(!cloned.is_finished());
}
