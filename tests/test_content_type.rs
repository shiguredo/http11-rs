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

mod helpers;

// ========================================
// quoted-string 文字種検証 (RFC 9110 Section 5.6.4 / 5.5)
// ========================================

// CR / LF / NUL / 他の CTL を含む quoted-string / quoted-pair が reject される
#[test]
fn test_content_type_quoted_string_rejects_ctl() {
    for &code in helpers::quoted_string::ALL_CTLS_EXCEPT_HTAB {
        let c = char::from_u32(code).unwrap();
        // qdtext 経路
        assert_eq!(
            ContentType::parse(&format!("text/html; charset=\"{c}\"")),
            Err(ContentTypeError::InvalidParameter),
            "qdtext で CTL U+{code:04X} が reject されない",
        );
        // quoted-pair 経路
        assert_eq!(
            ContentType::parse(&format!("text/html; charset=\"\\{c}\"")),
            Err(ContentTypeError::InvalidParameter),
            "quoted-pair で CTL U+{code:04X} が reject されない",
        );
    }

    // 中間に CTL を置いた `"\rabc"` 形式でも文字種エラーが先に検出される
    // (`parse_parameters` は値を `trim()` するため末尾だけの `\r\n` は届かない)
    assert_eq!(
        ContentType::parse("text/html; charset=\"\rabc\""),
        Err(ContentTypeError::InvalidParameter),
    );
}

// 空 quoted-string `""` の Display ラウンドトリップが破綻しない
// (`needs_quoting("")` を `true` に修正したリグレッション防止)
#[test]
fn test_content_type_empty_quoted_value_roundtrip() {
    let ct = ContentType::parse("text/plain; ext=\"\"").unwrap();
    assert_eq!(ct.parameter("ext"), Some(""));

    let displayed = ct.to_string();
    assert!(displayed.contains("ext=\"\""), "Display 出力 {displayed:?}");
    let reparsed = ContentType::parse(&displayed).unwrap();
    assert_eq!(reparsed.parameter("ext"), Some(""));
}

// ========================================
// NBSP は OWS ではないことの検証 (RFC 9110 Section 5.6.3)
// ========================================

#[test]
fn test_content_type_nbsp_not_stripped_as_ows() {
    // NBSP は OWS ではないため除去されず、media type の検証で失敗する
    let result = ContentType::parse("\u{00A0}text/html");
    assert!(result.is_err());
}

#[test]
fn test_content_type_trailing_nbsp_not_stripped() {
    // 末尾の NBSP も OWS として除去されない
    let result = ContentType::parse("text/html\u{00A0}");
    assert!(result.is_err());
}

#[test]
fn test_content_type_sp_htab_stripped_as_ows() {
    // SP と HTAB は OWS として正しく除去される
    let ct = ContentType::parse(" \ttext/html\t ").unwrap();
    assert_eq!(ct.media_type(), "text");
    assert_eq!(ct.subtype(), "html");
}

// ========================================
// src/content_type.rs のインラインテストを移動
// ========================================

#[test]
fn test_parse_simple() {
    let ct = ContentType::parse("text/html").unwrap();
    assert_eq!(ct.media_type(), "text");
    assert_eq!(ct.subtype(), "html");
    assert_eq!(ct.mime_type(), "text/html");
    assert!(ct.parameters().is_empty());
}

#[test]
fn test_parse_with_charset() {
    let ct = ContentType::parse("text/html; charset=utf-8").unwrap();
    assert_eq!(ct.media_type(), "text");
    assert_eq!(ct.subtype(), "html");
    assert_eq!(ct.charset(), Some("utf-8"));
}

#[test]
fn test_parse_with_quoted_charset() {
    let ct = ContentType::parse("text/html; charset=\"utf-8\"").unwrap();
    assert_eq!(ct.charset(), Some("utf-8"));
}

#[test]
fn test_parse_multipart() {
    let ct = ContentType::parse("multipart/form-data; boundary=----WebKitFormBoundary").unwrap();
    assert!(ct.is_form_data());
    assert_eq!(ct.boundary(), Some("----WebKitFormBoundary"));
}

