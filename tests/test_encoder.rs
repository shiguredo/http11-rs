//! エンコーダーのユニットテスト
//!
//! PBT でカバーできないエラーパス・境界値・エッジケースのみ記載する。

use shiguredo_http11::{
    EncodeError, Request, Response, StatusCode, encode_chunk, encode_chunks, encode_request,
    encode_request_headers, encode_response, encode_response_headers,
};

// ========================================
// encode_request のバリデーションエラーテスト
// ========================================

#[test]
fn test_encode_request_invalid_version() {
    // 空文字列は構築時に拒否
    let result = Request::with_version("GET", "/", "");
    assert!(matches!(result, Err(EncodeError::InvalidVersion { .. })));

    // スペースを含むバージョンは構築時に拒否 (SP は VCHAR ではない)
    let result = Request::with_version("GET", "/", "HTTP /1.1");
    assert!(matches!(result, Err(EncodeError::InvalidVersion { .. })));

    // 制御文字を含むバージョンは構築時に拒否
    let result = Request::with_version("GET", "/", "HTTP\x00/1.1");
    assert!(matches!(result, Err(EncodeError::InvalidVersion { .. })));
}

#[test]
fn test_encode_response_invalid_version() {
    // 空文字列は構築時に拒否
    let result = Response::with_version("", 200, "OK");
    assert!(matches!(result, Err(EncodeError::InvalidVersion { .. })));

    // スペースを含むバージョンは構築時に拒否
    let result = Response::with_version("HTTP /1.1", 200, "OK");
    assert!(matches!(result, Err(EncodeError::InvalidVersion { .. })));

    // 制御文字を含むバージョンは構築時に拒否
    let result = Response::with_version("HTTP\x01/1.1", 200, "OK");
    assert!(matches!(result, Err(EncodeError::InvalidVersion { .. })));
}

#[test]
fn test_encode_response_invalid_status_code() {
    // 100 未満のステータスコードは構築時に拒否
    let result = Response::with_version("HTTP/1.1", 99, "Invalid");
    assert!(matches!(
        result,
        Err(EncodeError::InvalidStatusCode { code: 99 })
    ));

    // 600 以上のステータスコードは構築時に拒否
    let result = Response::with_version("HTTP/1.1", 600, "Invalid");
    assert!(matches!(
        result,
        Err(EncodeError::InvalidStatusCode { code: 600 })
    ));

    // 0 も構築時に拒否
    let result = Response::with_version("HTTP/1.1", 0, "Invalid");
    assert!(matches!(
        result,
        Err(EncodeError::InvalidStatusCode { code: 0 })
    ));
}

// ========================================
// encode_request のエッジケーステスト
// ========================================

#[test]
fn test_encode_request_with_existing_content_length() {
    // Content-Length が既に設定されている場合は追加しない
    let req = Request::new("POST", "/")
        .unwrap()
        .header("Host", "example.com")
        .unwrap()
        .header("Content-Length", "5")
        .unwrap()
        .body(b"hello".to_vec());
    let encoded = encode_request(&req).unwrap();
    let encoded_str = String::from_utf8_lossy(&encoded);

    // Content-Length が 1 回だけ出現
    let count = encoded_str.matches("Content-Length").count();
    assert_eq!(count, 1);
}

// body == None と body == Some(vec![]) の挙動差分 (issue 0004)

#[test]
fn test_encode_post_with_explicit_empty_body_emits_content_length_zero() {
    // POST + body=Some(vec![]) なら Content-Length: 0 を自動付与する
    let req = Request::new("POST", "/")
        .unwrap()
        .header("Host", "example.com")
        .unwrap()
        .body(Vec::new());
    let encoded = encode_request(&req).unwrap();
    let encoded_str = String::from_utf8_lossy(&encoded);

    assert!(
        encoded_str.contains("Content-Length: 0\r\n"),
        "expected Content-Length: 0, got: {encoded_str}"
    );
}

#[test]
fn test_encode_post_without_body_emits_no_content_length() {
    // POST + body=None なら Content-Length は自動付与しない
    // (RFC 9110 8.6 はメソッド意味論で content が想定されるかは呼び出し側の判断とする)
    let req = Request::new("POST", "/")
        .unwrap()
        .header("Host", "example.com")
        .unwrap();
    assert!(req.body_bytes().is_none());
    let encoded = encode_request(&req).unwrap();
    let encoded_str = String::from_utf8_lossy(&encoded);

    assert!(
        !encoded_str.contains("Content-Length"),
        "expected no Content-Length, got: {encoded_str}"
    );
}

#[test]
fn test_encode_get_without_body_emits_no_content_length() {
    // GET + body=None (デフォルト) は Content-Length を自動付与しない
    let req = Request::new("GET", "/")
        .unwrap()
        .header("Host", "example.com")
        .unwrap();
    let encoded = encode_request(&req).unwrap();
    let encoded_str = String::from_utf8_lossy(&encoded);

    assert!(
        !encoded_str.contains("Content-Length"),
        "expected no Content-Length, got: {encoded_str}"
    );
}

// ========================================
// encode_response のエッジケーステスト
// ========================================

#[test]
fn test_encode_response_no_content_length_with_transfer_encoding() {
    // Transfer-Encoding がある場合は Content-Length を追加しない
    let res = Response::with_status(StatusCode::OK)
        .header("Transfer-Encoding", "chunked")
        .unwrap()
        .body(b"hello".to_vec());
    let encoded = encode_response(&res).unwrap();
    let encoded_str = String::from_utf8_lossy(&encoded);

    assert!(!encoded_str.contains("Content-Length"));
}

