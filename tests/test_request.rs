//! Request の構築時バリデーションテスト
//!
//! `Request::new` / `Request::with_version` / `Request::header` / `Request::add_header`
//! / `Request::set_header` の各バリデーション分岐を検証する。
//! HTTP Request Smuggling (CWE-444) の典型的なペイロードを構築時に拒否することも確認する。

use shiguredo_http11::{EncodeError, Request};

// ========================================
// method バリデーション
// ========================================

#[test]
fn test_request_new_rejects_empty_method() {
    let result = Request::new("", "/");
    assert!(matches!(result, Err(EncodeError::InvalidMethod { .. })));
}

#[test]
fn test_request_new_rejects_method_with_crlf() {
    for method in &["GET\r\nX: y", "GET\r", "GET\n", "POST\r\nEvil: header"] {
        let result = Request::new(method, "/");
        assert!(
            matches!(result, Err(EncodeError::InvalidMethod { .. })),
            "method {:?} should be rejected",
            method
        );
    }
}

#[test]
fn test_request_new_rejects_method_with_space() {
    let result = Request::new("GE T", "/");
    assert!(matches!(result, Err(EncodeError::InvalidMethod { .. })));
}

#[test]
fn test_request_new_rejects_method_with_invalid_token_chars() {
    // RFC 9110 Section 5.6.2 token に違反する文字
    for method in &["GET/", "GET@", "GET[", "GET{", "GET\""] {
        let result = Request::new(method, "/");
        assert!(
            matches!(result, Err(EncodeError::InvalidMethod { .. })),
            "method {:?} should be rejected",
            method
        );
    }
}

#[test]
fn test_request_new_accepts_standard_methods() {
    for method in &[
        "GET", "POST", "PUT", "DELETE", "HEAD", "OPTIONS", "PATCH", "CONNECT", "TRACE",
    ] {
        let result = Request::new(method, "/");
        assert!(result.is_ok(), "method {:?} should be accepted", method);
    }
}

#[test]
fn test_request_new_accepts_custom_method_token() {
    // RTSP の GET_PARAMETER も tchar
    let result = Request::new("GET_PARAMETER", "rtsp://example.com/media");
    assert!(result.is_ok());
}

// ========================================
// uri (request-target) バリデーション
// ========================================

#[test]
fn test_request_new_rejects_empty_uri() {
    let result = Request::new("GET", "");
    assert!(matches!(
        result,
        Err(EncodeError::InvalidRequestTarget { .. })
    ));
}

#[test]
fn test_request_new_rejects_uri_with_crlf() {
    for uri in &[
        "/path\r\nEvil: header",
        "/\r\n",
        "/test\nEvil",
        "/test\rEvil",
    ] {
        let result = Request::new("GET", uri);
        assert!(
            matches!(result, Err(EncodeError::InvalidRequestTarget { .. })),
            "uri {:?} should be rejected",
            uri
        );
    }
}

#[test]
fn test_request_new_rejects_uri_with_nul() {
    let result = Request::new("GET", "/path\0bad");
    assert!(matches!(
        result,
        Err(EncodeError::InvalidRequestTarget { .. })
    ));
}

#[test]
fn test_request_new_rejects_uri_with_space() {
    let result = Request::new("GET", "/path with space");
    assert!(matches!(
        result,
        Err(EncodeError::InvalidRequestTarget { .. })
    ));
}

#[test]
fn test_request_new_rejects_uri_with_control_chars() {
    let result = Request::new("GET", "/path\x01bad");
    assert!(matches!(
        result,
        Err(EncodeError::InvalidRequestTarget { .. })
    ));
}

