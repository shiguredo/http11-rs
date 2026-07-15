//! Request の構築時バリデーションテスト
//!
//! `Request::new` / `Request::with_version` / `Request::header` / `Request::add_header`
//! / `Request::set_header` の各バリデーション分岐を検証する。
//! HTTP Request Smuggling (CWE-444) の典型的なペイロードを構築時に拒否することも確認する。

use shiguredo_http11::{EncodeError, HeaderName, Method, Request};

// ========================================
// method バリデーション
// ========================================

#[test]
fn test_method_rejects_empty() {
    let result = Method::new(b"");
    assert!(result.is_err());
}

#[test]
fn test_method_rejects_crlf() {
    for method in [
        b"GET\r\nX: y".as_slice(),
        b"GET\r",
        b"GET\n",
        b"POST\r\nEvil: header",
    ] {
        let result = Method::new(method);
        assert!(result.is_err(), "method {:?} should be rejected", method);
    }
}

#[test]
fn test_method_rejects_space() {
    let result = Method::new(b"GE T");
    assert!(result.is_err());
}

#[test]
fn test_method_rejects_invalid_token_chars() {
    // RFC 9110 Section 5.6.2 token に違反する文字
    for method in [b"GET/", b"GET@", b"GET[", b"GET{", b"GET\""] {
        let result = Method::new(method);
        assert!(result.is_err(), "method {:?} should be rejected", method);
    }
}

#[test]
fn test_request_new_accepts_standard_methods() {
    for method in [
        Method::GET,
        Method::POST,
        Method::PUT,
        Method::DELETE,
        Method::HEAD,
        Method::OPTIONS,
        Method::PATCH,
        Method::CONNECT,
        Method::TRACE,
        Method::QUERY,
    ] {
        let result = Request::new(method, "/");
        assert!(result.is_ok());
    }
}

#[test]
fn test_request_new_accepts_custom_method_token() {
    // RTSP の GET_PARAMETER も tchar
    let result = Request::new(
        Method::from_static(b"GET_PARAMETER"),
        "rtsp://example.com/media",
    );
    assert!(result.is_ok());
}

// ========================================
// uri (request-target) バリデーション
// ========================================

#[test]
fn test_request_new_rejects_empty_uri() {
    let result = Request::new(Method::GET, "");
    assert!(matches!(
        result,
        Err(EncodeError::InvalidRequestTarget { .. })
    ));
}

#[test]
fn test_request_new_rejects_uri_with_crlf() {
    for &uri in &[
        "/path\r\nEvil: header",
        "/\r\n",
        "/test\nEvil",
        "/test\rEvil",
    ] {
        let result = Request::new(Method::GET, uri);
        assert!(
            matches!(result, Err(EncodeError::InvalidRequestTarget { .. })),
            "uri {:?} should be rejected",
            uri
        );
    }
}

#[test]
fn test_request_new_rejects_uri_with_nul() {
    let result = Request::new(Method::GET, "/path\0bad");
    assert!(matches!(
        result,
        Err(EncodeError::InvalidRequestTarget { .. })
    ));
}

#[test]
fn test_request_new_rejects_uri_with_space() {
    let result = Request::new(Method::GET, "/path with space");
    assert!(matches!(
        result,
        Err(EncodeError::InvalidRequestTarget { .. })
    ));
}

#[test]
fn test_request_new_rejects_uri_with_control_chars() {
    let result = Request::new(Method::GET, "/path\x01bad");
    assert!(matches!(
        result,
        Err(EncodeError::InvalidRequestTarget { .. })
    ));
}

#[test]
fn test_request_new_rejects_uri_with_rfc3986_excluded_chars() {
    // RFC 3986 で除外されている文字
    for &uri in &[
        "/path<bad",
        "/path>bad",
        "/path|bad",
        "/path{bad",
        "/path}bad",
        "/path^bad",
        "/path`bad",
        "/path\\bad",
        "/path\"bad",
        "/path#bad",
    ] {
        let result = Request::new(Method::GET, uri);
        assert!(
            matches!(result, Err(EncodeError::InvalidRequestTarget { .. })),
            "uri {:?} should be rejected",
            uri
        );
    }
}

