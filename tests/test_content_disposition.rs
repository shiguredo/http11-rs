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
// filename* (RFC 8187 ext-value) のテスト
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

// ========================================
// quoted-string / quoted-pair の CTL 拒否 (RFC 9110 Section 5.6.4)
// ========================================

/// RFC 9110 §5.6.4: quoted-pair = "\" ( HTAB / SP / VCHAR / obs-text )
/// CR / LF / NUL 等の CTL は escape の対象として許容しない
#[test]
fn test_content_disposition_quoted_pair_rejects_crlf() {
    // `\<CR>` を含む quoted-pair は reject される
    let input = "attachment; filename=\"a\\\rb\"";
    let result = ContentDisposition::parse(input);
    assert!(
        result.is_err(),
        "quoted-pair で CR を escape したものは reject されるべき"
    );

    let input = "attachment; filename=\"a\\\nb\"";
    let result = ContentDisposition::parse(input);
    assert!(
        result.is_err(),
        "quoted-pair で LF を escape したものは reject されるべき"
    );

    let input = "attachment; filename=\"a\\\0b\"";
    let result = ContentDisposition::parse(input);
    assert!(
        result.is_err(),
        "quoted-pair で NUL を escape したものは reject されるべき"
    );
}

/// RFC 9110 §5.6.4: qdtext は HTAB / SP / %x21 / %x23-5B / %x5D-7E / obs-text
/// CR / LF / NUL は qdtext として許容しない (escape されていない場合も同じ)
#[test]
fn test_content_disposition_qdtext_rejects_crlf() {
    // 生 CR を含む値は reject される (escape なし)
    let input = "attachment; filename=\"a\rb\"";
    let result = ContentDisposition::parse(input);
    assert!(result.is_err(), "qdtext に CR を含むものは reject される");

    let input = "attachment; filename=\"a\nb\"";
    let result = ContentDisposition::parse(input);
    assert!(result.is_err(), "qdtext に LF を含むものは reject される");
}

// Latin-1 mojibake 入力のリグレッション防止。`\xeb\xa3\xa3` と `\xe9\xa3\xa3` は
// それぞれ UTF-8 として valid な 3 バイトシーケンス (`U+B8E3` / `U+98E3`)。
#[test]
fn test_content_disposition_obs_text_filename_no_mojibake() {
    let input =
        b"inlnie;filename=\"aDDDDDDdttach]ment;}\\/\\\\\\\\;\\\\\\\xeb\xa3\xa3\xe9\xa3\xa3\\;\"";
    let s = core::str::from_utf8(input).expect("valid UTF-8");
    let cd = ContentDisposition::parse(s).expect("parse should succeed");

    let expected = "aDDDDDDdttach]ment;}/\\\\;\\\u{B8E3}\u{98E3};";
    assert_eq!(cd.filename(), Some(expected));

    // ラウンドトリップで mojibake しないこと
    let displayed = cd.to_string();
    let reparsed = ContentDisposition::parse(&displayed).expect("reparse should succeed");
    assert_eq!(reparsed.filename(), Some(expected));
}

// CR / LF / NUL は引き続き reject される (リグレッション防止)
#[test]
fn test_content_disposition_filename_rejects_cr_lf_nul() {
    for c in ['\r', '\n', '\0'] {
        let input = format!("attachment; filename=\"a{}b\"", c);
        let result = ContentDisposition::parse(&input);
        assert!(
            matches!(result, Err(ContentDispositionError::InvalidParameter)),
            "char {:?} は reject される想定: {:?}",
            c,
            result
        );
    }
}

// quoted-pair (`\` + char) でも CR / LF / NUL は reject される
#[test]
fn test_content_disposition_filename_quoted_pair_rejects_cr_lf_nul() {
    for c in ['\r', '\n', '\0'] {
        let input = format!("attachment; filename=\"a\\{}b\"", c);
        let result = ContentDisposition::parse(&input);
        assert!(
            matches!(result, Err(ContentDispositionError::InvalidParameter)),
            "quoted-pair char {:?} は reject される想定: {:?}",
            c,
            result
        );
    }
}

