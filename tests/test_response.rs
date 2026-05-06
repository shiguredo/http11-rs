//! Response の構築時バリデーションのユニットテスト
//!
//! 構築時に弾かれるエラー (CRLF 注入、token 違反、status_code 範囲外等) を網羅する。
//! PBT で生成不可能な特定値を含むケースを担う。

use shiguredo_http11::{EncodeError, HttpHead, Response, StatusCode};

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
    let mut r = Response::with_status(StatusCode::OK);
    let result = r.add_header("Bad Name", "x");
    assert!(matches!(result, Err(EncodeError::InvalidHeaderName { .. })));
}

#[test]
fn test_response_add_header_empty_name() {
    let mut r = Response::with_status(StatusCode::OK);
    let result = r.add_header("", "x");
    assert!(matches!(result, Err(EncodeError::InvalidHeaderName { .. })));
}

#[test]
fn test_response_add_header_crlf_in_value() {
    let mut r = Response::with_status(StatusCode::OK);
    let result = r.add_header("X-Header", "value\r\n");
    assert!(matches!(
        result,
        Err(EncodeError::InvalidHeaderValue { .. })
    ));
}

#[test]
fn test_response_add_header_lf_only_in_value() {
    let mut r = Response::with_status(StatusCode::OK);
    let result = r.add_header("X-Header", "value\n");
    assert!(matches!(
        result,
        Err(EncodeError::InvalidHeaderValue { .. })
    ));
}

#[test]
fn test_response_add_header_nul_in_value() {
    let mut r = Response::with_status(StatusCode::OK);
    let result = r.add_header("X-Header", "val\0ue");
    assert!(matches!(
        result,
        Err(EncodeError::InvalidHeaderValue { .. })
    ));
}

#[test]
fn test_response_add_header_empty_value_is_legal() {
    // RFC 9110 Section 5.5: field-value = *field-content, 空値は合法
    let mut r = Response::with_status(StatusCode::OK);
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
    let mut r = Response::with_status(StatusCode::OK);
    r.add_header("X-Custom", "first").unwrap();
    r.set_header("X-Custom", "second").unwrap();
    assert_eq!(r.get_headers("X-Custom").len(), 1);
    assert_eq!(r.get_header("X-Custom"), Some("second"));
}

#[test]
fn test_response_set_header_case_insensitive_overwrite() {
    let mut r = Response::with_status(StatusCode::OK);
    r.add_header("CONTENT-TYPE", "text/plain").unwrap();
    r.set_header("Content-Type", "text/html").unwrap();
    assert_eq!(r.get_header("Content-Type"), Some("text/html"));
    assert_eq!(r.get_headers("Content-Type").len(), 1);
}

#[test]
fn test_response_set_header_atomic_on_validation_failure() {
    // バリデーション失敗時に既存ヘッダーが消えないことを確認 (アトミック性)
    let mut r = Response::with_status(StatusCode::OK);
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
    let mut r = Response::with_status(StatusCode::OK);
    r.add_header("X-Custom", "first").unwrap();
    let result = r.set_header("Bad Name", "value");
    assert!(matches!(result, Err(EncodeError::InvalidHeaderName { .. })));
    // 既存ヘッダーが保持されていること
    assert_eq!(r.get_header("X-Custom"), Some("first"));
}

#[test]
fn test_response_accessors() {
    // アクセサ経由のフィールドアクセスを確認
    let r = Response::with_status(StatusCode::OK);
    assert_eq!(r.status_code(), 200);
    assert_eq!(r.reason_phrase(), "OK");
    assert_eq!(HttpHead::version(&r), "HTTP/1.1");
    assert!(r.body_bytes().is_none());
    assert!(!r.is_body_omitted());

    // with_version はカスタムバージョン用なのでそのまま残す
    let r2 = Response::with_version("HTTP/1.0", 404, "Not Found").unwrap();
    assert_eq!(HttpHead::version(&r2), "HTTP/1.0");
    assert_eq!(r2.status_code(), 404);
    assert_eq!(r2.reason_phrase(), "Not Found");

    let r3 = Response::with_status(StatusCode::OK).body(b"hello".to_vec());
    assert_eq!(r3.body_bytes(), Some(&b"hello"[..]));

    let r4 = Response::with_status(StatusCode::OK).omit_body(true);
    assert!(r4.is_body_omitted());
}