#[test]
fn test_parse_case_insensitive() {
    let ct = ContentType::parse("TEXT/HTML; CHARSET=UTF-8").unwrap();
    assert_eq!(ct.media_type(), "text");
    assert_eq!(ct.subtype(), "html");
    assert_eq!(ct.charset(), Some("UTF-8")); // 値は大文字小文字を保持
}

#[test]
fn test_parse_multiple_parameters() {
    let ct = ContentType::parse("text/plain; charset=utf-8; boundary=something").unwrap();
    assert_eq!(ct.charset(), Some("utf-8"));
    assert_eq!(ct.boundary(), Some("something"));
}

#[test]
fn test_parse_json() {
    let ct = ContentType::parse("application/json").unwrap();
    assert!(ct.is_json());
}

#[test]
fn test_parse_form_urlencoded() {
    let ct = ContentType::parse("application/x-www-form-urlencoded").unwrap();
    assert!(ct.is_form_urlencoded());
}

#[test]
fn test_parse_with_spaces() {
    let ct = ContentType::parse("  text/html  ;  charset = utf-8  ").unwrap();
    assert_eq!(ct.media_type(), "text");
    assert_eq!(ct.subtype(), "html");
}

#[test]
fn test_parse_quoted_with_escape() {
    let ct = ContentType::parse("text/plain; name=\"hello\\\"world\"").unwrap();
    assert_eq!(ct.parameter("name"), Some("hello\"world"));
}

#[test]
fn test_parse_empty() {
    assert!(ContentType::parse("").is_err());
}

#[test]
fn test_parse_no_subtype() {
    assert!(ContentType::parse("text").is_err());
}

#[test]
fn test_parse_empty_subtype() {
    assert!(ContentType::parse("text/").is_err());
}

#[test]
fn test_display() {
    let ct = ContentType::new("text", "html").with_parameter("charset", "utf-8");
    assert_eq!(ct.to_string(), "text/html; charset=utf-8");
}

#[test]
fn test_display_quoted() {
    let ct = ContentType::new("text", "plain").with_parameter("name", "hello world");
    assert_eq!(ct.to_string(), "text/plain; name=\"hello world\"");
}

#[test]
fn test_is_text() {
    assert!(ContentType::parse("text/plain").unwrap().is_text());
    assert!(ContentType::parse("text/html").unwrap().is_text());
    assert!(!ContentType::parse("application/json").unwrap().is_text());
}

#[test]
fn test_is_multipart() {
    assert!(
        ContentType::parse("multipart/form-data")
            .unwrap()
            .is_multipart()
    );
    assert!(
        ContentType::parse("multipart/mixed")
            .unwrap()
            .is_multipart()
    );
    assert!(!ContentType::parse("text/plain").unwrap().is_multipart());
}

// 修正 3: パラメータ値のトークン検証 (RFC 9110 Section 5.6.2)

#[test]
fn test_invalid_token_parameter_value() {
    assert!(ContentType::parse("text/plain; charset=hello@world").is_err());
}

#[test]
fn test_invalid_token_parameter_value_space() {
    assert!(ContentType::parse("text/plain; charset=hello world").is_err());
}

#[test]
fn test_valid_token_parameter_value() {
    let ct = ContentType::parse("text/plain; charset=utf-8").unwrap();
    assert_eq!(ct.charset(), Some("utf-8"));
}

#[test]
fn test_valid_token_parameter_value_complex() {
    let ct = ContentType::parse("application/octet-stream; name=file-v1.0_test").unwrap();
    assert_eq!(ct.parameter("name"), Some("file-v1.0_test"));
}

#[test]
fn test_quoted_special_chars() {
    let ct = ContentType::parse("text/plain; charset=\"hello@world\"").unwrap();
    assert_eq!(ct.charset(), Some("hello@world"));
}

#[test]
fn test_empty_token_parameter_value() {
    assert!(ContentType::parse("text/plain; charset=").is_err());
}