// ========================================
// encode_chunk のテスト
// ========================================

#[test]
fn test_encode_chunk_empty() {
    // 空データは終端チャンク
    let encoded = encode_chunk(&[]);
    assert_eq!(encoded, b"0\r\n\r\n");
}

// ========================================
// encode_chunks のテスト
// ========================================

#[test]
fn test_encode_chunks_empty_list() {
    // 空リストでも終端チャンクは出力
    let encoded = encode_chunks(&[]);
    assert_eq!(encoded, b"0\r\n\r\n");
}

#[test]
fn test_encode_chunks_various_sizes() {
    let chunks: Vec<&[u8]> = vec![b"a", b"bb", b"ccc", b"dddd"];
    let encoded = encode_chunks(&chunks);
    let encoded_str = String::from_utf8_lossy(&encoded);

    assert!(encoded_str.contains("1\r\na\r\n"));
    assert!(encoded_str.contains("2\r\nbb\r\n"));
    assert!(encoded_str.contains("3\r\nccc\r\n"));
    assert!(encoded_str.contains("4\r\ndddd\r\n"));
    assert!(encoded_str.ends_with("0\r\n\r\n"));
}

// ========================================
// encode_request_headers / encode_response_headers のテスト
// ========================================

#[test]
fn test_encode_request_headers_ignores_body() {
    let req = Request::with_version("POST", "/", "HTTP/1.0")
        .unwrap()
        .body(b"hello world".to_vec());
    let encoded = encode_request_headers(&req).unwrap();
    let encoded_str = String::from_utf8_lossy(&encoded);

    // ボディは含まれない
    assert!(!encoded_str.contains("hello world"));
    // Content-Length も自動追加されない
    assert!(!encoded_str.contains("Content-Length"));
}

#[test]
fn test_encode_response_headers_ignores_body() {
    let res = Response::with_status(StatusCode::OK).body(b"hello world".to_vec());
    let encoded = encode_response_headers(&res).unwrap();
    let encoded_str = String::from_utf8_lossy(&encoded);

    // ボディは含まれない
    assert!(!encoded_str.contains("hello world"));
}

// ========================================
// 正常ケース: Transfer-Encoding のみ、または Content-Length のみは許可
// ========================================

#[test]
fn test_encode_request_te_only_ok() {
    let req = Request::new("POST", "/")
        .unwrap()
        .header("Host", "example.com")
        .unwrap()
        .header("Transfer-Encoding", "chunked")
        .unwrap();
    let result = encode_request(&req);
    assert!(result.is_ok());
}

#[test]
fn test_encode_request_cl_only_ok() {
    let req = Request::new("POST", "/")
        .unwrap()
        .header("Host", "example.com")
        .unwrap()
        .header("Content-Length", "100")
        .unwrap()
        .body(vec![0u8; 100]);
    let result = encode_request(&req);
    assert!(result.is_ok());
}

#[test]
fn test_encode_response_te_only_ok() {
    let res = Response::with_status(StatusCode::OK)
        .header("Transfer-Encoding", "chunked")
        .unwrap();
    let result = encode_response(&res);
    assert!(result.is_ok());
}

#[test]
fn test_encode_response_cl_only_ok() {
    let res = Response::with_status(StatusCode::OK)
        .header("Content-Length", "100")
        .unwrap()
        .body(vec![0u8; 100]);
    let result = encode_response(&res);
    assert!(result.is_ok());
}

// ========================================
// 205 Reset Content の正常ケーステスト
// ========================================

#[test]
fn test_encode_response_205_empty_body_ok() {
    let res = Response::with_status(StatusCode::RESET_CONTENT);
    let result = encode_response(&res);
    assert!(result.is_ok());
}

#[test]
fn test_encode_response_205_with_cl_zero_ok() {
    // 205 で Content-Length: 0 は許可
    let res = Response::with_status(StatusCode::RESET_CONTENT)
        .header("Content-Length", "0")
        .unwrap();
    let result = encode_response(&res);
    assert!(result.is_ok());
}

// ========================================
// 205 Content-Length の OWS 厳格化 (issue 0062)
// ========================================
// RFC 9110 Section 5.6.3 の OWS は `*( SP / HTAB )` のみ。
// 旧実装は `str::trim()` を使っており NBSP (U+00A0) 等の Unicode 空白も除去していた。
// 防御層の一貫性のため `trim_ows` に統一し、encode_response / encode_response_headers
// の両経路で同じ判定を行う。

#[test]
fn test_encode_response_205_with_cl_nbsp_zero_error() {
    // Content-Length: \u{00A0}0 (NBSP 前置) は trim_ows で除去されないため非 "0"
    let res = Response::with_status(StatusCode::RESET_CONTENT)
        .header("Content-Length", "\u{00A0}0")
        .unwrap();
    let result = encode_response(&res);
    assert!(matches!(
        result,
        Err(EncodeError::ForbiddenContentLength { status_code: 205 })
    ));
}

#[test]
fn test_encode_response_headers_205_with_cl_nbsp_zero_error() {
    // encode_response_headers 経路でも同様に reject されること
    let res = Response::with_status(StatusCode::RESET_CONTENT)
        .header("Content-Length", "\u{00A0}0")
        .unwrap();
    let result = encode_response_headers(&res);
    assert!(matches!(
        result,
        Err(EncodeError::ForbiddenContentLength { status_code: 205 })
    ));
}

