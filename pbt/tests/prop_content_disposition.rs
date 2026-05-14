//! Content-Disposition ヘッダーのプロパティテスト

use proptest::prelude::*;
use shiguredo_http11::content_disposition::{ContentDisposition, DispositionType};

// ========================================
// Strategy 定義
// ========================================

// disposition-type
fn disposition_type_str() -> impl Strategy<Value = &'static str> {
    prop_oneof![Just("inline"), Just("attachment"), Just("form-data"),]
}

// 引用符内で使える文字 (qdtext + obs-text の Unicode scalar 拡張)
//
// RFC 9110 Section 5.6.4 の qdtext ABNF (オクテット表現) を、char 単位走査の本実装に
// 合わせて Unicode scalar に拡張解釈する (issue 0059)。
fn qdtext_char() -> impl Strategy<Value = char> {
    prop_oneof![
        Just('\t'),
        Just(' '),
        Just('!'),
        prop::char::range('#', '['), // 0x23-0x5B (引用符とバックスラッシュを除く)
        prop::char::range(']', '~'), // 0x5D-0x7E
        // obs-text を Unicode scalar として opaque 保持する範囲。
        // surrogate (`U+D800..=U+DFFF`) は char 型で構築不能なので、
        // shrink バイアスを surrogate 跨ぎで歪めないため二分割する。
        prop::char::range('\u{80}', '\u{D7FF}'),
        prop::char::range('\u{E000}', '\u{10FFFF}'),
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
// 単純なパースのテスト
// ========================================

// disposition-type のみのラウンドトリップ
proptest! {
    #[test]
    fn prop_content_disposition_type_only_roundtrip(dtype in disposition_type_str()) {
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

// ========================================
// filename パラメータのテスト
// ========================================

// 引用符付き filename のラウンドトリップ
proptest! {
    #[test]
    fn prop_content_disposition_filename_quoted_roundtrip(
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
    fn prop_content_disposition_filename_unquoted_roundtrip(
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
    fn prop_content_disposition_quoted_string_content(
        dtype in disposition_type_str(),
        content in quoted_string_content()
    ) {
        let input = format!("{}; filename=\"{}\"", dtype, content);
        let cd = ContentDisposition::parse(&input).unwrap();

        prop_assert_eq!(cd.filename(), Some(content.as_str()));
    }
}

// ========================================
// filename* (RFC 8187 ext-value) のテスト
// ========================================

// filename* のラウンドトリップ
proptest! {
    #[test]
    fn prop_content_disposition_filename_ext_roundtrip(
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
    fn prop_content_disposition_filename_ext_priority(
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

// ========================================
// name パラメータのテスト (form-data)
// ========================================

// form-data with name のラウンドトリップ
proptest! {
    #[test]
    fn prop_content_disposition_form_data_name_roundtrip(name in ascii_filename()) {
        let input = format!("form-data; name=\"{}\"", name);
        let cd = ContentDisposition::parse(&input).unwrap();

        prop_assert!(cd.is_form_data());
        prop_assert_eq!(cd.name(), Some(name.as_str()));
    }
}

// form-data with name and filename
proptest! {
    #[test]
    fn prop_content_disposition_form_data_name_and_filename(
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
    fn prop_content_disposition_custom_parameter(
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

// ========================================
// ビルダーパターンのテスト
// ========================================

// ContentDisposition::new + with_filename
proptest! {
    #[test]
    fn prop_content_disposition_builder_filename(filename in ascii_filename()) {
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
    fn prop_content_disposition_builder_filename_ext(filename in utf8_filename()) {
        let cd = ContentDisposition::new(DispositionType::Attachment)
            .with_filename_ext(&filename);

        prop_assert_eq!(cd.filename_ext(), Some(filename.as_str()));
        prop_assert_eq!(cd.filename(), Some(filename.as_str()));
    }
}

// ContentDisposition::new + with_name
proptest! {
    #[test]
    fn prop_content_disposition_builder_name(name in ascii_filename()) {
        let cd = ContentDisposition::new(DispositionType::FormData)
            .with_name(&name);

        prop_assert!(cd.is_form_data());
        prop_assert_eq!(cd.name(), Some(name.as_str()));
    }
}

// 複合ビルダー
proptest! {
    #[test]
    fn prop_content_disposition_builder_combined(
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
    fn prop_content_disposition_display_filename(fname in ascii_filename()) {
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
    fn prop_content_disposition_display_form_data(
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
    fn prop_content_disposition_display_filename_ext(filename in utf8_filename()) {
        let cd = ContentDisposition::new(DispositionType::Attachment)
            .with_filename_ext(&filename);
        let display = cd.to_string();

        prop_assert!(display.starts_with("attachment"));
        prop_assert!(display.contains("filename*=UTF-8''"));
    }
}

// ========================================
// パラメータ数の hard cap (issue 0047)
// ========================================

proptest! {
    /// 33..=200 個のパラメータは TooManyParameters を返す
    #[test]
    fn prop_content_disposition_too_many_params(count in 33usize..=200) {
        let mut s = String::from("attachment");
        for i in 0..count {
            s.push_str(&format!("; p{}=\"v\"", i));
        }
        let result = shiguredo_http11::content_disposition::ContentDisposition::parse(&s);
        prop_assert!(matches!(
            result,
            Err(shiguredo_http11::content_disposition::ContentDispositionError::TooManyParameters)
        ));
    }
}

proptest! {
    /// 0..=32 個のパラメータは正常に parse される
    #[test]
    fn prop_content_disposition_at_most_32_params_ok(count in 0usize..=32) {
        let mut s = String::from("attachment");
        for i in 0..count {
            s.push_str(&format!("; p{}=\"v\"", i));
        }
        let result = shiguredo_http11::content_disposition::ContentDisposition::parse(&s);
        prop_assert!(result.is_ok(), "count={}: {:?}", count, result);
    }
}
