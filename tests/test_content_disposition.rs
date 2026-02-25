//! Content-Disposition のユニットテスト

use shiguredo_http11::content_disposition::{
    ContentDisposition, ContentDispositionError, DispositionType,
};

// ========================================
// ContentDispositionError のテスト
// ========================================

#[test]
fn test_content_disposition_error_display() {
    let errors = [
        (ContentDispositionError::Empty, "empty content-disposition"),
        (
            ContentDispositionError::InvalidFormat,
            "invalid content-disposition format",
        ),
        (
            ContentDispositionError::InvalidDispositionType,
            "invalid disposition-type",
        ),
        (
            ContentDispositionError::InvalidParameter,
            "invalid parameter",
        ),
        (
            ContentDispositionError::InvalidExtValue,
            "invalid ext-value encoding",
        ),
        (
            ContentDispositionError::DuplicateParameter("filename".to_string()),
            "duplicate parameter: filename",
        ),
    ];

    for (error, expected) in errors {
        assert_eq!(error.to_string(), expected);
    }
}

// ========================================
// DispositionType のテスト
// ========================================

#[test]
fn test_disposition_type_display() {
    assert_eq!(DispositionType::Inline.to_string(), "inline");
    assert_eq!(DispositionType::Attachment.to_string(), "attachment");
    assert_eq!(DispositionType::FormData.to_string(), "form-data");
}

// ========================================
// filename* (RFC 5987 ext-value) のテスト
// ========================================

#[test]
fn test_content_disposition_filename_ext_non_utf8_rejected() {
    let result = ContentDisposition::parse("attachment; filename*=ISO-8859-1''test.txt");
    assert!(matches!(
        result,
        Err(ContentDispositionError::InvalidExtValue)
    ));

    let result = ContentDisposition::parse("attachment; filename*=ASCII''test.txt");
    assert!(matches!(
        result,
        Err(ContentDispositionError::InvalidExtValue)
    ));
}

#[test]
fn test_content_disposition_filename_ext_format_errors() {
    // シングルクォートがない
    let result = ContentDisposition::parse("attachment; filename*=UTF-8test.txt");
    assert!(matches!(
        result,
        Err(ContentDispositionError::InvalidExtValue)
    ));

    // シングルクォートが1つだけ
    let result = ContentDisposition::parse("attachment; filename*=UTF-8'test.txt");
    assert!(matches!(
        result,
        Err(ContentDispositionError::InvalidExtValue)
    ));
}

#[test]
fn test_content_disposition_incomplete_percent_encoding() {
    // % の後に1文字しかない
    let result = ContentDisposition::parse("attachment; filename*=UTF-8''test%2");
    assert!(matches!(
        result,
        Err(ContentDispositionError::InvalidExtValue)
    ));

    // % の後に何もない
    let result = ContentDisposition::parse("attachment; filename*=UTF-8''test%");
    assert!(matches!(
        result,
        Err(ContentDispositionError::InvalidExtValue)
    ));

    // 不正な16進数
    let result = ContentDisposition::parse("attachment; filename*=UTF-8''test%GG");
    assert!(matches!(
        result,
        Err(ContentDispositionError::InvalidExtValue)
    ));
}

#[test]
fn test_content_disposition_invalid_utf8_sequence() {
    // 無効な UTF-8 バイトシーケンス
    let result = ContentDisposition::parse("attachment; filename*=UTF-8''%FF%FE");
    assert!(matches!(
        result,
        Err(ContentDispositionError::InvalidExtValue)
    ));
}

// ========================================
// エスケープ処理のテスト
// ========================================

#[test]
fn test_content_disposition_escape_quote_in_filename() {
    // パース時のエスケープ解除
    let cd = ContentDisposition::parse(r#"attachment; filename="file\"name.txt""#).unwrap();
    assert_eq!(cd.filename(), Some("file\"name.txt"));

    // Display 時のエスケープ
    let cd = ContentDisposition::new(DispositionType::Attachment).with_filename("file\"name.txt");
    let display = cd.to_string();
    assert!(display.contains(r#"filename="file\"name.txt""#));
}

#[test]
fn test_content_disposition_escape_backslash_in_filename() {
    // パース時のエスケープ解除
    let cd = ContentDisposition::parse(r#"attachment; filename="path\\file.txt""#).unwrap();
    assert_eq!(cd.filename(), Some("path\\file.txt"));

    // Display 時のエスケープ
    let cd = ContentDisposition::new(DispositionType::Attachment).with_filename("path\\file.txt");
    let display = cd.to_string();
    assert!(display.contains(r#"filename="path\\file.txt""#));
}

#[test]
fn test_content_disposition_incomplete_escape() {
    // バックスラッシュで終わる
    let result = ContentDisposition::parse(r#"attachment; filename="test\"#);
    assert!(matches!(
        result,
        Err(ContentDispositionError::InvalidParameter)
    ));
}

// ========================================
// パースエラーのテスト
// ========================================

#[test]
fn test_content_disposition_parse_errors() {
    // 空
    assert!(matches!(
        ContentDisposition::parse(""),
        Err(ContentDispositionError::Empty)
    ));
    assert!(matches!(
        ContentDisposition::parse("   "),
        Err(ContentDispositionError::Empty)
    ));

    // RFC 6266 Section 4.1: 拡張 disposition-type は有効なトークンであれば受け入れられる
    // "unknown" と "download" は有効なトークンなので Unknown バリアントとしてパースされる
    let cd = ContentDisposition::parse("unknown").unwrap();
    assert_eq!(
        cd.disposition_type(),
        DispositionType::Unknown("unknown".to_string())
    );

    let cd = ContentDisposition::parse("download").unwrap();
    assert_eq!(
        cd.disposition_type(),
        DispositionType::Unknown("download".to_string())
    );

    // 不正な disposition-type: トークンとして無効な文字を含む
    assert!(matches!(
        ContentDisposition::parse("hello world"),
        Err(ContentDispositionError::InvalidDispositionType)
    ));
    assert!(matches!(
        ContentDisposition::parse("type@invalid"),
        Err(ContentDispositionError::InvalidDispositionType)
    ));
}

// ========================================
// 境界値テスト
// ========================================

#[test]
fn test_content_disposition_empty_parameter_parts() {
    // 末尾のセミコロン
    let cd = ContentDisposition::parse("attachment;").unwrap();
    assert!(cd.is_attachment());

    // 連続したセミコロン
    let cd = ContentDisposition::parse("attachment;; filename=\"test.txt\"").unwrap();
    assert_eq!(cd.filename(), Some("test.txt"));
}

#[test]
fn test_content_disposition_parameter_without_equals() {
    let cd = ContentDisposition::parse("attachment; filename").unwrap();
    assert!(cd.is_attachment());
    assert_eq!(cd.filename(), None);
}

// ========================================
// パラメータ名の大文字小文字テスト
// ========================================

#[test]
fn test_content_disposition_parameter_case_insensitive() {
    let cd = ContentDisposition::parse("attachment; FILENAME=\"test.txt\"").unwrap();
    assert_eq!(cd.filename(), Some("test.txt"));

    let cd = ContentDisposition::parse("form-data; NAME=\"field\"").unwrap();
    assert_eq!(cd.name(), Some("field"));
}