#[test]
fn test_encode_response_205_with_cl_htab_zero_ok() {
    // HTAB 前置は OWS として正しく除去される (リグレッション防止)
    let res = Response::with_status(StatusCode::RESET_CONTENT)
        .header("Content-Length", "\t0")
        .unwrap();
    assert!(encode_response(&res).is_ok());
    assert!(encode_response_headers(&res).is_ok());
}

#[test]
fn test_encode_response_205_with_cl_sp_zero_ok() {
    // SP 前置 / 後置は OWS として正しく除去される (リグレッション防止)
    let res = Response::with_status(StatusCode::RESET_CONTENT)
        .header("Content-Length", " 0 ")
        .unwrap();
    assert!(encode_response(&res).is_ok());
    assert!(encode_response_headers(&res).is_ok());
}

// ========================================
// Host ヘッダーバリデーションテスト
// ========================================

#[test]
fn test_encode_request_invalid_host_error() {
    // 不正な Host ヘッダー値はエラー
    let req = Request::new("GET", "/")
        .unwrap()
        .header("Host", "exam ple.com")
        .unwrap();
    let result = encode_request(&req);
    assert!(matches!(result, Err(EncodeError::InvalidHostHeader { .. })));
}

#[test]
fn test_encode_request_host_authority_mismatch_error() {
    // Host と URI authority の不一致はエラー
    let req = Request::new("GET", "http://example.com/path")
        .unwrap()
        .header("Host", "other.com")
        .unwrap();
    let result = encode_request(&req);
    assert!(matches!(
        result,
        Err(EncodeError::HostAuthorityMismatch { .. })
    ));
}

#[test]
fn test_encode_request_host_authority_match_ok() {
    // Host と URI authority の一致は OK
    let req = Request::new("GET", "http://example.com/path")
        .unwrap()
        .header("Host", "example.com")
        .unwrap();
    let result = encode_request(&req);
    assert!(result.is_ok());
}

#[test]
fn test_encode_request_empty_host_ok() {
    // 空の Host ヘッダーは許可 (RFC 9112 Section 3.2)
    let req = Request::new("GET", "/")
        .unwrap()
        .header("Host", "")
        .unwrap();
    let result = encode_request(&req);
    assert!(result.is_ok());
}

// ========================================
// CRLF/NUL インジェクション拒否テスト
// ========================================

#[test]
fn test_encode_request_crlf_in_method() {
    // 不正な method は構築時に拒否される
    for &method in &["GET\r\nEvil: header", "POST\r\n", "GET\nEvil", "GET\rEvil"] {
        let result = Request::new(method, "/");
        assert!(
            matches!(result, Err(EncodeError::InvalidMethod { .. })),
            "CRLF in method should be rejected at construction: {:?}",
            method
        );
    }
}

#[test]
fn test_encode_request_crlf_in_uri() {
    // 不正な URI は構築時に拒否される
    for &uri in &["/path\r\nEvil: header", "/\r\n", "/test\nEvil"] {
        let result = Request::new("GET", uri);
        assert!(
            matches!(result, Err(EncodeError::InvalidRequestTarget { .. })),
            "CRLF in URI should be rejected at construction: {:?}",
            uri
        );
    }
}

#[test]
fn test_encode_request_crlf_in_header_name() {
    // 不正なヘッダー名は構築時に拒否される
    for &name in &["Evil\r\nHeader", "Evil\nHeader", "Evil\rHeader"] {
        let req = Request::new("GET", "/")
            .unwrap()
            .header("Host", "example.com")
            .unwrap();
        let result = req.header(name, "value");
        assert!(
            matches!(result, Err(EncodeError::InvalidHeaderName { .. })),
            "CRLF in header name should be rejected at construction: {:?}",
            name
        );
    }
}

#[test]
fn test_encode_request_crlf_in_header_value() {
    // 不正なヘッダー値は構築時に拒否される
    for &value in &["evil\r\nEvil: injected", "evil\ninjected", "evil\rinjected"] {
        let req = Request::new("GET", "/")
            .unwrap()
            .header("Host", "example.com")
            .unwrap();
        let result = req.header("X-Test", value);
        assert!(
            matches!(result, Err(EncodeError::InvalidHeaderValue { .. })),
            "CRLF in header value should be rejected at construction: {:?}",
            value
        );
    }
}

#[test]
fn test_encode_request_nul_in_header_value() {
    // NUL を含むヘッダー値は構築時に拒否される
    let req = Request::new("GET", "/")
        .unwrap()
        .header("Host", "example.com")
        .unwrap();
    let result = req.header("X-Test", "evil\0value");
    assert!(
        matches!(result, Err(EncodeError::InvalidHeaderValue { .. })),
        "NUL in header value should be rejected at construction"
    );
}

#[test]
fn test_encode_response_crlf_in_reason_phrase() {
    // 不正な reason_phrase は構築時に拒否される
    for phrase in &["OK\r\nEvil: header", "OK\n", "OK\r"] {
        let result = Response::new(200, *phrase);
        assert!(
            matches!(result, Err(EncodeError::InvalidReasonPhrase { .. })),
            "CRLF in reason-phrase should be rejected at construction: {:?}",
            phrase
        );
    }
}

#[test]
fn test_encode_response_crlf_in_header_name() {
    // 不正なヘッダー名は構築時に拒否される
    for &name in &["Evil\r\nHeader", "Evil\nHeader"] {
        let res = Response::with_status(StatusCode::OK);
        let result = res.header(name, "value");
        assert!(
            matches!(result, Err(EncodeError::InvalidHeaderName { .. })),
            "CRLF in response header name should be rejected at construction: {:?}",
            name
        );
    }
}