// ========================================
// NBSP は OWS ではないことの検証 (RFC 9110 Section 5.6.3)
// ========================================

#[test]
fn test_content_disposition_nbsp_not_stripped_as_ows() {
    // NBSP は OWS ではないため除去されず、disposition type の検証で失敗する
    let result = ContentDisposition::parse("\u{00A0}attachment");
    assert!(result.is_err());
}

#[test]
fn test_content_disposition_trailing_nbsp_not_stripped() {
    // 末尾の NBSP も OWS として除去されない
    let result = ContentDisposition::parse("attachment\u{00A0}");
    assert!(result.is_err());
}

#[test]
fn test_content_disposition_sp_htab_stripped_as_ows() {
    // SP と HTAB は OWS として正しく除去される
    let cd = ContentDisposition::parse(" \tattachment\t ").unwrap();
    assert_eq!(cd.disposition_type(), DispositionType::Attachment);
}

// ========================================
// src/content_disposition.rs のインラインテストを移動
// ========================================

#[test]
fn test_parse_inline() {
    let cd = ContentDisposition::parse("inline").unwrap();
    assert_eq!(cd.disposition_type(), DispositionType::Inline);
    assert!(cd.is_inline());
    assert!(!cd.is_attachment());
}

#[test]
fn test_parse_attachment() {
    let cd = ContentDisposition::parse("attachment").unwrap();
    assert_eq!(cd.disposition_type(), DispositionType::Attachment);
    assert!(cd.is_attachment());
}

#[test]
fn test_parse_attachment_with_filename() {
    let cd = ContentDisposition::parse("attachment; filename=\"example.txt\"").unwrap();
    assert!(cd.is_attachment());
    assert_eq!(cd.filename(), Some("example.txt"));
}

#[test]
fn test_parse_filename_without_quotes() {
    let cd = ContentDisposition::parse("attachment; filename=example.txt").unwrap();
    assert_eq!(cd.filename(), Some("example.txt"));
}

