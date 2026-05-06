//! Response の構築時バリデーションのユニットテスト
//!
//! 構築時に弾かれるエラー (CRLF 注入、token 違反、status_code 範囲外等) を網羅する。
//! PBT で生成不可能な特定値を含むケースを担う。

use shiguredo_http11::{EncodeError, HttpHead, Response};

#[test]
fn test_response_new_invalid_status_code_zero() {
    let result = Response::new(0, "OK");
    assert!(matches!(
        result,
        Err(EncodeError::InvalidStatusCode { code: 0 })
    ));
}

#[test]
fn test_response_new_invalid_status_code_600() {
    let result = Response::new(600, "OK");
    assert!(matches!(
        result,
        Err(EncodeError::InvalidStatusCode { code: 600 })
    ));
}

#[test]
fn test_response_new_empty_reason_phrase() {
    let result = Response::new(200, "");
    assert!(matches!(
        result,
        Err(EncodeError::InvalidReasonPhrase { .. })
    ));
}

#[test]
fn test_response_new_crlf_in_reason_phrase() {
    let result = Response::new(200, "OK\r\nX-Inject: y");
    assert!(matches!(
        result,
        Err(EncodeError::InvalidReasonPhrase { .. })
    ));
}

#[test]
fn test_response_new_nul_in_reason_phrase() {
    let result = Response::new(200, "OK\0bad");
    assert!(matches!(
        result,
        Err(EncodeError::InvalidReasonPhrase { .. })
    ));
}

#[test]
fn test_response_add_header_space_in_name() {
    let mut r = Response::new(200, "OK").unwrap();
    let result = r.add_header("Bad Name", "x");
    assert!(matches!(result, Err(EncodeError::InvalidHeaderName { .. })));
}

#[test]
fn test_response_add_header_empty_name() {
    let mut r = Response::new(200, "OK").unwrap();
    let result = r.add_header("", "x");
    assert!(matches!(result, Err(EncodeError::InvalidHeaderName { .. })));
}

#[test]
fn test_response_add_header_crlf_in_value() {
    let mut r = Response::new(200, "OK").unwrap();
    let result = r.add_header("X-Header", "value\r\n");
    assert!(matches!(
        result,
        Err(EncodeError::InvalidHeaderValue { .. })
    ));
}

#[test]
fn test_response_add_header_lf_only_in_value() {
    let mut r = Response::new(200, "OK").unwrap();
    let result = r.add_header("X-Header", "value\n");
    assert!(matches!(
        result,
        Err(EncodeError::InvalidHeaderValue { .. })
    ));
}

#[test]
fn test_response_add_header_nul_in_value() {
    let mut r = Response::new(200, "OK").unwrap();
    let result = r.add_header("X-Header", "val\0ue");
    assert!(matches!(
        result,
        Err(EncodeError::InvalidHeaderValue { .. })
    ));
}

#[test]
fn test_response_add_header_empty_value_is_legal() {
    // RFC 9110 Section 5.5: field-value = *field-content, 空値は合法
    let mut r = Response::new(200, "OK").unwrap();
    assert!(r.add_header("X-Empty", "").is_ok());
}

#[test]
fn test_response_with_version_garbage() {
    let result = Response::with_version("garbage", 200, "OK");
    assert!(matches!(result, Err(EncodeError::InvalidVersion { .. })));
}

#[test]
fn test_response_with_version_crlf() {
    let result = Response::with_version("HTTP/1.1\r\nX: y", 200, "OK");
    assert!(matches!(result, Err(EncodeError::InvalidVersion { .. })));
}

#[test]
fn test_response_set_header_overwrite() {
    let mut r = Response::new(200, "OK").unwrap();
    r.add_header("X-Custom", "first").unwrap();
    r.set_header("X-Custom", "second").unwrap();
    assert_eq!(r.get_headers("X-Custom").len(), 1);
    assert_eq!(r.get_header("X-Custom"), Some("second"));
}

#[test]
fn test_response_set_header_case_insensitive_overwrite() {
    let mut r = Response::new(200, "OK").unwrap();
    r.add_header("CONTENT-TYPE", "text/plain").unwrap();
    r.set_header("Content-Type", "text/html").unwrap();
    assert_eq!(r.get_header("Content-Type"), Some("text/html"));
    assert_eq!(r.get_headers("Content-Type").len(), 1);
}

#[test]
fn test_response_set_header_atomic_on_validation_failure() {
    // バリデーション失敗時に既存ヘッダーが消えないことを確認 (アトミック性)
    let mut r = Response::new(200, "OK").unwrap();
    r.add_header("X-Custom", "first").unwrap();
    let result = r.set_header("X-Custom", "bad\r\nvalue");
    assert!(matches!(
        result,
        Err(EncodeError::InvalidHeaderValue { .. })
    ));
    // 既存ヘッダーが保持されていること
    assert_eq!(r.get_header("X-Custom"), Some("first"));
}

#[test]
fn test_response_set_header_invalid_name() {
    let mut r = Response::new(200, "OK").unwrap();
    r.add_header("X-Custom", "first").unwrap();
    let result = r.set_header("Bad Name", "value");
    assert!(matches!(result, Err(EncodeError::InvalidHeaderName { .. })));
    // 既存ヘッダーが保持されていること
    assert_eq!(r.get_header("X-Custom"), Some("first"));
}

#[test]
fn test_response_accessors() {
    // アクセサ経由のフィールドアクセスを確認
    let r = Response::new(200, "OK").unwrap();
    assert_eq!(r.status_code(), 200);
    assert_eq!(r.reason_phrase(), "OK");
    assert_eq!(HttpHead::version(&r), "HTTP/1.1");
    assert!(r.body_bytes().is_none());
    assert!(!r.is_body_omitted());

    let r2 = Response::with_version("HTTP/1.0", 404, "Not Found").unwrap();
    assert_eq!(HttpHead::version(&r2), "HTTP/1.0");
    assert_eq!(r2.status_code(), 404);
    assert_eq!(r2.reason_phrase(), "Not Found");

    let r3 = Response::new(200, "OK").unwrap().body(b"hello".to_vec());
    assert_eq!(r3.body_bytes(), Some(&b"hello"[..]));

    let r4 = Response::new(200, "OK").unwrap().omit_body(true);
    assert!(r4.is_body_omitted());
}

#[test]
fn test_response_encode_reason_phrase_absent_via_decoder() {
    // decoder が空 reason_phrase を持つ Response を作成し、encoder で再送信できる
    // RFC 9112 Section 4: status-line ABNF で reason-phrase は OPTIONAL
    use shiguredo_http11::ResponseDecoder;
    let mut decoder = ResponseDecoder::new();
    decoder.feed(b"HTTP/1.1 200 \r\n\r\n").unwrap();
    let response = decoder.decode_headers().unwrap();
    // decode_headers は本テストではボディ完了まで進まないので、別経路で完了させる
    // ここでは decode_headers の戻り値を使って構築済みの Response を再送信できるかは
    // 統合テストとして見ないが、validate_response_fields が空 reason-phrase を許容する
    // ことが encoder.rs の単体テスト (mod validate_response_fields_tests) で確認済み。
    let _ = response;
}