#[test]
fn test_encode_response_crlf_in_header_value() {
    // 不正なヘッダー値は構築時に拒否される
    for &value in &["evil\r\nEvil: injected", "evil\ninjected"] {
        let res = Response::with_status(StatusCode::OK);
        let result = res.header("X-Test", value);
        assert!(
            matches!(result, Err(EncodeError::InvalidHeaderValue { .. })),
            "CRLF in response header value should be rejected at construction: {:?}",
            value
        );
    }
}

// ========================================
// userinfo テスト (RFC 9110 Section 4.2.4)
// ========================================

#[test]
fn test_encode_request_http_userinfo_rejected() {
    // RFC 9110 Section 4.2.4: http URI の userinfo は MUST NOT
    let req = Request::new("GET", "http://user:pass@example.com/path")
        .unwrap()
        .header("Host", "example.com")
        .unwrap();
    assert!(matches!(
        encode_request(&req),
        Err(EncodeError::UserinfoInHttpUri { .. })
    ));
}

#[test]
fn test_encode_request_https_userinfo_rejected() {
    // RFC 9110 Section 4.2.4: https URI の userinfo は MUST NOT
    let req = Request::new("GET", "https://user@example.com/path")
        .unwrap()
        .header("Host", "example.com")
        .unwrap();
    assert!(matches!(
        encode_request(&req),
        Err(EncodeError::UserinfoInHttpUri { .. })
    ));
}

#[test]
fn test_encode_request_http_userinfo_with_port_rejected() {
    let req = Request::new("GET", "http://user@example.com:8080/path")
        .unwrap()
        .header("Host", "example.com:8080")
        .unwrap();
    assert!(matches!(
        encode_request(&req),
        Err(EncodeError::UserinfoInHttpUri { .. })
    ));
}

#[test]
fn test_encode_request_non_http_scheme_userinfo_allowed() {
    // http/https 以外のスキームでは userinfo は許可
    let req = Request::new("GET", "ftp://user@example.com/path")
        .unwrap()
        .header("Host", "example.com")
        .unwrap();
    assert!(encode_request(&req).is_ok());
}

// ========================================
// Content-Length と body.len() の整合性検証テスト
// ========================================

#[test]
fn test_encode_response_content_length_mismatch() {
    // Content-Length と body.len() が不一致 → ContentLengthMismatch エラー
    let res = Response::with_status(StatusCode::OK)
        .header("Content-Length", "10")
        .unwrap()
        .body(b"hello".to_vec());
    let result = encode_response(&res);
    assert!(matches!(
        result,
        Err(EncodeError::ContentLengthMismatch {
            header_value: 10,
            body_length: 5,
        })
    ));
}

#[test]
fn test_encode_request_content_length_mismatch() {
    // Content-Length と body.len() が不一致 → ContentLengthMismatch エラー
    let req = Request::new("POST", "/")
        .unwrap()
        .header("Host", "example.com")
        .unwrap()
        .header("Content-Length", "10")
        .unwrap()
        .body(b"hello".to_vec());
    let result = encode_request(&req);
    assert!(matches!(
        result,
        Err(EncodeError::ContentLengthMismatch {
            header_value: 10,
            body_length: 5,
        })
    ));
}

#[test]
fn test_encode_response_omit_body_without_content_length_does_not_add_header() {
    // omit_body: true かつ body が空の場合、自動で Content-Length を追加しない
    let res = Response::with_status(StatusCode::OK).omit_body(true);
    let encoded = encode_response(&res).unwrap();
    let encoded_str = String::from_utf8_lossy(&encoded);
    assert!(!encoded_str.contains("Content-Length"));
}

#[test]
fn test_encode_response_omit_body_allows_content_length_without_body() {
    // HEAD レスポンス相当: body を送信しないが Content-Length で表現長を返す
    let res = Response::with_status(StatusCode::OK)
        .header("Content-Length", "100")
        .unwrap()
        .omit_body(true);
    let encoded = encode_response(&res).unwrap();
    let encoded_str = String::from_utf8_lossy(&encoded);

    assert!(encoded_str.contains("Content-Length: 100\r\n"));
    assert!(encoded_str.ends_with("\r\n\r\n"));
}

#[test]
fn test_encode_response_omit_body_does_not_encode_body() {
    // omit_body: true の場合、status がボディ許可でも実ボディは送信しない
    let res = Response::with_status(StatusCode::OK)
        .header("Content-Length", "5")
        .unwrap()
        .body(b"hello".to_vec())
        .omit_body(true);
    let encoded = encode_response(&res).unwrap();
    let encoded_str = String::from_utf8_lossy(&encoded);

    assert!(encoded_str.contains("Content-Length: 5\r\n"));
    assert!(encoded_str.ends_with("\r\n\r\n"));
    assert!(!encoded_str.ends_with("hello"));
}

#[test]
fn test_encode_response_omit_body_with_non_empty_body_still_validates_content_length() {
    // omit_body: true でも body を持っている場合は Content-Length 整合性を検証する
    let res = Response::with_status(StatusCode::OK)
        .header("Content-Length", "10")
        .unwrap()
        .body(b"hello".to_vec())
        .omit_body(true);
    let result = encode_response(&res);
    assert!(matches!(
        result,
        Err(EncodeError::ContentLengthMismatch {
            header_value: 10,
            body_length: 5,
        })
    ));
}

#[test]
fn test_encode_response_304_content_length_representation_size_ok() {
    // RFC 9110 Section 8.6: 304 の Content-Length は表現長を示せる
    let res = Response::with_status(StatusCode::NOT_MODIFIED)
        .header("Content-Length", "100")
        .unwrap();
    let encoded = encode_response(&res).unwrap();
    let encoded_str = String::from_utf8_lossy(&encoded);

    assert!(encoded_str.contains("Content-Length: 100\r\n"));
    assert!(encoded_str.ends_with("\r\n\r\n"));
}

