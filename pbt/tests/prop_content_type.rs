//! Content-Type のプロパティテスト

use proptest::prelude::*;
use shiguredo_http11::content_type::{ContentType, ContentTypeError};

// ========================================
// Strategy 定義
// ========================================

// 有効なトークン文字列 (RFC 7230)
fn valid_token() -> impl Strategy<Value = String> {
    // トークン文字: !#$%&'*+-.0-9A-Z^_`a-z|~
    "[a-zA-Z0-9!#$%&'*+.^_`|~-]{1,8}".prop_map(|s| s)
}

// メディアタイプ (一般的なもの)
fn common_media_type() -> impl Strategy<Value = &'static str> {
    prop_oneof![
        Just("text"),
        Just("application"),
        Just("image"),
        Just("audio"),
        Just("video"),
        Just("multipart"),
        Just("message"),
        Just("font"),
    ]
}

// サブタイプ (一般的なもの)
fn common_subtype() -> impl Strategy<Value = &'static str> {
    prop_oneof![
        Just("plain"),
        Just("html"),
        Just("json"),
        Just("xml"),
        Just("javascript"),
        Just("css"),
        Just("form-data"),
        Just("x-www-form-urlencoded"),
        Just("octet-stream"),
        Just("png"),
        Just("jpeg"),
        Just("gif"),
        Just("mp4"),
        Just("mpeg"),
        Just("mixed"),
    ]
}

// charset 値
fn charset_value() -> impl Strategy<Value = &'static str> {
    prop_oneof![
        Just("utf-8"),
        Just("UTF-8"),
        Just("iso-8859-1"),
        Just("ISO-8859-1"),
        Just("us-ascii"),
        Just("shift_jis"),
        Just("euc-jp"),
    ]
}

// boundary 値 (トークン文字のみ)
fn boundary_value() -> impl Strategy<Value = String> {
    "[a-zA-Z0-9._-]{1,32}".prop_map(|s| s)
}

// 引用符が必要な文字を含む値
fn value_needing_quotes() -> impl Strategy<Value = String> {
    prop_oneof![
        // スペースを含む
        "[a-z]{1,4} [a-z]{1,4}".prop_map(|s| s),
        // セミコロンを含む
        "[a-z]{1,4};[a-z]{1,4}".prop_map(|s| s),
        // カンマを含む
        "[a-z]{1,4},[a-z]{1,4}".prop_map(|s| s),
        // イコールを含む
        "[a-z]{1,4}=[a-z]{1,4}".prop_map(|s| s),
    ]
}

// ========================================
// ContentTypeError のテスト
// ========================================