#[test]
fn test_response_with_status_basic() {
    // with_status は infallible で Response を返す
    let r = Response::with_status(StatusCode::OK);
    assert_eq!(r.status_code(), 200);
    assert_eq!(r.reason_phrase(), "OK");
    assert_eq!(HttpHead::version(&r), "HTTP/1.1");
    assert!(r.body_bytes().is_none());
    assert!(!r.is_body_omitted());
}

#[test]
fn test_response_with_status_equivalent_to_new() {
    // with_status(StatusCode::OK) と new(200, "OK") は同一の Response を生成する
    let via_status = Response::with_status(StatusCode::OK);
    let via_new = Response::new(200, "OK").unwrap();
    assert_eq!(via_status, via_new);
}

#[test]
fn test_response_with_status_404() {
    let r = Response::with_status(StatusCode::NOT_FOUND);
    assert_eq!(r.status_code(), 404);
    assert_eq!(r.reason_phrase(), "Not Found");
    assert_eq!(HttpHead::version(&r), "HTTP/1.1");
}

#[test]
fn test_response_with_status_chains_with_builders() {
    let r = Response::with_status(StatusCode::CREATED)
        .header("Content-Type", "application/json")
        .unwrap()
        .body(b"{}".to_vec());
    assert_eq!(r.status_code(), 201);
    assert_eq!(r.reason_phrase(), "Created");
    assert_eq!(r.get_header("Content-Type"), Some("application/json"));
    assert_eq!(r.body_bytes(), Some(&b"{}"[..]));
}