#[test]
fn test_request_new_rejects_uri_with_rfc3986_excluded_chars() {
    // RFC 3986 で除外されている文字
    for uri in &[
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
        let result = Request::new("GET", uri);
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
    let result = Request::new("GET", uri);
    assert!(matches!(
        result,
        Err(EncodeError::InvalidRequestTarget { .. })
    ));
}

#[test]
fn test_request_new_rejects_uri_with_pct_nul() {
    // %00 (パーセントエンコーディングされた NUL) は smuggling ペイロード
    let result = Request::new("GET", "/path%00bad");
    assert!(matches!(
        result,
        Err(EncodeError::InvalidRequestTarget { .. })
    ));
}

#[test]
fn test_request_new_rejects_uri_with_invalid_percent_encoding() {
    for uri in &["/path%xx", "/path%0", "/path%"] {
        let result = Request::new("GET", uri);
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
    let result = Request::with_version("GET", "/", "");
    assert!(matches!(result, Err(EncodeError::InvalidVersion { .. })));
}

#[test]
fn test_request_with_version_rejects_invalid_format() {
    for version in &[
        "HTTP",
        "/1.1",
        "HTTP/",
        "HTTP/1",
        "HTTP/1.",
        "HTTP/.1",
        "HTTP/abc.def",
    ] {
        let result = Request::with_version("GET", "/", version);
        assert!(
            matches!(result, Err(EncodeError::InvalidVersion { .. })),
            "version {:?} should be rejected",
            version
        );
    }
}

#[test]
fn test_request_with_version_rejects_version_with_space() {
    let result = Request::with_version("GET", "/", "HTTP /1.1");
    assert!(matches!(result, Err(EncodeError::InvalidVersion { .. })));
}

#[test]
fn test_request_with_version_accepts_http_versions() {
    for version in &["HTTP/1.1", "HTTP/1.0", "HTTP/0.9", "HTTP/2.0"] {
        let result = Request::with_version("GET", "/", version);
        assert!(result.is_ok(), "version {:?} should be accepted", version);
    }
}

#[test]
fn test_request_with_version_accepts_rtsp_versions() {
    for version in &["RTSP/1.0", "RTSP/2.0"] {
        let result = Request::with_version("GET", "rtsp://example.com/m", version);
        assert!(result.is_ok(), "version {:?} should be accepted", version);
    }
}

// ========================================
// 構築時のバリデーション順序
// ========================================

#[test]
fn test_request_new_validation_order_method_before_uri() {
    // method 失敗 + uri 失敗 → method のエラーが先に返る
    let result = Request::new("BAD\r\n", "/bad\r\n");
    assert!(matches!(result, Err(EncodeError::InvalidMethod { .. })));
}

#[test]
fn test_request_with_version_validation_order_method_before_uri_before_version() {
    // method 失敗 + uri 失敗 + version 失敗 → method のエラーが先に返る
    let result = Request::with_version("BAD\r\n", "/bad\r\n", "BAD\r\n");
    assert!(matches!(result, Err(EncodeError::InvalidMethod { .. })));
}

#[test]
fn test_request_with_version_validation_order_uri_before_version() {
    // method 成功 + uri 失敗 + version 失敗 → uri のエラーが先に返る
    let result = Request::with_version("GET", "/bad\r\n", "BAD\r\n");
    assert!(matches!(
        result,
        Err(EncodeError::InvalidRequestTarget { .. })
    ));
}

// ========================================
// ヘッダー名/値のバリデーション
// ========================================

#[test]
fn test_request_header_rejects_invalid_name() {
    let req = Request::new("GET", "/").unwrap();
    // スペースを含むヘッダー名は不正
    let result = req.header("Bad Name", "value");
    assert!(matches!(result, Err(EncodeError::InvalidHeaderName { .. })));
}

#[test]
fn test_request_header_rejects_empty_name() {
    let req = Request::new("GET", "/").unwrap();
    let result = req.header("", "value");
    assert!(matches!(result, Err(EncodeError::InvalidHeaderName { .. })));
}

#[test]
fn test_request_header_rejects_crlf_in_value() {
    let req = Request::new("GET", "/").unwrap();
    for value in &["evil\r\nEvil: injected", "evil\rinjected", "evil\ninjected"] {
        let r = req.clone().header("X-Test", value);
        assert!(
            matches!(r, Err(EncodeError::InvalidHeaderValue { .. })),
            "value {:?} should be rejected",
            value
        );
    }
}

#[test]
fn test_request_header_rejects_nul_in_value() {
    let req = Request::new("GET", "/").unwrap();
    let result = req.header("X-Test", "evil\0value");
    assert!(matches!(
        result,
        Err(EncodeError::InvalidHeaderValue { .. })
    ));
}

#[test]
fn test_request_header_accepts_empty_value() {
    // RFC 9110 Section 5.5: field-value = *field-content (空値は合法)
    let req = Request::new("GET", "/").unwrap();
    let result = req.header("X-Empty", "");
    assert!(result.is_ok());
}

#[test]
fn test_request_header_accepts_value_with_leading_trailing_whitespace() {
    // RFC 9110 §5.5 では「A field parsing implementation MUST exclude such whitespace」
    // と先頭/末尾空白の除外を要求しているが、本 issue では smuggling 防御に注力するため
    // trim を行わない。後続 issue で trim を導入する予定の暫定動作。
    let req = Request::new("GET", "/").unwrap();
    let result = req.header("X-Test", " value with leading space ");
    assert!(result.is_ok());
}

// ========================================
// add_header の挙動確認
// ========================================

#[test]
fn test_request_add_header_appends_duplicate_names() {
    let mut req = Request::new("GET", "/").unwrap();
    req.add_header("Set-Cookie", "a=1").unwrap();
    req.add_header("Set-Cookie", "b=2").unwrap();
    let cookies = req.get_headers("Set-Cookie");
    assert_eq!(cookies, vec!["a=1", "b=2"]);
}

#[test]
fn test_request_add_header_does_not_modify_self_on_invalid_name() {
    let mut req = Request::new("GET", "/").unwrap();
    req.add_header("Host", "example.com").unwrap();
    let result = req.add_header("Bad Name", "v");
    assert!(matches!(result, Err(EncodeError::InvalidHeaderName { .. })));
    // Host は残っている
    assert_eq!(req.get_header("Host"), Some("example.com"));
    // 不正な名前のヘッダーは追加されていない
    assert!(req.get_header("Bad Name").is_none());
}

#[test]
fn test_request_add_header_does_not_modify_self_on_invalid_value() {
    let mut req = Request::new("GET", "/").unwrap();
    req.add_header("Host", "example.com").unwrap();
    let result = req.add_header("X-Test", "evil\r\ninjected");
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
    let mut req = Request::new("GET", "/").unwrap();
    req.add_header("Host", "old.example.com").unwrap();
    req.set_header("Host", "new.example.com").unwrap();
    assert_eq!(req.get_header("Host"), Some("new.example.com"));
    assert_eq!(req.get_headers("Host").len(), 1);
}

#[test]
fn test_request_set_header_overwrites_case_insensitively() {
    let mut req = Request::new("GET", "/").unwrap();
    req.add_header("Host", "old.example.com").unwrap();
    // 大文字小文字違いでも同名と判定される
    req.set_header("HOST", "new.example.com").unwrap();
    let hosts = req.get_headers("Host");
    assert_eq!(hosts, vec!["new.example.com"]);
}

#[test]
fn test_request_set_header_removes_all_duplicates() {
    let mut req = Request::new("GET", "/").unwrap();
    req.add_header("X-Test", "v1").unwrap();
    req.add_header("X-Test", "v2").unwrap();
    req.add_header("X-Test", "v3").unwrap();
    req.set_header("X-Test", "new").unwrap();
    let values = req.get_headers("X-Test");
    assert_eq!(values, vec!["new"]);
}

#[test]
fn test_request_set_header_atomicity_on_invalid_name() {
    let mut req = Request::new("GET", "/").unwrap();
    req.add_header("Host", "example.com").unwrap();
    req.add_header("X-Other", "value").unwrap();
    // バリデーション失敗時に既存ヘッダーが消えないこと
    let result = req.set_header("Bad Name", "value");
    assert!(matches!(result, Err(EncodeError::InvalidHeaderName { .. })));
    assert_eq!(req.get_header("Host"), Some("example.com"));
    assert_eq!(req.get_header("X-Other"), Some("value"));
}

#[test]
fn test_request_set_header_atomicity_on_invalid_value() {
    let mut req = Request::new("GET", "/").unwrap();
    req.add_header("Host", "example.com").unwrap();
    // 既存の Host を上書きしようとして失敗 → Host が消えないこと
    let result = req.set_header("Host", "evil\r\ninjected");
    assert!(matches!(
        result,
        Err(EncodeError::InvalidHeaderValue { .. })
    ));
    assert_eq!(req.get_header("Host"), Some("example.com"));
}

#[test]
fn test_request_set_header_inserts_when_not_present() {
    let mut req = Request::new("GET", "/").unwrap();
    req.set_header("Host", "example.com").unwrap();
    assert_eq!(req.get_header("Host"), Some("example.com"));
}

// ========================================
// HTTP Request Smuggling (CWE-444) ペイロード拒否
// ========================================

#[test]
fn test_request_rejects_smuggling_te_cl_via_crlf_in_value() {
    // CRLF 注入による TE/CL 競合の偽装を構築時に拒否する
    let req = Request::new("POST", "/").unwrap();
    let result = req.header("Transfer-Encoding", "chunked\r\nContent-Length: 0");
    assert!(matches!(
        result,
        Err(EncodeError::InvalidHeaderValue { .. })
    ));
}

#[test]
fn test_request_rejects_smuggling_method_crlf_injection() {
    // method への CRLF 注入を構築時に拒否する
    let result = Request::new("GET\r\nX: y", "/");
    assert!(matches!(result, Err(EncodeError::InvalidMethod { .. })));
}

#[test]
fn test_request_rejects_smuggling_uri_sp_injection() {
    // URI への SP 注入 (smuggling の典型ペイロード) を構築時に拒否する
    let result = Request::new("GET", "/api?q=x HTTP/1.1\r\nGET /admin");
    assert!(matches!(
        result,
        Err(EncodeError::InvalidRequestTarget { .. })
    ));
}

#[test]
fn test_request_rejects_smuggling_uri_pct_nul() {
    // URI への %00 NUL エンコーディング (smuggling ペイロード) を構築時に拒否する
    let result = Request::new("GET", "/path%00bad");
    assert!(matches!(
        result,
        Err(EncodeError::InvalidRequestTarget { .. })
    ));
}

#[test]
fn test_request_rejects_smuggling_header_name_with_crlf() {
    let req = Request::new("GET", "/").unwrap();
    let result = req.header("Bad\r\nEvil", "value");
    assert!(matches!(result, Err(EncodeError::InvalidHeaderName { .. })));
}

// ========================================
// アクセサ
// ========================================

#[test]
fn test_request_accessors() {
    let req = Request::new("GET", "/path")
        .unwrap()
        .header("Host", "example.com")
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
    let req = Request::new("GET", "/").unwrap();
    assert!(req.body_bytes().is_none());
}

#[test]
fn test_request_body_bytes_some_empty() {
    let req = Request::new("POST", "/").unwrap().body(Vec::new());
    assert_eq!(req.body_bytes(), Some(b"".as_slice()));
}