#[test]
fn test_request_new_rejects_uri_with_obs_text() {
    // 送信側ポリシーとして obs-text (0x80-0xFF) を拒否
    let uri = "/path\u{0080}bad";
    let result = Request::new(Method::GET, uri);
    assert!(matches!(
        result,
        Err(EncodeError::InvalidRequestTarget { .. })
    ));
}

#[test]
fn test_request_new_rejects_uri_with_pct_nul() {
    // %00 (パーセントエンコーディングされた NUL) は smuggling ペイロード
    let result = Request::new(Method::GET, "/path%00bad");
    assert!(matches!(
        result,
        Err(EncodeError::InvalidRequestTarget { .. })
    ));
}

#[test]
fn test_request_new_rejects_uri_with_invalid_percent_encoding() {
    for &uri in &["/path%xx", "/path%0", "/path%"] {
        let result = Request::new(Method::GET, uri);
        assert!(
            matches!(result, Err(EncodeError::InvalidRequestTarget { .. })),
            "uri {:?} should be rejected",
            uri
        );
    }
}

// ========================================
// with_version: version バリデーション
// ========================================

#[test]
fn test_request_with_version_rejects_empty_version() {
    let result = Request::with_version(Method::GET, "/", "");
    assert!(matches!(result, Err(EncodeError::InvalidVersion { .. })));
}

#[test]
fn test_request_with_version_rejects_invalid_format() {
    for &version in &[
        "HTTP",
        "/1.1",
        "HTTP/",
        "HTTP/1",
        "HTTP/1.",
        "HTTP/.1",
        "HTTP/abc.def",
    ] {
        let result = Request::with_version(Method::GET, "/", version);
        assert!(
            matches!(result, Err(EncodeError::InvalidVersion { .. })),
            "version {:?} should be rejected",
            version
        );
    }
}

#[test]
fn test_request_with_version_rejects_version_with_space() {
    let result = Request::with_version(Method::GET, "/", "HTTP /1.1");
    assert!(matches!(result, Err(EncodeError::InvalidVersion { .. })));
}

#[test]
fn test_request_with_version_accepts_http_versions() {
    for &version in &["HTTP/1.1", "HTTP/1.0", "HTTP/0.9", "HTTP/2.0"] {
        let result = Request::with_version(Method::GET, "/", version);
        assert!(result.is_ok(), "バージョン {:?} は受理されるべき", version);
    }
}

#[test]
fn test_request_with_version_accepts_rtsp_versions() {
    for &version in &["RTSP/1.0", "RTSP/2.0"] {
        let result = Request::with_version(Method::GET, "rtsp://example.com/m", version);
        assert!(result.is_ok(), "バージョン {:?} は受理されるべき", version);
    }
}

// ========================================
// 構築時のバリデーション順序
// ========================================

#[test]
fn test_method_validation_rejects_invalid_before_request_new() {
    // method のバリデーションは Method::new で行われる
    let result = Method::new(b"BAD\r\n");
    assert!(result.is_err());
}

#[test]
fn test_method_validation_rejects_invalid_before_with_version() {
    // method のバリデーションは Method::new で行われる
    let result = Method::new(b"BAD\r\n");
    assert!(result.is_err());
}

#[test]
fn test_request_with_version_validation_order_uri_before_version() {
    // method 成功 + uri 失敗 + version 失敗 → uri のエラーが先に返る
    let result = Request::with_version(Method::GET, "/bad\r\n", "BAD\r\n");
    assert!(matches!(
        result,
        Err(EncodeError::InvalidRequestTarget { .. })
    ));
}

// ========================================
// ヘッダー名/値のバリデーション
// ========================================

#[test]
fn test_header_name_rejects_space() {
    let result = HeaderName::new(b"Bad Name");
    assert!(result.is_err());
}