#[test]
fn content_type_error_display() {
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

#[test]
fn content_type_error_is_error_trait() {
    let error: Box<dyn std::error::Error> = Box::new(ContentTypeError::Empty);
    assert_eq!(error.to_string(), "empty Content-Type");
}

#[test]
fn content_type_error_clone_eq() {
    let error = ContentTypeError::InvalidMediaType;
    let cloned = error.clone();
    assert_eq!(error, cloned);
}

// ========================================
// 基本的なパースのテスト
// ========================================

// Content-Type パースのラウンドトリップ
proptest! {
    #[test]
    fn content_type_roundtrip(media_type in "[a-z]{1,16}", subtype in "[a-z0-9-]{1,16}") {
        let ct_str = format!("{}/{}", media_type, subtype);
        let ct = ContentType::parse(&ct_str).unwrap();
        prop_assert_eq!(ct.media_type(), media_type.as_str());
        prop_assert_eq!(ct.subtype(), subtype.as_str());
    }
}

// 一般的なメディアタイプのパース
proptest! {
    #[test]
    fn content_type_common_types(
        media_type in common_media_type(),
        subtype in common_subtype()
    ) {
        let ct_str = format!("{}/{}", media_type, subtype);
        let ct = ContentType::parse(&ct_str).unwrap();
        prop_assert_eq!(ct.media_type(), media_type);
        prop_assert_eq!(ct.subtype(), subtype);
    }
}

// トークン文字を使用したメディアタイプ
proptest! {
    #[test]
    fn content_type_token_chars(
        media_type in valid_token(),
        subtype in valid_token()
    ) {
        let ct_str = format!("{}/{}", media_type, subtype);
        let ct = ContentType::parse(&ct_str).unwrap();
        let expected_media = media_type.to_ascii_lowercase();
        let expected_sub = subtype.to_ascii_lowercase();
        prop_assert_eq!(ct.media_type(), expected_media.as_str());
        prop_assert_eq!(ct.subtype(), expected_sub.as_str());
    }
}

// mime_type() のテスト
proptest! {
    #[test]
    fn content_type_mime_type(media_type in "[a-z]{1,8}", subtype in "[a-z]{1,8}") {
        let ct = ContentType::parse(&format!("{}/{}", media_type, subtype)).unwrap();
        let expected = format!("{}/{}", media_type, subtype);
        prop_assert_eq!(ct.mime_type(), expected);
    }
}

// ========================================
// charset パラメータのテスト
// ========================================

// charset パラメータ付き Content-Type
proptest! {
    #[test]
    fn content_type_with_charset(
        media_type in "[a-z]{1,8}",
        subtype in "[a-z0-9-]{1,8}",
        charset in "[a-zA-Z0-9-]{1,16}"
    ) {
        let ct_str = format!("{}/{}; charset={}", media_type, subtype, charset);
        let ct = ContentType::parse(&ct_str).unwrap();
        prop_assert_eq!(ct.charset(), Some(charset.as_str()));
    }
}

// 一般的な charset 値
proptest! {
    #[test]
    fn content_type_common_charsets(charset in charset_value()) {
        let ct_str = format!("text/html; charset={}", charset);
        let ct = ContentType::parse(&ct_str).unwrap();
        prop_assert_eq!(ct.charset(), Some(charset));
    }
}

// 引用符付き charset
proptest! {
    #[test]
    fn content_type_quoted_charset(charset in "[a-zA-Z0-9-]{1,16}") {
        let ct_str = format!("text/html; charset=\"{}\"", charset);
        let ct = ContentType::parse(&ct_str).unwrap();
        prop_assert_eq!(ct.charset(), Some(charset.as_str()));
    }
}

// ========================================
// boundary パラメータのテスト
// ========================================

// boundary パラメータ付き multipart
proptest! {
    #[test]
    fn content_type_multipart_boundary(boundary in boundary_value()) {
        let ct_str = format!("multipart/form-data; boundary={}", boundary);
        let ct = ContentType::parse(&ct_str).unwrap();
        prop_assert!(ct.is_form_data());
        prop_assert_eq!(ct.boundary(), Some(boundary.as_str()));
    }
}

// 引用符付き boundary
proptest! {
    #[test]
    fn content_type_quoted_boundary(boundary in boundary_value()) {
        let ct_str = format!("multipart/form-data; boundary=\"{}\"", boundary);
        let ct = ContentType::parse(&ct_str).unwrap();
        prop_assert_eq!(ct.boundary(), Some(boundary.as_str()));
    }
}

// multipart/mixed
proptest! {
    #[test]
    fn content_type_multipart_mixed(boundary in boundary_value()) {
        let ct_str = format!("multipart/mixed; boundary={}", boundary);
        let ct = ContentType::parse(&ct_str).unwrap();
        prop_assert!(ct.is_multipart());
        prop_assert!(!ct.is_form_data());
        prop_assert_eq!(ct.boundary(), Some(boundary.as_str()));
    }
}

// ========================================
// 複数パラメータのテスト
// ========================================

// charset と boundary の両方
proptest! {
    #[test]
    fn content_type_multiple_params(
        charset in "[a-zA-Z0-9-]{1,8}",
        boundary in boundary_value()
    ) {
        let ct_str = format!(
            "multipart/form-data; charset={}; boundary={}",
            charset, boundary
        );
        let ct = ContentType::parse(&ct_str).unwrap();
        prop_assert_eq!(ct.charset(), Some(charset.as_str()));
        prop_assert_eq!(ct.boundary(), Some(boundary.as_str()));
    }
}

// パラメータの順序が異なる場合
proptest! {
    #[test]
    fn content_type_params_order(
        charset in "[a-zA-Z0-9-]{1,8}",
        boundary in boundary_value()
    ) {
        let ct_str = format!(
            "multipart/form-data; boundary={}; charset={}",
            boundary, charset
        );
        let ct = ContentType::parse(&ct_str).unwrap();
        prop_assert_eq!(ct.charset(), Some(charset.as_str()));
        prop_assert_eq!(ct.boundary(), Some(boundary.as_str()));
    }
}

// カスタムパラメータ
proptest! {
    #[test]
    fn content_type_custom_param(
        name in "[a-z]{1,8}",
        value in "[a-zA-Z0-9-]{1,16}"
    ) {
        let ct_str = format!("text/plain; {}={}", name, value);
        let ct = ContentType::parse(&ct_str).unwrap();
        prop_assert_eq!(ct.parameter(&name), Some(value.as_str()));
    }
}

// ========================================
// 引用符付き値のテスト
// ========================================

// スペースを含む引用符付き値
proptest! {
    #[test]
    fn content_type_quoted_value_with_space(
        word1 in "[a-z]{1,4}",
        word2 in "[a-z]{1,4}"
    ) {
        let value = format!("{} {}", word1, word2);
        let ct_str = format!("text/plain; name=\"{}\"", value);
        let ct = ContentType::parse(&ct_str).unwrap();
        prop_assert_eq!(ct.parameter("name"), Some(value.as_str()));
    }
}

// セミコロンを含む引用符付き値
proptest! {
    #[test]
    fn content_type_quoted_value_with_semicolon(
        part1 in "[a-z]{1,4}",
        part2 in "[a-z]{1,4}"
    ) {
        let value = format!("{};{}", part1, part2);
        let ct_str = format!("text/plain; name=\"{}\"", value);
        let ct = ContentType::parse(&ct_str).unwrap();
        prop_assert_eq!(ct.parameter("name"), Some(value.as_str()));
    }
}

// エスケープされた引用符を含む値
proptest! {
    #[test]
    fn content_type_escaped_quote(word in "[a-z]{1,8}") {
        let ct_str = format!("text/plain; name=\"{}\\\"{}\"", word, word);
        let ct = ContentType::parse(&ct_str).unwrap();
        let expected = format!("{}\"{}",word, word);
        prop_assert_eq!(ct.parameter("name"), Some(expected.as_str()));
    }
}

// エスケープされたバックスラッシュ
proptest! {
    #[test]
    fn content_type_escaped_backslash(word in "[a-z]{1,8}") {
        let ct_str = format!("text/plain; name=\"{}\\\\{}\"", word, word);
        let ct = ContentType::parse(&ct_str).unwrap();
        let expected = format!("{}\\{}", word, word);
        prop_assert_eq!(ct.parameter("name"), Some(expected.as_str()));
    }
}

// ========================================
// Display のテスト
// ========================================

// Content-Type 表示のラウンドトリップ
proptest! {
    #[test]
    fn content_type_display_roundtrip(
        media_type in "[a-z]{1,8}",
        subtype in "[a-z0-9-]{1,8}"
    ) {
        let ct = ContentType::new(&media_type, &subtype);
        let displayed = ct.to_string();
        let reparsed = ContentType::parse(&displayed).unwrap();
        prop_assert_eq!(ct.media_type(), reparsed.media_type());
        prop_assert_eq!(ct.subtype(), reparsed.subtype());
    }
}

// パラメータ付き Display のラウンドトリップ
proptest! {
    #[test]
    fn content_type_display_with_param_roundtrip(
        media_type in "[a-z]{1,8}",
        subtype in "[a-z]{1,8}",
        param_value in "[a-zA-Z0-9-]{1,16}"
    ) {
        let ct = ContentType::new(&media_type, &subtype)
            .with_parameter("charset", &param_value);
        let displayed = ct.to_string();
        let reparsed = ContentType::parse(&displayed).unwrap();
        prop_assert_eq!(ct.media_type(), reparsed.media_type());
        prop_assert_eq!(ct.subtype(), reparsed.subtype());
        prop_assert_eq!(ct.charset(), reparsed.charset());
    }
}

// 引用符が必要な値の Display ラウンドトリップ
proptest! {
    #[test]
    fn content_type_display_quoted_roundtrip(
        media_type in "[a-z]{1,8}",
        subtype in "[a-z]{1,8}",
        value in value_needing_quotes()
    ) {
        let ct = ContentType::new(&media_type, &subtype)
            .with_parameter("name", &value);
        let displayed = ct.to_string();
        let reparsed = ContentType::parse(&displayed).unwrap();
        prop_assert_eq!(ct.parameter("name"), reparsed.parameter("name"));
    }
}

// ========================================
// is_* メソッドのテスト
// ========================================

// is_text()
proptest! {
    #[test]
    fn content_type_is_text(subtype in "[a-z]{1,8}") {
        let ct = ContentType::parse(&format!("text/{}", subtype)).unwrap();
        prop_assert!(ct.is_text());
    }
}

// is_text() の否定
proptest! {
    #[test]
    fn content_type_is_not_text(
        media_type in prop_oneof![Just("application"), Just("image"), Just("audio"), Just("video")],
        subtype in "[a-z]{1,8}"
    ) {
        let ct = ContentType::parse(&format!("{}/{}", media_type, subtype)).unwrap();
        prop_assert!(!ct.is_text());
    }
}

// is_json()
#[test]
fn content_type_is_json() {
    assert!(ContentType::parse("application/json").unwrap().is_json());
    assert!(ContentType::parse("APPLICATION/JSON").unwrap().is_json());
    assert!(!ContentType::parse("text/json").unwrap().is_json());
    assert!(!ContentType::parse("application/xml").unwrap().is_json());
}

// is_multipart()
proptest! {
    #[test]
    fn content_type_is_multipart(subtype in "[a-z]{1,8}") {
        let ct = ContentType::parse(&format!("multipart/{}", subtype)).unwrap();
        prop_assert!(ct.is_multipart());
    }
}

// is_form_data()
#[test]
fn content_type_is_form_data() {
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
fn content_type_is_form_urlencoded() {
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
// 大文字小文字の正規化テスト
// ========================================

// メディアタイプは大文字小文字を正規化
proptest! {
    #[test]
    fn content_type_case_insensitive(
        media_type in "[A-Z]{1,8}",
        subtype in "[A-Z0-9-]{1,8}"
    ) {
        let ct_str = format!("{}/{}", media_type, subtype);
        let ct = ContentType::parse(&ct_str).unwrap();
        let expected_media_type = media_type.to_ascii_lowercase();
        let expected_subtype = subtype.to_ascii_lowercase();
        prop_assert_eq!(ct.media_type(), expected_media_type.as_str());
        prop_assert_eq!(ct.subtype(), expected_subtype.as_str());
    }
}

// パラメータ名は大文字小文字を正規化、値は保持
proptest! {
    #[test]
    fn content_type_param_case(
        param_name in "[A-Z]{1,8}",
        param_value in "[A-Za-z0-9]{1,8}"
    ) {
        let ct_str = format!("text/plain; {}={}", param_name, param_value);
        let ct = ContentType::parse(&ct_str).unwrap();
        // パラメータ名は小文字で取得できる
        let lower_name = param_name.to_ascii_lowercase();
        prop_assert_eq!(ct.parameter(&lower_name), Some(param_value.as_str()));
        // 大文字でも取得できる
        prop_assert_eq!(ct.parameter(&param_name), Some(param_value.as_str()));
    }
}

// ========================================
// 空白処理のテスト
// ========================================

// 前後の空白
proptest! {
    #[test]
    fn content_type_trim_whitespace(
        media_type in "[a-z]{1,8}",
        subtype in "[a-z]{1,8}"
    ) {
        let ct_str = format!("  {}/{}  ", media_type, subtype);
        let ct = ContentType::parse(&ct_str).unwrap();
        prop_assert_eq!(ct.media_type(), media_type.as_str());
        prop_assert_eq!(ct.subtype(), subtype.as_str());
    }
}

// パラメータ周りの空白
proptest! {
    #[test]
    fn content_type_param_whitespace(
        param_value in "[a-zA-Z0-9]{1,8}"
    ) {
        let ct_str = format!("text/plain  ;  charset  =  {}", param_value);
        let ct = ContentType::parse(&ct_str).unwrap();
        prop_assert_eq!(ct.charset(), Some(param_value.as_str()));
    }
}

// ========================================
// エラーケースのテスト
// ========================================

#[test]
fn content_type_parse_errors() {
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

// 不正な文字を含むメディアタイプ
proptest! {
    #[test]
    fn content_type_invalid_media_type_char(
        media_type in "[a-z]{1,4}",
        invalid_char in prop_oneof![Just(' '), Just('/'), Just('('), Just(')'), Just('<'), Just('>')],
        subtype in "[a-z]{1,4}"
    ) {
        let ct_str = format!("{}{}{}/{}", media_type, invalid_char, media_type, subtype);
        let result = ContentType::parse(&ct_str);
        prop_assert!(result.is_err());
    }
}

// ========================================
// Clone と PartialEq のテスト
// ========================================

proptest! {
    #[test]
    fn content_type_clone_eq(
        media_type in "[a-z]{1,8}",
        subtype in "[a-z]{1,8}",
        charset in "[a-z]{1,8}"
    ) {
        let ct = ContentType::new(&media_type, &subtype)
            .with_parameter("charset", &charset);
        let cloned = ct.clone();
        prop_assert_eq!(ct, cloned);
    }
}

// ========================================
// new() と with_parameter() のテスト
// ========================================

proptest! {
    #[test]
    fn content_type_new_with_params(
        media_type in "[a-z]{1,8}",
        subtype in "[a-z]{1,8}",
        charset in "[a-z]{1,8}",
        boundary in boundary_value()
    ) {
        let ct = ContentType::new(&media_type, &subtype)
            .with_parameter("charset", &charset)
            .with_parameter("boundary", &boundary);

        prop_assert_eq!(ct.media_type(), media_type.as_str());
        prop_assert_eq!(ct.subtype(), subtype.as_str());
        prop_assert_eq!(ct.charset(), Some(charset.as_str()));
        prop_assert_eq!(ct.boundary(), Some(boundary.as_str()));
    }
}

// parameters() アクセサ
proptest! {
    #[test]
    fn content_type_parameters_accessor(
        media_type in "[a-z]{1,8}",
        subtype in "[a-z]{1,8}",
        value1 in "[a-z]{1,8}",
        value2 in "[a-z]{1,8}"
    ) {
        let ct = ContentType::new(&media_type, &subtype)
            .with_parameter("param1", &value1)
            .with_parameter("param2", &value2);

        let params = ct.parameters();
        prop_assert_eq!(params.len(), 2);
        prop_assert_eq!(&params[0].0, "param1");
        prop_assert_eq!(&params[0].1, &value1);
        prop_assert_eq!(&params[1].0, "param2");
        prop_assert_eq!(&params[1].1, &value2);
    }
}

// ========================================
// no_panic テスト
// ========================================

// 任意の文字列で Content-Type パースがパニックしない
proptest! {
    #[test]
    fn content_type_parse_no_panic(s in "[ -~]{0,64}") {
        let _ = ContentType::parse(&s);
    }
}

// より広範な入力でパニックしない
proptest! {
    #[test]
    fn content_type_parse_no_panic_extended(s in ".{0,128}") {
        let _ = ContentType::parse(&s);
    }
}

// ========================================
// エッジケースのテスト
// ========================================

#[test]
fn content_type_edge_cases() {
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
fn content_type_semicolon_in_quoted_value() {
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
fn content_type_quote_only_value() {
    let ct = ContentType::parse("text/plain; name=\"\\\"\"").unwrap();
    assert_eq!(ct.parameter("name"), Some("\""));
}

// 空の引用符付き値
#[test]
fn content_type_empty_quoted_value() {
    let ct = ContentType::parse("text/plain; name=\"\"").unwrap();
    assert_eq!(ct.parameter("name"), Some(""));
}
