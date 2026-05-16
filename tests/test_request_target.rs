//! request-target のユニットテスト (RFC 9112 Section 3.2)
//!
//! ## なぜ PBT ではテストできないのか
//!
//! これらのテストは固定入力に対する具体的な振る舞いを検証するものであり、
//! ランダム生成による性質テストの対象ではない。
//! asterisk-form ("*") は入力が一意であり、不正文字やパーセントエンコーディングの
//! テストも特定のエッジケースを網羅的に確認する目的で書かれている。

use shiguredo_http11::RequestDecoder;

// ========================================
// asterisk-form テスト
// ========================================

#[test]
fn test_asterisk_form_with_options_succeeds() {
    let request_line = "OPTIONS * HTTP/1.1\r\nHost: example.com\r\n\r\n";
    let mut decoder = RequestDecoder::new();
    decoder.feed(request_line.as_bytes()).unwrap();
    let result = decoder.decode_headers();
    assert!(result.is_ok(), "OPTIONS + asterisk-form は成功すべき");
    assert!(result.unwrap().is_some());
}

#[test]
fn test_asterisk_form_with_get_fails() {
    let request_line = "GET * HTTP/1.1\r\nHost: example.com\r\n\r\n";
    let mut decoder = RequestDecoder::new();
    decoder.feed(request_line.as_bytes()).unwrap();
    let result = decoder.decode_headers();
    assert!(result.is_err(), "GET + asterisk-form は失敗すべき");
}

#[test]
fn test_asterisk_form_with_post_fails() {
    let request_line = "POST * HTTP/1.1\r\nHost: example.com\r\n\r\n";
    let mut decoder = RequestDecoder::new();
    decoder.feed(request_line.as_bytes()).unwrap();
    let result = decoder.decode_headers();
    assert!(result.is_err(), "POST + asterisk-form は失敗すべき");
}

// ========================================
// 不正な文字テスト (RFC 3986)
// ========================================

#[test]
fn test_invalid_path_character_space() {
    let request_line = "GET /path with space HTTP/1.1\r\nHost: example.com\r\n\r\n";
    let mut decoder = RequestDecoder::new();
    decoder.feed(request_line.as_bytes()).unwrap();
    let result = decoder.decode_headers();
    assert!(result.is_err(), "空白を含むパスは失敗すべき");
}

#[test]
fn test_invalid_path_character_backslash() {
    let request_line = "GET /path\\file HTTP/1.1\r\nHost: example.com\r\n\r\n";
    let mut decoder = RequestDecoder::new();
    decoder.feed(request_line.as_bytes()).unwrap();
    let result = decoder.decode_headers();
    assert!(result.is_err(), "バックスラッシュを含むパスは失敗すべき");
}

#[test]
fn test_invalid_path_character_angle_bracket() {
    let request_line = "GET /path<file HTTP/1.1\r\nHost: example.com\r\n\r\n";
    let mut decoder = RequestDecoder::new();
    decoder.feed(request_line.as_bytes()).unwrap();
    let result = decoder.decode_headers();
    assert!(result.is_err(), "山括弧を含むパスは失敗すべき");
}

// ========================================
// パーセントエンコーディングテスト
// ========================================

#[test]
fn test_valid_percent_encoding() {
    let request_line = "GET /path%20with%20space HTTP/1.1\r\nHost: example.com\r\n\r\n";
    let mut decoder = RequestDecoder::new();
    decoder.feed(request_line.as_bytes()).unwrap();
    let result = decoder.decode_headers();
    assert!(
        result.is_ok(),
        "正常なパーセントエンコーディングは成功すべき"
    );
    assert!(result.unwrap().is_some());
}

#[test]
fn test_invalid_percent_encoding_incomplete() {
    let request_line = "GET /path%2 HTTP/1.1\r\nHost: example.com\r\n\r\n";
    let mut decoder = RequestDecoder::new();
    decoder.feed(request_line.as_bytes()).unwrap();
    let result = decoder.decode_headers();
    assert!(
        result.is_err(),
        "不完全なパーセントエンコーディングは失敗すべき"
    );
}

#[test]
fn test_invalid_percent_encoding_non_hex() {
    let request_line = "GET /path%GG HTTP/1.1\r\nHost: example.com\r\n\r\n";
    let mut decoder = RequestDecoder::new();
    decoder.feed(request_line.as_bytes()).unwrap();
    let result = decoder.decode_headers();
    assert!(
        result.is_err(),
        "16 進でないパーセントエンコーディングは失敗すべき"
    );
}

// ========================================
// "://" なしの absolute-form テスト
// ========================================

#[test]
fn test_mailto_absolute_form() {
    let raw = "GET mailto:user@example.com HTTP/1.1\r\nHost: \r\n\r\n";
    let mut decoder = RequestDecoder::new();
    decoder.feed(raw.as_bytes()).unwrap();
    let (head, _) = decoder.decode_headers().unwrap().unwrap();
    assert_eq!(head.uri(), "mailto:user@example.com");
}

#[test]
fn test_tel_absolute_form() {
    let raw = "GET tel:+1-201-555-0123 HTTP/1.1\r\nHost: \r\n\r\n";
    let mut decoder = RequestDecoder::new();
    decoder.feed(raw.as_bytes()).unwrap();
    let (head, _) = decoder.decode_headers().unwrap().unwrap();
    assert_eq!(head.uri(), "tel:+1-201-555-0123");
}
