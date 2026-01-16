//! Content-Disposition ヘッダーのプロパティテスト

use proptest::prelude::*;
use shiguredo_http11::content_disposition::{
    ContentDisposition, ContentDispositionError, DispositionType,
};

// ========================================
// Strategy 定義
// ========================================

// disposition-type
fn disposition_type_str() -> impl Strategy<Value = &'static str> {
    prop_oneof![Just("inline"), Just("attachment"), Just("form-data"),]
}

// 大文字小文字混在の disposition-type
fn mixed_case_disposition_type() -> impl Strategy<Value = String> {
    prop_oneof![
        Just("INLINE".to_string()),
        Just("Inline".to_string()),
        Just("ATTACHMENT".to_string()),
        Just("Attachment".to_string()),
        Just("FORM-DATA".to_string()),
        Just("Form-Data".to_string()),
    ]
}

// 引用符内で使える文字 (qdtext)
fn qdtext_char() -> impl Strategy<Value = char> {
    prop_oneof![
        Just('\t'),
        Just(' '),
        Just('!'),
        prop::char::range('#', '['), // 0x23-0x5B (引用符とバックスラッシュを除く)
        prop::char::range(']', '~'), // 0x5D-0x7E
    ]
}

// 引用符付き文字列の内容 (エスケープなし)
fn quoted_string_content() -> impl Strategy<Value = String> {
    proptest::collection::vec(qdtext_char(), 0..32).prop_map(|chars| chars.into_iter().collect())
}

// ASCII ファイル名
fn ascii_filename() -> impl Strategy<Value = String> {
    "[a-zA-Z0-9_.-]{1,32}".prop_map(|s| s)
}

// UTF-8 ファイル名 (日本語を含む)
fn utf8_filename() -> impl Strategy<Value = String> {
    prop_oneof![
        ascii_filename(),
        Just("日本語.txt".to_string()),
        Just("файл.txt".to_string()),
        Just("文件.txt".to_string()),
        Just("ファイル名.pdf".to_string()),
    ]
}

// パーセントエンコードされた UTF-8 値
fn percent_encoded_utf8() -> impl Strategy<Value = (String, String)> {
    utf8_filename().prop_map(|filename| {
        let encoded = encode_ext_value_for_test(&filename);
        (filename, encoded)
    })
}

// テスト用のエンコード関数
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

// ========================================
// ContentDispositionError のテスト
// ========================================

#[test]
fn content_disposition_error_display() {
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
    ];

    for (error, expected) in errors {
        assert_eq!(error.to_string(), expected);
    }
}

#[test]
fn content_disposition_error_is_error_trait() {
    let error: Box<dyn std::error::Error> = Box::new(ContentDispositionError::Empty);
    assert_eq!(error.to_string(), "empty content-disposition");
}

// ========================================
// DispositionType のテスト
// ========================================

#[test]
fn disposition_type_display() {
    assert_eq!(DispositionType::Inline.to_string(), "inline");
    assert_eq!(DispositionType::Attachment.to_string(), "attachment");
    assert_eq!(DispositionType::FormData.to_string(), "form-data");
}

// ========================================
// 単純なパースのテスト
// ========================================

// disposition-type のみのラウンドトリップ
proptest! {
    #[test]
    fn content_disposition_type_only_roundtrip(dtype in disposition_type_str()) {
        let cd = ContentDisposition::parse(dtype).unwrap();

        match dtype {
            "inline" => {
                prop_assert!(cd.is_inline());
                prop_assert_eq!(cd.disposition_type(), DispositionType::Inline);
            }
            "attachment" => {
                prop_assert!(cd.is_attachment());
                prop_assert_eq!(cd.disposition_type(), DispositionType::Attachment);
            }
            "form-data" => {
                prop_assert!(cd.is_form_data());
                prop_assert_eq!(cd.disposition_type(), DispositionType::FormData);
            }
            _ => unreachable!(),
        }

        // Display で正規化される
        let display = cd.to_string();
        prop_assert_eq!(display, dtype);
    }
}

// 大文字小文字混在のパース
proptest! {
    #[test]
    fn content_disposition_case_insensitive(dtype in mixed_case_disposition_type()) {
        let cd = ContentDisposition::parse(&dtype).unwrap();

        // Display は小文字に正規化される
        let display = cd.to_string();
        prop_assert_eq!(display, dtype.to_lowercase());
    }
}

// ========================================
// filename パラメータのテスト
// ========================================

