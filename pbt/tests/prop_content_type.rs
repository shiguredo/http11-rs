//! Content-Type のプロパティテスト

use proptest::prelude::*;
use shiguredo_http11::content_type::ContentType;

// ========================================
// Strategy 定義
// ========================================

// 有効なトークン文字列 (RFC 9110 Section 5.6.2)
fn valid_token() -> impl Strategy<Value = String> {
    // トークン文字: !#$%&'*+-.0-9A-Z^_`a-z|~
    "[a-zA-Z0-9!#$%&'*+.^_`|~-]{1,8}".prop_map(|s| s)
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
// 基本的なパースのテスト
// ========================================

// Content-Type パースのラウンドトリップ
proptest! {
    #[test]
    fn prop_content_type_roundtrip(media_type in "[a-z]{1,16}", subtype in "[a-z0-9-]{1,16}") {
        let ct_str = format!("{}/{}", media_type, subtype);
        let ct = ContentType::parse(&ct_str).unwrap();
        prop_assert_eq!(ct.media_type(), media_type.as_str());
        prop_assert_eq!(ct.subtype(), subtype.as_str());
    }
}

// トークン文字を使用したメディアタイプ
proptest! {
    #[test]
    fn prop_content_type_token_chars(
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
    fn prop_content_type_mime_type(media_type in "[a-z]{1,8}", subtype in "[a-z]{1,8}") {
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
    fn prop_content_type_with_charset(
        media_type in "[a-z]{1,8}",
        subtype in "[a-z0-9-]{1,8}",
        charset in "[a-zA-Z0-9-]{1,16}"
    ) {
        let ct_str = format!("{}/{}; charset={}", media_type, subtype, charset);
        let ct = ContentType::parse(&ct_str).unwrap();
        prop_assert_eq!(ct.charset(), Some(charset.as_str()));
    }
}

// 引用符付き charset
proptest! {
    #[test]
    fn prop_content_type_quoted_charset(charset in "[a-zA-Z0-9-]{1,16}") {
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
    fn prop_content_type_multipart_boundary(boundary in boundary_value()) {
        let ct_str = format!("multipart/form-data; boundary={}", boundary);
        let ct = ContentType::parse(&ct_str).unwrap();
        prop_assert!(ct.is_form_data());
        prop_assert_eq!(ct.boundary(), Some(boundary.as_str()));
    }
}

// 引用符付き boundary
proptest! {
    #[test]
    fn prop_content_type_quoted_boundary(boundary in boundary_value()) {
        let ct_str = format!("multipart/form-data; boundary=\"{}\"", boundary);
        let ct = ContentType::parse(&ct_str).unwrap();
        prop_assert_eq!(ct.boundary(), Some(boundary.as_str()));
    }
}

// multipart/mixed
proptest! {
    #[test]
    fn prop_content_type_multipart_mixed(boundary in boundary_value()) {
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
    fn prop_content_type_multiple_params(
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
    fn prop_content_type_params_order(
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
    fn prop_content_type_custom_param(
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
    fn prop_content_type_quoted_value_with_space(
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
    fn prop_content_type_quoted_value_with_semicolon(
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
    fn prop_content_type_escaped_quote(word in "[a-z]{1,8}") {
        let ct_str = format!("text/plain; name=\"{}\\\"{}\"", word, word);
        let ct = ContentType::parse(&ct_str).unwrap();
        let expected = format!("{}\"{}",word, word);
        prop_assert_eq!(ct.parameter("name"), Some(expected.as_str()));
    }
}

// エスケープされたバックスラッシュ
proptest! {
    #[test]
    fn prop_content_type_escaped_backslash(word in "[a-z]{1,8}") {
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
    fn prop_content_type_display_roundtrip(
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
    fn prop_content_type_display_with_param_roundtrip(
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
    fn prop_content_type_display_quoted_roundtrip(
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
    fn prop_content_type_is_text(subtype in "[a-z]{1,8}") {
        let ct = ContentType::parse(&format!("text/{}", subtype)).unwrap();
        prop_assert!(ct.is_text());
    }
}

// is_multipart()
proptest! {
    #[test]
    fn prop_content_type_is_multipart(subtype in "[a-z]{1,8}") {
        let ct = ContentType::parse(&format!("multipart/{}", subtype)).unwrap();
        prop_assert!(ct.is_multipart());
    }
}

// ========================================
// 大文字小文字の正規化テスト
// ========================================

// メディアタイプは大文字小文字を正規化
proptest! {
    #[test]
    fn prop_content_type_case_insensitive(
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
    fn prop_content_type_param_case(
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
    fn prop_content_type_trim_whitespace(
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
    fn prop_content_type_param_whitespace(
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

// 不正な文字を含むメディアタイプ
proptest! {
    #[test]
    fn prop_content_type_invalid_media_type_char(
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
// new() と with_parameter() のテスト
// ========================================

proptest! {
    #[test]
    fn prop_content_type_new_with_params(
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
    fn prop_content_type_parameters_accessor(
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