#[test]
fn test_encode_response_content_length_with_nbsp_is_rejected() {
    // RFC 9110 Section 5.6.3: OWS = *( SP / HTAB ) のみ。
    // Unicode 空白 (NBSP = U+00A0) は obs-text 経由でヘッダー値に通り得るが、
    // OWS としては扱わず Content-Length 値の DIGIT 検査で拒否する必要がある。
    // HTTP Request Smuggling (CWE-444) の経路を塞ぐ意図のテスト。
    let res = Response::with_status(StatusCode::OK)
        .header("Content-Length", "\u{A0}5")
        .unwrap()
        .body(b"hello".to_vec());
    let result = encode_response(&res);
    assert!(
        matches!(result, Err(EncodeError::InvalidContentLengthValue { .. })),
        "NBSP を含む Content-Length は拒否される想定だが {result:?} だった"
    );
}

#[test]
fn test_encode_request_content_length_with_ideographic_space_is_rejected() {
    // U+3000 (全角空白) も str::trim で除去される Unicode 空白だが OWS ではない
    let req = Request::new("POST", "/")
        .unwrap()
        .header("Host", "example.com")
        .unwrap()
        .header("Content-Length", "\u{3000}5")
        .unwrap()
        .body(b"hello".to_vec());
    let result = encode_request(&req);
    assert!(
        matches!(result, Err(EncodeError::InvalidContentLengthValue { .. })),
        "全角空白を含む Content-Length は拒否される想定だが {result:?} だった"
    );
}

#[test]
fn test_encode_response_content_length_with_sp_htab_is_accepted() {
    // OWS = *( SP / HTAB ) は引き続き正しく trim される
    let res = Response::with_status(StatusCode::OK)
        .header("Content-Length", " \t5\t ")
        .unwrap()
        .body(b"hello".to_vec());
    let encoded = encode_response(&res).expect("SP/HTAB のみの OWS は受理される想定");
    let encoded_str = String::from_utf8_lossy(&encoded);
    assert!(encoded_str.contains("Content-Length:  \t5\t \r\n"));
}

#[test]
fn test_encode_response_204_no_auto_content_length_with_no_body() {
    // 204: status_has_body=false なので Content-Length 自動付与なし (body=None)
    let res = Response::with_status(StatusCode::NO_CONTENT);
    let encoded = encode_response(&res).unwrap();
    let s = String::from_utf8_lossy(&encoded);
    assert!(!s.contains("Content-Length"));
    assert!(s.ends_with("\r\n\r\n"));
}

#[test]
fn test_encode_response_204_no_auto_content_length_with_empty_body() {
    // 204: body=Some(vec![]) でも Content-Length は自動付与しない
    let res = Response::with_status(StatusCode::NO_CONTENT).body(Vec::new());
    let encoded = encode_response(&res).unwrap();
    let s = String::from_utf8_lossy(&encoded);
    assert!(!s.contains("Content-Length"));
    assert!(s.ends_with("\r\n\r\n"));
}

#[test]
fn test_encode_response_205_with_non_empty_body_error() {
    // RFC 9110 Section 15.3.6: 205 はボディを生成してはならない (MUST NOT)
    let res = Response::with_status(StatusCode::RESET_CONTENT).body(b"hello".to_vec());
    let result = encode_response(&res);
    assert!(matches!(result, Err(EncodeError::ForbiddenBodyFor205)));
}

#[test]
fn test_encode_response_omit_body_with_explicit_empty_body_does_not_add_content_length() {
    // omit_body=true かつ body=Some(vec![]) のケースで Content-Length を自動付与しない
    // (encoder の (omit_body, body_len) == (true, Some(0)) 分岐を固定)
    let res = Response::with_status(StatusCode::OK)
        .body(Vec::new())
        .omit_body(true);
    let encoded = encode_response(&res).unwrap();
    let s = String::from_utf8_lossy(&encoded);
    assert!(!s.contains("Content-Length"));
    assert!(s.ends_with("\r\n\r\n"));
}

#[test]
fn test_encode_request_absolute_form_at_in_userinfo() {
    // RFC 9110 Section 4.2.4: userinfo の "@" は http URI で禁止
    let req = Request::new("GET", "http://user%40name@example.com/path")
        .unwrap()
        .header("Host", "example.com")
        .unwrap();
    assert!(matches!(
        encode_request(&req),
        Err(EncodeError::UserinfoInHttpUri { .. })
    ));
}

// ========================================
// CONNECT リクエストのヘッダー・ボディ許容テスト (RFC 9110 Section 9.3.6)
//
// RFC 9110 Section 9.3.6 の文言:
//   "A CONNECT request message does not have content."
//
// これは事実の記述 ("does not") であり、MUST NOT による禁止ではない。
// また RFC は CONNECT リクエスト側に対して Content-Length / Transfer-Encoding を
// MUST NOT とは規定していない。MUST NOT なのは CONNECT の 2xx レスポンス側
// (RFC 9110 Section 9.3.6):
//   "A server MUST NOT send any Transfer-Encoding or Content-Length
//    header fields in a 2xx (Successful) response to CONNECT."
//
// Content-Length については RFC 9110 Section 8.6 で:
//   "A user agent SHOULD NOT send a Content-Length header field when
//    the request message does not contain content and the method
//    semantics do not anticipate such data."
// と SHOULD NOT に留まる。
//
// したがってエンコーダーは CONNECT リクエストのヘッダーやボディの有無で reject せず、
// CONNECT の意味論的制約の判断はアプリケーション層の責務とする。
// ========================================