// 引用符付き filename のラウンドトリップ
proptest! {
    #[test]
    fn content_disposition_filename_quoted_roundtrip(
        dtype in disposition_type_str(),
        filename in ascii_filename()
    ) {
        let input = format!("{}; filename=\"{}\"", dtype, filename);
        let cd = ContentDisposition::parse(&input).unwrap();

        prop_assert_eq!(cd.filename(), Some(filename.as_str()));
        prop_assert_eq!(cd.filename_ascii(), Some(filename.as_str()));
    }
}

// 引用符なし filename のパース
proptest! {
    #[test]
    fn content_disposition_filename_unquoted_roundtrip(
        dtype in disposition_type_str(),
        filename in ascii_filename()
    ) {
        let input = format!("{}; filename={}", dtype, filename);
        let cd = ContentDisposition::parse(&input).unwrap();

        prop_assert_eq!(cd.filename(), Some(filename.as_str()));
    }
}

// 特殊文字を含む引用符付き文字列
proptest! {
    #[test]
    fn content_disposition_quoted_string_content(
        dtype in disposition_type_str(),
        content in quoted_string_content()
    ) {
        let input = format!("{}; filename=\"{}\"", dtype, content);
        let cd = ContentDisposition::parse(&input).unwrap();

        prop_assert_eq!(cd.filename(), Some(content.as_str()));
    }
}

// ========================================
// filename* (RFC 5987 ext-value) のテスト
// ========================================

// filename* のラウンドトリップ
proptest! {
    #[test]
    fn content_disposition_filename_ext_roundtrip(
        dtype in disposition_type_str(),
        (original, encoded) in percent_encoded_utf8()
    ) {
        let input = format!("{}; filename*=UTF-8''{}", dtype, encoded);
        let cd = ContentDisposition::parse(&input).unwrap();

        prop_assert_eq!(cd.filename_ext(), Some(original.as_str()));
        prop_assert_eq!(cd.filename(), Some(original.as_str()));
    }
}

// filename* は filename より優先される
proptest! {
    #[test]
    fn content_disposition_filename_ext_priority(
        dtype in disposition_type_str(),
        ascii_name in ascii_filename(),
        (utf8_name, encoded) in percent_encoded_utf8()
    ) {
        let input = format!(
            "{}; filename=\"{}\"; filename*=UTF-8''{}",
            dtype, ascii_name, encoded
        );
        let cd = ContentDisposition::parse(&input).unwrap();

        // filename* が優先
        prop_assert_eq!(cd.filename(), Some(utf8_name.as_str()));
        // 個別アクセスも可能
        prop_assert_eq!(cd.filename_ascii(), Some(ascii_name.as_str()));
        prop_assert_eq!(cd.filename_ext(), Some(utf8_name.as_str()));
    }
}

