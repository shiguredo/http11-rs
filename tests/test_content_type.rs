//! Content-Type のユニットテスト

use shiguredo_http11::content_type::{ContentType, ContentTypeError};

// ========================================
// ContentTypeError のテスト
// ========================================

#[test]
fn test_content_type_error_display() {
    let errors = [
        (ContentTypeError::Empty, "empty Content-Type"),
        (ContentTypeError::InvalidMediaType, "invalid media type"),
        (ContentTypeError::InvalidParameter, "invalid parameter"),
        (ContentTypeError::UnterminatedQuote, "unterminated quote"),
    ];

    for (error, expected) in errors {
        assert_eq!(error.to_string(), expected);
    }
}

// ========================================
// is_* メソッドのテスト
// ========================================

// is_json()
#[test]
fn test_content_type_is_json() {
    assert!(ContentType::parse("application/json").unwrap().is_json());
    assert!(ContentType::parse("APPLICATION/JSON").unwrap().is_json());
    assert!(!ContentType::parse("text/json").unwrap().is_json());
    assert!(!ContentType::parse("application/xml").unwrap().is_json());
}

// is_form_data()
#[test]
fn test_content_type_is_form_data() {
    assert!(
        ContentType::parse("multipart/form-data")
            .unwrap()
            .is_form_data()
    );
    assert!(
        ContentType::parse("MULTIPART/FORM-DATA")
            .unwrap()
            .is_form_data()
    );
    assert!(
        !ContentType::parse("multipart/mixed")
            .unwrap()
            .is_form_data()
    );
}

// is_form_urlencoded()
#[test]
fn test_content_type_is_form_urlencoded() {
    assert!(
        ContentType::parse("application/x-www-form-urlencoded")
            .unwrap()
            .is_form_urlencoded()
    );
    assert!(
        ContentType::parse("APPLICATION/X-WWW-FORM-URLENCODED")
            .unwrap()
            .is_form_urlencoded()
    );
    assert!(
        !ContentType::parse("application/json")
            .unwrap()
            .is_form_urlencoded()
    );
}

// ========================================
// エラーケースのテスト
// ========================================

#[test]
fn test_content_type_parse_errors() {
    // 空
    assert!(matches!(
        ContentType::parse(""),
        Err(ContentTypeError::Empty)
    ));
    assert!(matches!(
        ContentType::parse("   "),
        Err(ContentTypeError::Empty)
    ));

    // スラッシュなし
    assert!(matches!(
        ContentType::parse("text"),
        Err(ContentTypeError::InvalidMediaType)
    ));

    // 空のメディアタイプ
    assert!(matches!(
        ContentType::parse("/html"),
        Err(ContentTypeError::InvalidMediaType)
    ));

    // 空のサブタイプ
    assert!(matches!(
        ContentType::parse("text/"),
        Err(ContentTypeError::InvalidMediaType)
    ));

    // 不正な文字を含むメディアタイプ
    assert!(matches!(
        ContentType::parse("te xt/html"),
        Err(ContentTypeError::InvalidMediaType)
    ));

    // 閉じていない引用符
    assert!(matches!(
        ContentType::parse("text/plain; name=\"unclosed"),
        Err(ContentTypeError::UnterminatedQuote)
    ));

    // パラメータに = がない
    assert!(matches!(
        ContentType::parse("text/plain; charset"),
        Err(ContentTypeError::InvalidParameter)
    ));

    // 空のパラメータ名
    assert!(matches!(
        ContentType::parse("text/plain; =value"),
        Err(ContentTypeError::InvalidParameter)
    ));
}

// ========================================
// エッジケースのテスト
// ========================================

#[test]
fn test_content_type_edge_cases() {
    // 末尾のセミコロン
    let ct = ContentType::parse("text/html;").unwrap();
    assert_eq!(ct.mime_type(), "text/html");
    assert!(ct.parameters().is_empty());

    // 複数のセミコロン
    let ct = ContentType::parse("text/html;;;").unwrap();
    assert_eq!(ct.mime_type(), "text/html");

    // 連続するセミコロン
    let ct = ContentType::parse("text/html; ; charset=utf-8").unwrap();
    assert_eq!(ct.charset(), Some("utf-8"));
}

// セミコロンを含む引用符付き値のパース確認
#[test]
fn test_content_type_semicolon_in_quoted_value() {
    // セミコロンを含む引用符付き値
    let ct = ContentType::parse("text/plain; name=\"a;b\"").unwrap();
    assert_eq!(ct.parameter("name"), Some("a;b"));

    // セミコロンを含む値の後に別のパラメータ
    let ct = ContentType::parse("text/plain; name=\"a;b\"; charset=utf-8").unwrap();
    assert_eq!(ct.parameter("name"), Some("a;b"));
    assert_eq!(ct.charset(), Some("utf-8"));
}

// 引用符のみの値
#[test]
fn test_content_type_quote_only_value() {
    let ct = ContentType::parse("text/plain; name=\"\\\"\"").unwrap();
    assert_eq!(ct.parameter("name"), Some("\""));
}

// 空の引用符付き値
#[test]
fn test_content_type_empty_quoted_value() {
    let ct = ContentType::parse("text/plain; name=\"\"").unwrap();
    assert_eq!(ct.parameter("name"), Some(""));
}