/// CONNECT リクエストに Content-Length / Transfer-Encoding / body があっても
/// エンコーダーは reject しない。
#[test]
fn test_encode_request_connect_accepts_body_headers() {
    // body 付き + Content-Length
    let req = Request::new("CONNECT", "example.com:443")
        .unwrap()
        .header("Host", "example.com:443")
        .unwrap()
        .header("Content-Length", "5")
        .unwrap()
        .body(b"hello".to_vec());
    assert!(encode_request(&req).is_ok());

    // Content-Length: 0 (content がないことの明示)
    let req = Request::new("CONNECT", "example.com:443")
        .unwrap()
        .header("Host", "example.com:443")
        .unwrap()
        .header("Content-Length", "0")
        .unwrap();
    assert!(encode_request(&req).is_ok());

    // Transfer-Encoding: chunked
    let req = Request::new("CONNECT", "example.com:443")
        .unwrap()
        .header("Host", "example.com:443")
        .unwrap()
        .header("Transfer-Encoding", "chunked")
        .unwrap();
    assert!(encode_request(&req).is_ok());

    // body なし (最も一般的なケース)
    let req = Request::new("CONNECT", "example.com:443")
        .unwrap()
        .header("Host", "example.com:443")
        .unwrap();
    assert!(encode_request(&req).is_ok());
}

/// encode_request_headers でも CONNECT + Content-Length をエンコードできる。
/// encode_request_headers は body を扱わないヘッダーのみのエンコードだが、
/// CONNECT 専用の制約は RFC に根拠がないため適用しない。
#[test]
fn test_encode_request_headers_connect_accepts_content_length() {
    // Content-Length: 0
    let req = Request::new("CONNECT", "example.com:443")
        .unwrap()
        .header("Host", "example.com:443")
        .unwrap()
        .header("Content-Length", "0")
        .unwrap();
    assert!(encode_request_headers(&req).is_ok());

    // Content-Length: N > 0
    let req = Request::new("CONNECT", "example.com:443")
        .unwrap()
        .header("Host", "example.com:443")
        .unwrap()
        .header("Content-Length", "10")
        .unwrap();
    assert!(encode_request_headers(&req).is_ok());
}

// ========================================
// method/request-target form 整合性テスト (RFC 9112 Section 3.2)
// ========================================

#[test]
fn test_encode_request_connect_authority_form_ok() {
    // CONNECT は authority-form (host:port) のみ許可
    let req = Request::new("CONNECT", "example.com:443")
        .unwrap()
        .header("Host", "example.com:443")
        .unwrap();
    let result = encode_request(&req);
    assert!(result.is_ok());
}

#[test]
fn test_encode_request_connect_origin_form_error() {
    // CONNECT で origin-form は不正
    let req = Request::new("CONNECT", "/path")
        .unwrap()
        .header("Host", "example.com")
        .unwrap();
    let result = encode_request(&req);
    assert!(matches!(
        result,
        Err(EncodeError::InvalidRequestTargetForm { .. })
    ));
}

#[test]
fn test_encode_request_connect_absolute_form_error() {
    // CONNECT で absolute-form は不正
    let req = Request::new("CONNECT", "http://example.com/path")
        .unwrap()
        .header("Host", "example.com")
        .unwrap();
    let result = encode_request(&req);
    assert!(matches!(
        result,
        Err(EncodeError::InvalidRequestTargetForm { .. })
    ));
}

#[test]
fn test_encode_request_connect_asterisk_form_error() {
    // CONNECT で asterisk-form は不正
    let req = Request::new("CONNECT", "*")
        .unwrap()
        .header("Host", "example.com")
        .unwrap();
    let result = encode_request(&req);
    assert!(matches!(
        result,
        Err(EncodeError::InvalidRequestTargetForm { .. })
    ));
}

#[test]
fn test_encode_request_options_asterisk_form_ok() {
    // OPTIONS * は asterisk-form 許可
    let req = Request::new("OPTIONS", "*")
        .unwrap()
        .header("Host", "example.com")
        .unwrap();
    let result = encode_request(&req);
    assert!(result.is_ok());
}

#[test]
fn test_encode_request_get_asterisk_form_error() {
    // GET で asterisk-form は不正
    let req = Request::new("GET", "*")
        .unwrap()
        .header("Host", "example.com")
        .unwrap();
    let result = encode_request(&req);
    assert!(matches!(
        result,
        Err(EncodeError::InvalidRequestTargetForm { .. })
    ));
}

#[test]
fn test_encode_request_get_authority_form_error() {
    // GET で authority-form は不正
    let req = Request::new("GET", "example.com:80")
        .unwrap()
        .header("Host", "example.com")
        .unwrap();
    let result = encode_request(&req);
    assert!(matches!(
        result,
        Err(EncodeError::InvalidRequestTargetForm { .. })
    ));
}

// ========================================
// absolute-form 判定テスト ("://" なし absolute-URI)
// ========================================

#[test]
fn test_encode_request_absolute_form_without_double_slash_ok() {
    // "://" を含まない absolute-URI (urn:isbn:...) は absolute-form
    let req = Request::with_version("GET", "urn:isbn:0451450523", "HTTP/1.0").unwrap();
    let result = encode_request(&req);
    assert!(result.is_ok());
}