#[test]
fn test_header_name_rejects_empty() {
    let result = HeaderName::new(b"");
    assert!(result.is_err());
}

#[test]
fn test_request_header_rejects_crlf_in_value() {
    let req = Request::new(Method::GET, "/").unwrap();
    for &value in &["evil\r\nEvil: injected", "evil\rinjected", "evil\ninjected"] {
        let r = req
            .clone()
            .header(HeaderName::from_static(b"X-Test"), value);
        assert!(
            matches!(r, Err(EncodeError::InvalidHeaderValue { .. })),
            "value {:?} should be rejected",
            value
        );
    }
}

#[test]
fn test_request_header_rejects_nul_in_value() {
    let req = Request::new(Method::GET, "/").unwrap();
    let result = req.header(HeaderName::from_static(b"X-Test"), "evil\0value");
    assert!(matches!(
        result,
        Err(EncodeError::InvalidHeaderValue { .. })
    ));
}

#[test]
fn test_request_header_accepts_empty_value() {
    // RFC 9110 Section 5.5: field-value = *field-content (空値は合法)
    let req = Request::new(Method::GET, "/").unwrap();
    let result = req.header(HeaderName::from_static(b"X-Empty"), "");
    assert!(result.is_ok());
}

#[test]
fn test_request_header_accepts_value_with_leading_trailing_whitespace() {
    // RFC 9110 §5.5 では「A field parsing implementation MUST exclude such whitespace」
    // と先頭/末尾空白の除外を要求しているが、現状では smuggling 防御に注力するため
    // trim を行わない。将来 trim を導入する予定の暫定動作。
    let req = Request::new(Method::GET, "/").unwrap();
    let result = req.header(
        HeaderName::from_static(b"X-Test"),
        " value with leading space ",
    );
    assert!(result.is_ok());
}

// ========================================
// add_header の挙動確認
// ========================================

#[test]
fn test_request_add_header_appends_duplicate_names() {
    let mut req = Request::new(Method::GET, "/").unwrap();
    req.add_header(HeaderName::from_static(b"Set-Cookie"), "a=1")
        .unwrap();
    req.add_header(HeaderName::from_static(b"Set-Cookie"), "b=2")
        .unwrap();
    let cookies = req.get_headers("Set-Cookie");
    assert_eq!(cookies, vec!["a=1", "b=2"]);
}

#[test]
fn test_request_add_header_does_not_modify_self_on_invalid_name() {
    // HeaderName が構築時に検証するため、不正な名前は add_header に到達しない
    assert!(HeaderName::new(b"Bad Name").is_err());

    // 有効な名前は正常に動作する
    let mut req = Request::new(Method::GET, "/").unwrap();
    req.add_header(HeaderName::from_static(b"Host"), "example.com")
        .unwrap();
    assert_eq!(req.get_header("Host"), Some("example.com"));
}

#[test]
fn test_request_add_header_does_not_modify_self_on_invalid_value() {
    let mut req = Request::new(Method::GET, "/").unwrap();
    req.add_header(HeaderName::from_static(b"Host"), "example.com")
        .unwrap();
    let result = req.add_header(HeaderName::from_static(b"X-Test"), "evil\r\ninjected");
    assert!(matches!(
        result,
        Err(EncodeError::InvalidHeaderValue { .. })
    ));
    // X-Test は追加されていない
    assert!(req.get_header("X-Test").is_none());
}

// ========================================
// set_header の挙動確認 (case-insensitive 上書き、アトミック性)
// ========================================

#[test]
fn test_request_set_header_overwrites_existing() {
    let mut req = Request::new(Method::GET, "/").unwrap();
    req.add_header(HeaderName::from_static(b"Host"), "old.example.com")
        .unwrap();
    req.set_header(HeaderName::from_static(b"Host"), "new.example.com")
        .unwrap();
    assert_eq!(req.get_header("Host"), Some("new.example.com"));
    assert_eq!(req.get_headers("Host").len(), 1);
}