#[test]
fn test_parse_filename_with_escape() {
    let cd = ContentDisposition::parse(r#"attachment; filename="file\"name.txt""#).unwrap();
    assert_eq!(cd.filename(), Some("file\"name.txt"));
}

#[test]
fn test_parse_filename_ext() {
    let cd =
        ContentDisposition::parse("attachment; filename*=UTF-8''%E6%97%A5%E6%9C%AC%E8%AA%9E.txt")
            .unwrap();
    assert_eq!(cd.filename(), Some("日本語.txt"));
    assert_eq!(cd.filename_ext(), Some("日本語.txt"));
}

#[test]
fn test_filename_ext_priority() {
    let cd = ContentDisposition::parse(
        "attachment; filename=\"fallback.txt\"; filename*=UTF-8''preferred.txt",
    )
    .unwrap();
    assert_eq!(cd.filename(), Some("preferred.txt"));
    assert_eq!(cd.filename_ascii(), Some("fallback.txt"));
}

#[test]
fn test_parse_form_data() {
    let cd = ContentDisposition::parse("form-data; name=\"field1\"").unwrap();
    assert!(cd.is_form_data());
    assert_eq!(cd.name(), Some("field1"));
}

#[test]
fn test_parse_form_data_with_filename() {
    let cd = ContentDisposition::parse("form-data; name=\"file\"; filename=\"image.png\"").unwrap();
    assert!(cd.is_form_data());
    assert_eq!(cd.name(), Some("file"));
    assert_eq!(cd.filename(), Some("image.png"));
}

#[test]
fn test_parse_case_insensitive() {
    let cd = ContentDisposition::parse("ATTACHMENT; FILENAME=\"test.txt\"").unwrap();
    assert!(cd.is_attachment());
    assert_eq!(cd.filename(), Some("test.txt"));
}

#[test]
fn test_parse_empty() {
    assert!(ContentDisposition::parse("").is_err());
}

#[test]
fn test_parse_invalid_type() {
    assert!(ContentDisposition::parse("hello world").is_err());
    assert!(ContentDisposition::parse("type@invalid").is_err());
}

#[test]
fn test_display() {
    let cd = ContentDisposition::new(DispositionType::Attachment).with_filename("test.txt");
    assert_eq!(cd.to_string(), "attachment; filename=\"test.txt\"");
}

#[test]
fn test_display_with_filename_ext() {
    let cd = ContentDisposition::new(DispositionType::Attachment)
        .with_filename("fallback.txt")
        .with_filename_ext("日本語.txt");
    let s = cd.to_string();
    assert!(s.contains("attachment"));
    assert!(s.contains("filename=\"fallback.txt\""));
    assert!(s.contains("filename*=UTF-8''"));
}

#[test]
fn test_display_form_data() {
    let cd = ContentDisposition::new(DispositionType::FormData)
        .with_name("field")
        .with_filename("file.txt");
    let s = cd.to_string();
    assert!(s.contains("form-data"));
    assert!(s.contains("name=\"field\""));
    assert!(s.contains("filename=\"file.txt\""));
}

#[test]
fn test_builder() {
    let cd = ContentDisposition::new(DispositionType::Attachment)
        .with_filename("example.txt")
        .with_filename_ext("例.txt");

    assert!(cd.is_attachment());
    assert_eq!(cd.filename_ascii(), Some("example.txt"));
    assert_eq!(cd.filename_ext(), Some("例.txt"));
    assert_eq!(cd.filename(), Some("例.txt"));
}

#[test]
fn test_ext_value_invalid_char() {
    assert!(ContentDisposition::parse("attachment; filename*=UTF-8''hello world.txt").is_err());
    assert!(ContentDisposition::parse("attachment; filename*=UTF-8''test@file.txt").is_err());
}

#[test]
fn test_ext_value_valid_chars() {
    let cd = ContentDisposition::parse("attachment; filename*=UTF-8''test-file_v1.0.txt").unwrap();
    assert_eq!(cd.filename(), Some("test-file_v1.0.txt"));
}

#[test]
fn test_unknown_disposition_type() {
    let cd = ContentDisposition::parse("signal").unwrap();
    assert_eq!(
        cd.disposition_type(),
        DispositionType::Unknown("signal".to_string())
    );
}

#[test]
fn test_unknown_disposition_type_with_params() {
    let cd = ContentDisposition::parse("notification; id=123").unwrap();
    assert_eq!(
        cd.disposition_type(),
        DispositionType::Unknown("notification".to_string())
    );
    assert_eq!(cd.parameter("id"), Some("123"));
}

#[test]
fn test_unknown_disposition_type_case_insensitive() {
    let cd = ContentDisposition::parse("CUSTOM-TYPE").unwrap();
    assert_eq!(
        cd.disposition_type(),
        DispositionType::Unknown("custom-type".to_string())
    );
}

#[test]
fn test_invalid_disposition_type() {
    assert!(ContentDisposition::parse("hello world").is_err());
}

#[test]
fn test_invalid_disposition_type_special_char() {
    assert!(ContentDisposition::parse("type@invalid").is_err());
}

#[test]
fn test_unknown_disposition_display() {
    let cd = ContentDisposition::parse("custom-type; name=\"test\"").unwrap();
    let s = cd.to_string();
    assert!(s.starts_with("custom-type"));
}

#[test]
fn test_invalid_token_parameter_value() {
    assert!(ContentDisposition::parse("attachment; filename=hello@world.txt").is_err());
}

#[test]
fn test_invalid_token_parameter_value_space() {
    assert!(ContentDisposition::parse("attachment; filename=hello world.txt").is_err());
}

#[test]
fn test_valid_token_parameter_value() {
    let cd = ContentDisposition::parse("attachment; filename=valid-token_v1.0").unwrap();
    assert_eq!(cd.filename(), Some("valid-token_v1.0"));
}

#[test]
fn test_quoted_special_chars() {
    let cd = ContentDisposition::parse("attachment; filename=\"hello@world.txt\"").unwrap();
    assert_eq!(cd.filename(), Some("hello@world.txt"));
}