#[test]
fn test_encode_request_absolute_form_urn_nid_nss() {
    // urn:nid:nss 形式の absolute-URI
    let req = Request::with_version("GET", "urn:example:animal:ferret:nose", "HTTP/1.0").unwrap();
    let result = encode_request(&req);
    assert!(result.is_ok());
}

// ========================================
// authority なし URI の Host 検証テスト (RFC 9112 Section 3.2)
// ========================================

#[test]
fn test_encode_request_authority_less_uri_non_empty_host_error() {
    // authority がない absolute-form で Host が非空はエラー
    let req = Request::new("GET", "urn:isbn:0451450523")
        .unwrap()
        .header("Host", "example.com")
        .unwrap();
    let result = encode_request(&req);
    assert!(matches!(
        result,
        Err(EncodeError::NonEmptyHostWithoutAuthority { .. })
    ));
}

#[test]
fn test_encode_request_authority_less_uri_empty_host_ok() {
    // authority がない absolute-form で Host が空は OK
    let req = Request::new("GET", "urn:isbn:0451450523")
        .unwrap()
        .header("Host", "")
        .unwrap();
    let result = encode_request(&req);
    assert!(result.is_ok());
}

// ========================================
// http/https 空 host 拒否テスト (RFC 9110 Section 4.2)
// ========================================

#[test]
fn test_encode_request_http_empty_host_error() {
    // http:///path は空 host で不正
    let req = Request::new("GET", "http:///path")
        .unwrap()
        .header("Host", "")
        .unwrap();
    let result = encode_request(&req);
    assert!(matches!(
        result,
        Err(EncodeError::EmptyHostInHttpUri { .. })
    ));
}

#[test]
fn test_encode_request_https_port_only_host_error() {
    // https://:443/path は空 host で不正
    let req = Request::new("GET", "https://:443/path")
        .unwrap()
        .header("Host", "")
        .unwrap();
    let result = encode_request(&req);
    assert!(matches!(
        result,
        Err(EncodeError::EmptyHostInHttpUri { .. })
    ));
}

#[test]
fn test_encode_request_authority_form_still_works() {
    // 通常の authority-form (host:port) は引き続き authority-form と判定
    let req = Request::new("CONNECT", "example.com:443")
        .unwrap()
        .header("Host", "example.com:443")
        .unwrap();
    let result = encode_request(&req);
    assert!(result.is_ok());
}

// ========================================
// http/https URI の :// 必須検証 (RFC 9110 Section 4.2)
// ========================================

#[test]
fn test_encode_request_http_without_double_slash_error() {
    // http:foo は "://" がないので不正
    let req = Request::new("GET", "http:foo")
        .unwrap()
        .header("Host", "")
        .unwrap();
    let result = encode_request(&req);
    assert!(matches!(
        result,
        Err(EncodeError::InvalidRequestTarget { .. })
    ));
}

#[test]
fn test_encode_request_https_without_double_slash_error() {
    // https:path は "://" がないので不正
    let req = Request::new("GET", "https:path")
        .unwrap()
        .header("Host", "")
        .unwrap();
    let result = encode_request(&req);
    assert!(matches!(
        result,
        Err(EncodeError::InvalidRequestTarget { .. })
    ));
}

#[test]
fn test_encode_request_non_http_scheme_without_double_slash_ok() {
    // urn:isbn:xxx は http/https ではないので OK
    let req = Request::new("GET", "urn:isbn:0451450523")
        .unwrap()
        .header("Host", "")
        .unwrap();
    let result = encode_request(&req);
    assert!(result.is_ok());
}

// ========================================
// CONNECT authority-form のホスト検証 (RFC 9112 Section 3.2.3)
// ========================================

#[test]
fn test_encode_request_connect_userinfo_error() {
    // user@example.com:443 は authority-form として不正 (userinfo を含む)
    let req = Request::new("CONNECT", "user@example.com:443")
        .unwrap()
        .header("Host", "example.com:443")
        .unwrap();
    let result = encode_request(&req);
    assert!(matches!(
        result,
        Err(EncodeError::InvalidRequestTargetForm { .. })
            | Err(EncodeError::InvalidRequestTarget { .. })
    ));
}

#[test]
fn test_encode_request_connect_empty_host_error() {
    // :443 は authority-form として不正 (ホストが空)
    let req = Request::new("CONNECT", ":443")
        .unwrap()
        .header("Host", "")
        .unwrap();
    let result = encode_request(&req);
    assert!(result.is_err());
}

// ========================================
// authority 付き URI で空 Host の拒否 (RFC 9112 Section 3.2)
// ========================================

#[test]
fn test_encode_request_empty_host_with_authority_uri_error() {
    // URI に authority があるのに Host が空は不正
    let req = Request::new("GET", "http://example.com/path")
        .unwrap()
        .header("Host", "")
        .unwrap();
    let result = encode_request(&req);
    assert!(matches!(
        result,
        Err(EncodeError::HostAuthorityMismatch { .. })
    ));
}

#[test]
fn test_encode_request_matching_host_with_authority_uri_ok() {
    // URI の authority と Host が一致する場合は OK
    let req = Request::new("GET", "http://example.com/path")
        .unwrap()
        .header("Host", "example.com")
        .unwrap();
    let result = encode_request(&req);
    assert!(result.is_ok());
}

// ========================================
// Content-Length ABNF 検証テスト (RFC 9110 Section 8.6)
// ========================================

#[test]
fn test_encode_request_non_numeric_content_length() {
    // 非数値の Content-Length はエラー
    let req = Request::new("POST", "/")
        .unwrap()
        .header("Host", "example.com")
        .unwrap()
        .header("Content-Length", "abc")
        .unwrap()
        .body(b"hello".to_vec());
    let result = encode_request(&req);
    assert!(matches!(
        result,
        Err(EncodeError::InvalidContentLengthValue { .. })
    ));
}