#[test]
fn test_request_set_header_overwrites_case_insensitively() {
    let mut req = Request::new(Method::GET, "/").unwrap();
    req.add_header(HeaderName::from_static(b"Host"), "old.example.com")
        .unwrap();
    // 大文字小文字違いでも同名と判定される
    req.set_header(HeaderName::from_static(b"HOST"), "new.example.com")
        .unwrap();
    let hosts = req.get_headers("Host");
    assert_eq!(hosts, vec!["new.example.com"]);
}

#[test]
fn test_request_set_header_removes_all_duplicates() {
    let mut req = Request::new(Method::GET, "/").unwrap();
    req.add_header(HeaderName::from_static(b"X-Test"), "v1")
        .unwrap();
    req.add_header(HeaderName::from_static(b"X-Test"), "v2")
        .unwrap();
    req.add_header(HeaderName::from_static(b"X-Test"), "v3")
        .unwrap();
    req.set_header(HeaderName::from_static(b"X-Test"), "new")
        .unwrap();
    let values = req.get_headers("X-Test");
    assert_eq!(values, vec!["new"]);
}

#[test]
fn test_request_set_header_atomicity_on_invalid_name() {
    // HeaderName が構築時に検証するため、不正な名前は set_header に到達しない
    assert!(HeaderName::new(b"Bad Name").is_err());
}

#[test]
fn test_request_set_header_atomicity_on_invalid_value() {
    let mut req = Request::new(Method::GET, "/").unwrap();
    req.add_header(HeaderName::from_static(b"Host"), "example.com")
        .unwrap();
    // 既存の Host を上書きしようとして失敗 → Host が消えないこと
    let result = req.set_header(HeaderName::from_static(b"Host"), "evil\r\ninjected");
    assert!(matches!(
        result,
        Err(EncodeError::InvalidHeaderValue { .. })
    ));
    assert_eq!(req.get_header("Host"), Some("example.com"));
}

#[test]
fn test_request_set_header_inserts_when_not_present() {
    let mut req = Request::new(Method::GET, "/").unwrap();
    req.set_header(HeaderName::from_static(b"Host"), "example.com")
        .unwrap();
    assert_eq!(req.get_header("Host"), Some("example.com"));
}

// ========================================
// HTTP Request Smuggling (CWE-444) ペイロード拒否
// ========================================

#[test]
fn test_request_rejects_smuggling_te_cl_via_crlf_in_value() {
    // CRLF 注入による TE/CL 競合の偽装を構築時に拒否する
    let req = Request::new(Method::POST, "/").unwrap();
    let result = req.header(
        HeaderName::from_static(b"Transfer-Encoding"),
        "chunked\r\nContent-Length: 0",
    );
    assert!(matches!(
        result,
        Err(EncodeError::InvalidHeaderValue { .. })
    ));
}

#[test]
fn test_request_rejects_smuggling_method_crlf_injection() {
    // method への CRLF 注入を Method::new で拒否する
    let result = Method::new(b"GET\r\nX: y");
    assert!(result.is_err());
}

#[test]
fn test_request_rejects_smuggling_uri_sp_injection() {
    // URI への SP 注入 (smuggling の典型ペイロード) を構築時に拒否する
    let result = Request::new(Method::GET, "/api?q=x HTTP/1.1\r\nGET /admin");
    assert!(matches!(
        result,
        Err(EncodeError::InvalidRequestTarget { .. })
    ));
}

#[test]
fn test_request_rejects_smuggling_uri_pct_nul() {
    // URI への %00 NUL エンコーディング (smuggling ペイロード) を構築時に拒否する
    let result = Request::new(Method::GET, "/path%00bad");
    assert!(matches!(
        result,
        Err(EncodeError::InvalidRequestTarget { .. })
    ));
}

#[test]
fn test_request_rejects_smuggling_header_name_with_crlf() {
    // HeaderName が CRLF を含む名前を構築時に拒否する
    let result = HeaderName::new(b"Bad\r\nEvil");
    assert!(result.is_err());
}

// ========================================
// アクセサ
// ========================================