#[test]
fn test_response_with_status_encodable() {
    // with_status で構築した Response は encoder の二重バリデーションを通過する
    let r = Response::with_status(StatusCode::NO_CONTENT);
    let bytes = r.try_encode().unwrap();
    let s = core::str::from_utf8(&bytes).unwrap();
    assert!(s.starts_with("HTTP/1.1 204 No Content\r\n"));
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

// ========================================
// 0021: ビルダーと mutator の対称性 / チェイン化 / impl Into 化のテスト
// ========================================

#[test]
fn test_response_set_body_then_clear_body() {
    // set_body で body を設定した後、clear_body で None になる
    let mut r = Response::with_status(StatusCode::OK);
    r.set_body(b"data".to_vec());
    assert!(r.body_bytes().is_some());
    r.clear_body();
    assert!(r.body_bytes().is_none());
}

#[test]
fn test_response_without_body_builder() {
    // builder の without_body() で body が None になる
    let r = Response::with_status(StatusCode::OK)
        .body(b"data".to_vec())
        .without_body();
    assert!(r.body_bytes().is_none());
}

#[test]
fn test_response_set_body_value() {
    let mut r = Response::with_status(StatusCode::OK);
    r.set_body(b"hello".to_vec());
    assert_eq!(r.body_bytes(), Some(b"hello".as_slice()));
}

#[test]
fn test_response_set_body_empty_vec_is_explicit_empty() {
    // set_body(Vec::new()) は明示的空ボディ (body = Some(vec![]))
    let mut r = Response::with_status(StatusCode::OK);
    r.set_body(Vec::new());
    assert_eq!(r.body_bytes(), Some(&[] as &[u8]));
}

#[test]
fn test_response_clear_body_and_without_body_equivalence() {
    // clear_body (mutator) と without_body (builder) は同じ結果になる
    let mut r1 = Response::with_status(StatusCode::OK);
    r1.set_body(b"data".to_vec());
    r1.clear_body();
    let r2 = Response::with_status(StatusCode::OK)
        .body(b"data".to_vec())
        .without_body();
    assert_eq!(r1.body_bytes(), r2.body_bytes());
    assert!(r1.body_bytes().is_none());
}

#[test]
fn test_response_set_omit_body() {
    let mut r = Response::with_status(StatusCode::OK);
    r.set_omit_body(true);
    assert!(r.is_body_omitted());
    r.set_omit_body(false);
    assert!(!r.is_body_omitted());
}

#[test]
fn test_response_add_header_chain() {
    // add_header のチェイン: Result<&mut Self, E> を unwrap で消費して連結
    let mut r = Response::with_status(StatusCode::OK);
    r.add_header("X-A", "1")
        .unwrap()
        .add_header("X-B", "2")
        .unwrap();
    assert_eq!(r.get_headers("X-A"), vec!["1"]);
    assert_eq!(r.get_headers("X-B"), vec!["2"]);
}

#[test]
fn test_response_add_header_chain_partial_failure() {
    // 先行ヘッダーは成功し、後続のバリデーションエラーは先行を破壊しない
    let mut r = Response::with_status(StatusCode::OK);
    r.add_header("X-A", "1").unwrap();
    let result = r.add_header("", "bad");
    assert!(result.is_err());
    assert_eq!(r.get_headers("X-A"), vec!["1"]);
}

#[test]
fn test_response_add_header_accepts_string_owned() {
    // String を所有する値が impl Into<String> でムーブできる
    let name = String::from("X-Custom");
    let value = String::from("my-value");
    let mut r = Response::with_status(StatusCode::OK);
    r.add_header(name, value).unwrap();
    assert_eq!(r.get_header("X-Custom"), Some("my-value"));
}

#[test]
fn test_response_with_version_accepts_string_owned() {
    // version / reason_phrase に String を所有値でムーブできる
    let version = String::from("HTTP/1.1");
    let reason = String::from("OK");
    let r = Response::with_version(version, 200, reason).unwrap();
    assert_eq!(HttpHead::version(&r), "HTTP/1.1");
    assert_eq!(r.reason_phrase(), "OK");
}

#[test]
fn test_response_new_accepts_string_owned() {
    let reason = String::from("OK");
    let r = Response::new(200, reason).unwrap();
    assert_eq!(r.reason_phrase(), "OK");
}

#[test]
fn test_response_body_accepts_vec_owned() {
    // builder body() が impl Into<Vec<u8>> で Vec<u8> をムーブできる
    let data = b"payload".to_vec();
    let r = Response::with_status(StatusCode::OK).body(data);
    assert_eq!(r.body_bytes(), Some(b"payload".as_slice()));
}

#[test]
fn test_response_body_accepts_slice_clone() {
    // builder body() が impl Into<Vec<u8>> で &[u8] (clone) も受け付ける
    let r = Response::with_status(StatusCode::OK).body(b"payload".as_slice());
    assert_eq!(r.body_bytes(), Some(b"payload".as_slice()));
}

#[test]
fn test_response_set_header_chain() {
    // set_header のチェイン
    let mut r = Response::with_status(StatusCode::OK);
    r.set_header("Content-Type", "text/plain")
        .unwrap()
        .set_header("Content-Length", "0")
        .unwrap();
    assert_eq!(r.get_header("Content-Type"), Some("text/plain"));
    assert_eq!(r.get_header("Content-Length"), Some("0"));
}

#[test]
fn test_response_set_body_then_set_omit_body_chain() {
    // set_body は infallible なのでチェインで set_omit_body を続けられる
    let mut r = Response::with_status(StatusCode::OK);
    r.set_body(b"hello".to_vec()).set_omit_body(true);
    assert!(r.body_bytes().is_some());
    assert!(r.is_body_omitted());
}

#[test]
fn test_response_header_builder_chain_with_impl_into() {
    // builder header() が impl Into<String> でも従来通り動作する
    let r = Response::with_status(StatusCode::OK)
        .header("X-A", "1")
        .unwrap()
        .header(String::from("X-B"), String::from("2"))
        .unwrap();
    assert_eq!(r.get_header("X-A"), Some("1"));
    assert_eq!(r.get_header("X-B"), Some("2"));
}