#[test]
fn test_encode_response_non_numeric_content_length() {
    // 非数値の Content-Length はエラー
    let res = Response::with_status(StatusCode::OK)
        .header("Content-Length", "abc")
        .unwrap()
        .body(b"hello".to_vec());
    let result = encode_response(&res);
    assert!(matches!(
        result,
        Err(EncodeError::InvalidContentLengthValue { .. })
    ));
}

#[test]
fn test_encode_request_duplicate_content_length_mismatch() {
    // 重複 Content-Length で値が不一致はエラー
    let req = Request::new("POST", "/")
        .unwrap()
        .header("Host", "example.com")
        .unwrap()
        .header("Content-Length", "5")
        .unwrap()
        .header("Content-Length", "10")
        .unwrap()
        .body(b"hello".to_vec());
    let result = encode_request(&req);
    assert!(matches!(result, Err(EncodeError::DuplicateContentLength)));
}

#[test]
fn test_encode_response_duplicate_content_length_mismatch() {
    // 重複 Content-Length で値が不一致はエラー
    let res = Response::with_status(StatusCode::OK)
        .header("Content-Length", "5")
        .unwrap()
        .header("Content-Length", "10")
        .unwrap()
        .body(b"hello".to_vec());
    let result = encode_response(&res);
    assert!(matches!(result, Err(EncodeError::DuplicateContentLength)));
}

#[test]
fn test_encode_request_duplicate_content_length_same_value() {
    // 同一値の重複 Content-Length は通過する
    let req = Request::new("POST", "/")
        .unwrap()
        .header("Host", "example.com")
        .unwrap()
        .header("Content-Length", "5")
        .unwrap()
        .header("Content-Length", "5")
        .unwrap()
        .body(b"hello".to_vec());
    let result = encode_request(&req);
    assert!(result.is_ok());
}

// `write_hex_usize` / `write_usize_decimal` の桁繰り上がり境界値テスト
// (ヘルパーは encoder.rs のプライベート関数なので公開 API 経由で検証する)

#[test]
fn test_encode_chunk_hex_boundaries() {
    // 桁の境界を跨ぐ data.len() で先頭の hex が format!("{:x}") と一致する
    for &len in &[0usize, 1, 15, 16, 255, 256] {
        let data = vec![b'a'; len];
        let encoded = encode_chunk(&data);
        if len == 0 {
            assert_eq!(encoded, b"0\r\n\r\n");
        } else {
            let expected_hex = format!("{:x}\r\n", len);
            assert!(
                encoded.starts_with(expected_hex.as_bytes()),
                "len={len}: encoded does not start with {expected_hex:?}",
            );
            assert!(encoded.ends_with(b"\r\n"));
            let data_start = expected_hex.len();
            let data_end = encoded.len() - 2;
            assert_eq!(&encoded[data_start..data_end], data.as_slice());
        }
    }
}

#[test]
fn test_encode_response_status_code_decimal_boundaries() {
    // status_code 100 / 200 / 999 のステータスラインが format!("{}") と一致する
    for &code in &[100u16, 200, 599] {
        let res = Response::new(code, "Reason").unwrap().body(Vec::new());
        let encoded = encode_response(&res).unwrap();
        let expected_status_line = format!("HTTP/1.1 {code} Reason\r\n");
        assert!(
            encoded.starts_with(expected_status_line.as_bytes()),
            "code={code}: encoded does not start with {expected_status_line:?}",
        );
    }
}

#[test]
fn test_encode_response_content_length_decimal_boundaries() {
    // body.len() の桁の境界 (0, 9, 10, 99, 100) で Content-Length が format!("{}") と一致する
    for &len in &[0usize, 9, 10, 99, 100] {
        let body = vec![b'x'; len];
        let res = Response::with_status(StatusCode::OK).body(body.clone());
        let encoded = encode_response(&res).unwrap();
        let encoded_str = core::str::from_utf8(&encoded).unwrap();
        let expected_header = format!("Content-Length: {len}\r\n");
        assert!(
            encoded_str.contains(&expected_header),
            "len={len}: encoded does not contain {expected_header:?}",
        );
    }
}

#[test]
fn test_encode_request_content_length_decimal_boundaries() {
    // body.len() の桁の境界 (0, 9, 10, 99, 100) で Content-Length が format!("{}") と一致する
    for &len in &[0usize, 9, 10, 99, 100] {
        let body = vec![b'x'; len];
        let req = Request::new("POST", "/")
            .unwrap()
            .header("Host", "example.com")
            .unwrap()
            .body(body.clone());
        let encoded = encode_request(&req).unwrap();
        let encoded_str = core::str::from_utf8(&encoded).unwrap();
        let expected_header = format!("Content-Length: {len}\r\n");
        assert!(
            encoded_str.contains(&expected_header),
            "len={len}: encoded does not contain {expected_header:?}",
        );
    }
}

#[test]
fn test_encode_response_headers_status_code_decimal_boundaries() {
    // encode_response_headers 経路でも status_code が format!("{}") と一致する
    for &code in &[100u16, 200, 599] {
        let res = Response::new(code, "Reason").unwrap();
        let encoded = encode_response_headers(&res).unwrap();
        let expected_status_line = format!("HTTP/1.1 {code} Reason\r\n");
        assert!(
            encoded.starts_with(expected_status_line.as_bytes()),
            "code={code}: encoded does not start with {expected_status_line:?}",
        );
    }
}