#[test]
fn test_request_accessors() {
    let req = Request::new(Method::GET, "/path")
        .unwrap()
        .header(HeaderName::from_static(b"Host"), "example.com")
        .unwrap()
        .body(b"hello".to_vec());
    assert_eq!(req.method(), "GET");
    assert_eq!(req.uri(), "/path");
    assert_eq!(req.version(), "HTTP/1.1");
    assert_eq!(req.body_bytes(), Some(b"hello".as_slice()));
    assert_eq!(req.get_header("Host"), Some("example.com"));
    assert!(req.has_header("Host"));
    assert!(!req.has_header("X-Missing"));
}

#[test]
fn test_request_body_bytes_none_by_default() {
    let req = Request::new(Method::GET, "/").unwrap();
    assert!(req.body_bytes().is_none());
}

#[test]
fn test_request_body_bytes_some_empty() {
    let req = Request::new(Method::POST, "/").unwrap().body(Vec::new());
    assert_eq!(req.body_bytes(), Some(b"".as_slice()));
}

// ========================================
// API 対称化テスト (Response との対称化)
// ========================================

/// without_body() / set_body() / clear_body() の動作
#[test]
fn test_request_body_mutators() {
    let mut req = Request::new(Method::POST, "/").unwrap();
    req.set_body(b"hello".to_vec());
    assert_eq!(req.body_bytes(), Some(b"hello".as_slice()));

    req.clear_body();
    assert_eq!(req.body_bytes(), None);

    let req = Request::new(Method::POST, "/")
        .unwrap()
        .body(b"hello".to_vec())
        .without_body();
    assert_eq!(req.body_bytes(), None);
}

// ========================================
// is_keep_alive の HTTP/1.1 完全一致
// ========================================

/// HTTP/1.1 で Connection ヘッダーなし → keep-alive
#[test]
fn test_request_is_keep_alive_http11_default() {
    let req = Request::new(Method::GET, "/").unwrap();
    assert!(req.is_keep_alive(), "HTTP/1.1 はデフォルトで keep-alive");
}

/// HTTP/1.0 で Connection ヘッダーなし → keep-alive ではない
#[test]
fn test_request_is_keep_alive_http10_default_false() {
    let req = Request::with_version(Method::GET, "/", "HTTP/1.0").unwrap();
    assert!(!req.is_keep_alive(), "HTTP/1.0 はデフォルトで非 keep-alive");
}

/// HTTP/1.1 で Connection: close → keep-alive ではない
#[test]
fn test_request_is_keep_alive_http11_with_close() {
    let req = Request::new(Method::GET, "/")
        .unwrap()
        .header(HeaderName::from_static(b"Connection"), "close")
        .unwrap();
    assert!(!req.is_keep_alive());
}

/// HTTP/1.0 で Connection: keep-alive → keep-alive
#[test]
fn test_request_is_keep_alive_http10_with_keep_alive() {
    let req = Request::with_version(Method::GET, "/", "HTTP/1.0")
        .unwrap()
        .header(HeaderName::from_static(b"Connection"), "keep-alive")
        .unwrap();
    assert!(req.is_keep_alive());
}

/// RTSP/1.1 / FOO/1.1 等は HTTP/1.1 と区別され、Connection ヘッダーなしでは keep-alive にならない
///
/// 旧実装 `version.ends_with("/1.1")` だと RTSP/1.1 で誤って true を返していた。
/// 修正後は `version == "HTTP/1.1"` 完全一致のみ true。
#[test]
fn test_request_is_keep_alive_rtsp_or_foo_11_not_keep_alive_by_default() {
    let req = Request::with_version(
        Method::from_static(b"DESCRIBE"),
        "rtsp://example.com/m",
        "RTSP/1.1",
    )
    .unwrap();
    assert!(
        !req.is_keep_alive(),
        "RTSP/1.1 はデフォルトで HTTP の keep-alive 判定対象外"
    );

    // 独自プロトコル
    let req = Request::with_version(Method::GET, "/", "FOO/1.1").unwrap();
    assert!(
        !req.is_keep_alive(),
        "独自プロトコル FOO/1.1 もデフォルトで keep-alive 対象外"
    );

    // ただし Connection: keep-alive で明示的に指定されれば true
    let req = Request::with_version(
        Method::from_static(b"DESCRIBE"),
        "rtsp://example.com/m",
        "RTSP/1.1",
    )
    .unwrap()
    .header(HeaderName::from_static(b"Connection"), "keep-alive")
    .unwrap();
    assert!(
        req.is_keep_alive(),
        "Connection: keep-alive が明示指定されれば true"
    );
}