// UTF-8 以外の charset は拒否される
#[test]
fn content_disposition_filename_ext_non_utf8_rejected() {
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

// ext-value のフォーマットエラー
#[test]
fn content_disposition_filename_ext_format_errors() {
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

// 不完全なパーセントエンコーディング
#[test]
fn content_disposition_incomplete_percent_encoding() {
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

// 不正な UTF-8 シーケンス
#[test]
fn content_disposition_invalid_utf8_sequence() {
    // 無効な UTF-8 バイトシーケンス
    let result = ContentDisposition::parse("attachment; filename*=UTF-8''%FF%FE");
    assert!(matches!(
        result,
        Err(ContentDispositionError::InvalidExtValue)
    ));
}

// ========================================
// name パラメータのテスト (form-data)
// ========================================

// form-data with name のラウンドトリップ
proptest! {
    #[test]
    fn content_disposition_form_data_name_roundtrip(name in ascii_filename()) {
        let input = format!("form-data; name=\"{}\"", name);
        let cd = ContentDisposition::parse(&input).unwrap();

        prop_assert!(cd.is_form_data());
        prop_assert_eq!(cd.name(), Some(name.as_str()));
    }
}

// form-data with name and filename
proptest! {
    #[test]
    fn content_disposition_form_data_name_and_filename(
        name in ascii_filename(),
        filename in ascii_filename()
    ) {
        let input = format!("form-data; name=\"{}\"; filename=\"{}\"", name, filename);
        let cd = ContentDisposition::parse(&input).unwrap();

        prop_assert!(cd.is_form_data());
        prop_assert_eq!(cd.name(), Some(name.as_str()));
        prop_assert_eq!(cd.filename(), Some(filename.as_str()));
    }
}

// ========================================
// カスタムパラメータのテスト
// ========================================

// カスタムパラメータの取得
proptest! {
    #[test]
    fn content_disposition_custom_parameter(
        dtype in disposition_type_str(),
        param_name in "[a-z]{1,8}",
        param_value in ascii_filename()
    ) {
        // 予約済みパラメータ名を除外
        prop_assume!(!["filename", "name"].contains(&param_name.as_str()));

        let input = format!("{}; {}=\"{}\"", dtype, param_name, param_value);
        let cd = ContentDisposition::parse(&input).unwrap();

        prop_assert_eq!(cd.parameter(&param_name), Some(param_value.as_str()));
    }
}

// パラメータ名は大文字小文字を区別しない
#[test]
fn content_disposition_parameter_case_insensitive() {
    let cd = ContentDisposition::parse("attachment; FILENAME=\"test.txt\"").unwrap();
    assert_eq!(cd.filename(), Some("test.txt"));

    let cd = ContentDisposition::parse("form-data; NAME=\"field\"").unwrap();
    assert_eq!(cd.name(), Some("field"));
}

// ========================================
// ビルダーパターンのテスト
// ========================================

// ContentDisposition::new + with_filename
proptest! {
    #[test]
    fn content_disposition_builder_filename(filename in ascii_filename()) {
        let cd = ContentDisposition::new(DispositionType::Attachment)
            .with_filename(&filename);

        prop_assert!(cd.is_attachment());
        prop_assert_eq!(cd.filename_ascii(), Some(filename.as_str()));
        prop_assert_eq!(cd.filename(), Some(filename.as_str()));
    }
}

// ContentDisposition::new + with_filename_ext
proptest! {
    #[test]
    fn content_disposition_builder_filename_ext(filename in utf8_filename()) {
        let cd = ContentDisposition::new(DispositionType::Attachment)
            .with_filename_ext(&filename);

        prop_assert_eq!(cd.filename_ext(), Some(filename.as_str()));
        prop_assert_eq!(cd.filename(), Some(filename.as_str()));
    }
}

// ContentDisposition::new + with_name
proptest! {
    #[test]
    fn content_disposition_builder_name(name in ascii_filename()) {
        let cd = ContentDisposition::new(DispositionType::FormData)
            .with_name(&name);

        prop_assert!(cd.is_form_data());
        prop_assert_eq!(cd.name(), Some(name.as_str()));
    }
}

// 複合ビルダー
proptest! {
    #[test]
    fn content_disposition_builder_combined(
        name in ascii_filename(),
        ascii_fn in ascii_filename(),
        utf8_fn in utf8_filename()
    ) {
        let cd = ContentDisposition::new(DispositionType::FormData)
            .with_name(&name)
            .with_filename(&ascii_fn)
            .with_filename_ext(&utf8_fn);

        prop_assert!(cd.is_form_data());
        prop_assert_eq!(cd.name(), Some(name.as_str()));
        prop_assert_eq!(cd.filename_ascii(), Some(ascii_fn.as_str()));
        prop_assert_eq!(cd.filename_ext(), Some(utf8_fn.as_str()));
        // filename() は filename* を優先
        prop_assert_eq!(cd.filename(), Some(utf8_fn.as_str()));
    }
}

// ========================================
// Display のテスト
// ========================================

// attachment + filename の Display
proptest! {
    #[test]
    fn content_disposition_display_filename(fname in ascii_filename()) {
        let cd = ContentDisposition::new(DispositionType::Attachment)
            .with_filename(&fname);
        let display = cd.to_string();

        prop_assert!(display.starts_with("attachment"));
        let expected = format!("filename=\"{}\"", fname);
        prop_assert!(display.contains(&expected));
    }
}

// form-data + name + filename の Display
proptest! {
    #[test]
    fn content_disposition_display_form_data(
        nm in ascii_filename(),
        fname in ascii_filename()
    ) {
        let cd = ContentDisposition::new(DispositionType::FormData)
            .with_name(&nm)
            .with_filename(&fname);
        let display = cd.to_string();

        prop_assert!(display.starts_with("form-data"));
        let expected_name = format!("name=\"{}\"", nm);
        let expected_filename = format!("filename=\"{}\"", fname);
        prop_assert!(display.contains(&expected_name));
        prop_assert!(display.contains(&expected_filename));
    }
}

// filename* の Display (パーセントエンコーディング)
proptest! {
    #[test]
    fn content_disposition_display_filename_ext(filename in utf8_filename()) {
        let cd = ContentDisposition::new(DispositionType::Attachment)
            .with_filename_ext(&filename);
        let display = cd.to_string();

        prop_assert!(display.starts_with("attachment"));
        prop_assert!(display.contains("filename*=UTF-8''"));
    }
}

// ========================================
// エスケープ処理のテスト
// ========================================

// 引用符を含む filename
#[test]
fn content_disposition_escape_quote_in_filename() {
    // パース時のエスケープ解除
    let cd = ContentDisposition::parse(r#"attachment; filename="file\"name.txt""#).unwrap();
    assert_eq!(cd.filename(), Some("file\"name.txt"));

    // Display 時のエスケープ
    let cd = ContentDisposition::new(DispositionType::Attachment).with_filename("file\"name.txt");
    let display = cd.to_string();
    assert!(display.contains(r#"filename="file\"name.txt""#));
}

// バックスラッシュを含む filename
#[test]
fn content_disposition_escape_backslash_in_filename() {
    // パース時のエスケープ解除
    let cd = ContentDisposition::parse(r#"attachment; filename="path\\file.txt""#).unwrap();
    assert_eq!(cd.filename(), Some("path\\file.txt"));

    // Display 時のエスケープ
    let cd = ContentDisposition::new(DispositionType::Attachment).with_filename("path\\file.txt");
    let display = cd.to_string();
    assert!(display.contains(r#"filename="path\\file.txt""#));
}

// 不完全なエスケープシーケンス
#[test]
fn content_disposition_incomplete_escape() {
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
fn content_disposition_parse_errors() {
    // 空
    assert!(matches!(
        ContentDisposition::parse(""),
        Err(ContentDispositionError::Empty)
    ));
    assert!(matches!(
        ContentDisposition::parse("   "),
        Err(ContentDispositionError::Empty)
    ));

    // 不正な disposition-type
    assert!(matches!(
        ContentDisposition::parse("unknown"),
        Err(ContentDispositionError::InvalidDispositionType)
    ));
    assert!(matches!(
        ContentDisposition::parse("download"),
        Err(ContentDispositionError::InvalidDispositionType)
    ));
}

// ========================================
// 境界値テスト
// ========================================

// 空のパラメータ部分
#[test]
fn content_disposition_empty_parameter_parts() {
    // 末尾のセミコロン
    let cd = ContentDisposition::parse("attachment;").unwrap();
    assert!(cd.is_attachment());

    // 連続したセミコロン
    let cd = ContentDisposition::parse("attachment;; filename=\"test.txt\"").unwrap();
    assert_eq!(cd.filename(), Some("test.txt"));
}

// = がないパラメータは無視される
#[test]
fn content_disposition_parameter_without_equals() {
    let cd = ContentDisposition::parse("attachment; filename").unwrap();
    assert!(cd.is_attachment());
    assert_eq!(cd.filename(), None);
}

// ========================================
// Clone と PartialEq のテスト
// ========================================

proptest! {
    #[test]
    fn content_disposition_clone_eq(
        dtype in disposition_type_str(),
        filename in prop::option::of(ascii_filename()),
        name in prop::option::of(ascii_filename())
    ) {
        let mut cd = ContentDisposition::new(match dtype {
            "inline" => DispositionType::Inline,
            "attachment" => DispositionType::Attachment,
            "form-data" => DispositionType::FormData,
            _ => unreachable!(),
        });

        if let Some(f) = &filename {
            cd = cd.with_filename(f);
        }
        if let Some(n) = &name {
            cd = cd.with_name(n);
        }

        let cloned = cd.clone();
        prop_assert_eq!(cd, cloned);
    }
}

// ========================================
// is_* ヘルパーメソッドのテスト
// ========================================

proptest! {
    #[test]
    fn content_disposition_type_helpers(dtype in prop_oneof![
        Just(DispositionType::Inline),
        Just(DispositionType::Attachment),
        Just(DispositionType::FormData)
    ]) {
        let cd = ContentDisposition::new(dtype);

        prop_assert_eq!(cd.is_inline(), dtype == DispositionType::Inline);
        prop_assert_eq!(cd.is_attachment(), dtype == DispositionType::Attachment);
        prop_assert_eq!(cd.is_form_data(), dtype == DispositionType::FormData);
    }
}

// ========================================
// no_panic テスト
// ========================================

proptest! {
    #[test]
    fn content_disposition_parse_no_panic(s in "[ -~]{0,128}") {
        let _ = ContentDisposition::parse(&s);
    }
}

proptest! {
    #[test]
    fn content_disposition_parse_with_utf8_no_panic(s in ".*{0,64}") {
        let _ = ContentDisposition::parse(&s);
    }
}