// ========================================
// is_keep_alive / is_chunked の OWS 厳格化
// ========================================
// RFC 9110 Section 5.6.3 の OWS は `*( SP / HTAB )` のみ。
// 旧実装は `str::trim()` を使っており NBSP (U+00A0) 等の Unicode 空白も除去していたため、
// 前段プロキシ (ASCII OWS のみ trim) との解釈不一致で HTTP Request Smuggling
// (CWE-444) の足場となっていた。

/// Connection ヘッダー先頭の NBSP (U+00A0) を OWS として扱わないこと
#[test]
fn test_request_is_keep_alive_nbsp_not_trimmed() {
    let req = Request::with_version(Method::GET, "/", "HTTP/1.0")
        .unwrap()
        .header(HeaderName::from_static(b"Connection"), "\u{00A0}keep-alive")
        .unwrap();
    assert!(
        !req.is_keep_alive(),
        "NBSP 前置のトークンは keep-alive と一致してはならない"
    );
}

/// Connection ヘッダーの HTAB / SP は OWS として正しく除去されること (リグレッション防止)
#[test]
fn test_request_is_keep_alive_htab_sp_trimmed() {
    let req = Request::with_version(Method::GET, "/", "HTTP/1.0")
        .unwrap()
        .header(HeaderName::from_static(b"Connection"), "\tkeep-alive")
        .unwrap();
    assert!(req.is_keep_alive(), "HTAB 前置は OWS として除去される");

    let req = Request::with_version(Method::GET, "/", "HTTP/1.0")
        .unwrap()
        .header(HeaderName::from_static(b"Connection"), " keep-alive")
        .unwrap();
    assert!(req.is_keep_alive(), "SP 前置は OWS として除去される");

    let req = Request::with_version(Method::GET, "/", "HTTP/1.0")
        .unwrap()
        .header(HeaderName::from_static(b"Connection"), "  keep-alive ")
        .unwrap();
    assert!(req.is_keep_alive(), "前後の SP は OWS として除去される");
}

/// Transfer-Encoding ヘッダー先頭の NBSP は OWS として扱わないこと
#[test]
fn test_request_is_chunked_nbsp_not_trimmed() {
    let req = Request::new(Method::POST, "/")
        .unwrap()
        .header(
            HeaderName::from_static(b"Transfer-Encoding"),
            "\u{00A0}chunked",
        )
        .unwrap();
    assert!(
        !req.is_chunked(),
        "NBSP 前置のトークンは chunked と一致してはならない"
    );
}

/// Transfer-Encoding ヘッダーの HTAB / SP は OWS として正しく除去されること
#[test]
fn test_request_is_chunked_htab_sp_trimmed() {
    let req = Request::new(Method::POST, "/")
        .unwrap()
        .header(HeaderName::from_static(b"Transfer-Encoding"), "\tchunked")
        .unwrap();
    assert!(req.is_chunked(), "HTAB 前置は OWS として除去される");

    let req = Request::new(Method::POST, "/")
        .unwrap()
        .header(HeaderName::from_static(b"Transfer-Encoding"), " chunked")
        .unwrap();
    assert!(req.is_chunked(), "SP 前置は OWS として除去される");

    let req = Request::new(Method::POST, "/")
        .unwrap()
        .header(HeaderName::from_static(b"Transfer-Encoding"), "  chunked ")
        .unwrap();
    assert!(req.is_chunked(), "前後の SP は OWS として除去される");
}
