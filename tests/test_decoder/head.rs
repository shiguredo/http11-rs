//! Decoder の start-line / ヘッダーパース関連テスト
//!
//! - リクエスト行 / ステータス行のバリデーション (プロトコルバージョン / メソッド / target /
//!   status-code)
//! - Host ヘッダーの存在 / 重複 / 形式検証 (RFC 9112 Section 3.2)
//! - http / https URI の "://" 必須検証 (RFC 9110 Section 4.2)
//! - ヘッダー値の制御文字拒否
//! - Content-Length パース (Unicode 空白拒否、`HttpHead::content_length` 厳格パース)
//! - `RequestDecoder::default()` / `ResponseDecoder::default()` の挙動

use shiguredo_http11::{RequestDecoder, ResponseDecoder};

// ========================================
// ヘッダー値の制御文字エラーのテスト
// ========================================

/// ヘッダー値に制御文字 (NUL) を含むとエラー
#[test]
fn test_header_value_control_char_nul_error() {
    let data = b"GET / HTTP/1.1\r\nHost: localhost\r\nX-Bad: hello\x00world\r\n\r\n";
    let mut decoder = RequestDecoder::new();
    decoder.feed(data).unwrap();

    let result = decoder.decode_headers();
    assert!(result.is_err());
}

/// ヘッダー値に制御文字 (BEL) を含むとエラー
#[test]
fn test_header_value_control_char_bel_error() {
    let data = b"GET / HTTP/1.1\r\nHost: localhost\r\nX-Bad: hello\x07world\r\n\r\n";
    let mut decoder = RequestDecoder::new();
    decoder.feed(data).unwrap();

    let result = decoder.decode_headers();
    assert!(result.is_err());
}

// ========================================
// Host ヘッダー検証のテスト (RFC 9112 Section 3.2)
// ========================================

/// HTTP/1.1 リクエストで Host ヘッダーがないとエラー
#[test]
fn test_request_http11_missing_host_error() {
    let mut decoder = RequestDecoder::new();
    let request = "GET / HTTP/1.1\r\n\r\n";
    decoder.feed(request.as_bytes()).unwrap();

    let result = decoder.decode_headers();
    assert!(result.is_err());
}

/// HTTP/1.1 リクエストで Host ヘッダーが複数あるとエラー
#[test]
fn test_request_http11_multiple_host_error() {
    let mut decoder = RequestDecoder::new();
    let request = "GET / HTTP/1.1\r\nHost: a.com\r\nHost: b.com\r\n\r\n";
    decoder.feed(request.as_bytes()).unwrap();

    let result = decoder.decode_headers();
    assert!(result.is_err());
}

/// HTTP/1.1 リクエストで空の Host ヘッダーは許可
#[test]
fn test_request_http11_empty_host_ok() {
    let mut decoder = RequestDecoder::new();
    let request = "GET / HTTP/1.1\r\nHost: \r\n\r\n";
    decoder.feed(request.as_bytes()).unwrap();

    let result = decoder.decode_headers().unwrap();
    assert!(result.is_some());
}

/// HTTP/1.1 リクエストで不正な Host ヘッダー値はエラー
#[test]
fn test_request_http11_invalid_host_value_error() {
    let mut decoder = RequestDecoder::new();
    let request = "GET / HTTP/1.1\r\nHost: :invalid:host:\r\n\r\n";
    decoder.feed(request.as_bytes()).unwrap();

    let result = decoder.decode_headers();
    assert!(result.is_err());
}

// ========================================
// Default トレイト実装のテスト
// ========================================

/// RequestDecoder::default() は new() と同等
#[test]
fn test_request_decoder_default() {
    let mut decoder = RequestDecoder::default();
    let request = "GET / HTTP/1.1\r\nHost: localhost\r\n\r\n";
    decoder.feed(request.as_bytes()).unwrap();
    let result = decoder.decode_headers().unwrap();
    assert!(result.is_some());
}

/// ResponseDecoder::default() は new() と同等
#[test]
fn test_response_decoder_default() {
    let mut decoder = ResponseDecoder::default();
    let response = "HTTP/1.1 200 OK\r\n\r\n";
    decoder.feed(response.as_bytes()).unwrap();
    let result = decoder.decode_headers().unwrap();
    assert!(result.is_some());
}

// ========================================
// リクエスト行バリデーションのテスト
// ========================================

/// 不正なプロトコルバージョン (token "/" DIGIT+ "." DIGIT+ でない形式)
#[test]
fn test_request_invalid_protocol_version_error() {
    // "/" がない
    let mut decoder = RequestDecoder::new();
    let request = "GET / INVALID\r\nHost: localhost\r\n\r\n";
    decoder.feed(request.as_bytes()).unwrap();
    assert!(decoder.decode_headers().is_err());

    // "/" の後にドットがない
    let mut decoder = RequestDecoder::new();
    let request = "GET / HTTP/11\r\nHost: localhost\r\n\r\n";
    decoder.feed(request.as_bytes()).unwrap();
    assert!(decoder.decode_headers().is_err());

    // ドットの後に数字がない
    let mut decoder = RequestDecoder::new();
    let request = "GET / HTTP/1.\r\nHost: localhost\r\n\r\n";
    decoder.feed(request.as_bytes()).unwrap();
    assert!(decoder.decode_headers().is_err());

    // バージョン部分に 3 つのドット区切り
    let mut decoder = RequestDecoder::new();
    let request = "GET / HTTP/1.1.1\r\nHost: localhost\r\n\r\n";
    decoder.feed(request.as_bytes()).unwrap();
    assert!(decoder.decode_headers().is_err());
}

/// 不正なメソッド名 (スペースを含む)
#[test]
fn test_request_invalid_method_error() {
    // メソッドに不正な文字を含む (トークン文字でない)
    let data = b"G\x01T / HTTP/1.1\r\nHost: localhost\r\n\r\n";
    let mut decoder = RequestDecoder::new();
    decoder.feed(data).unwrap();

    let result = decoder.decode_headers();
    assert!(result.is_err());
}

/// 不正な request-target (制御文字を含む)
#[test]
fn test_request_invalid_request_target_error() {
    let data = b"GET /path\x01invalid HTTP/1.1\r\nHost: localhost\r\n\r\n";
    let mut decoder = RequestDecoder::new();
    decoder.feed(data).unwrap();

    let result = decoder.decode_headers();
    assert!(result.is_err());
}

// ========================================
// レスポンス行バリデーションのテスト
// ========================================

/// 不正なプロトコルバージョン (レスポンス)
#[test]
fn test_response_invalid_protocol_version_error() {
    // "/" がない
    let mut decoder = ResponseDecoder::new();
    let response = "INVALID 200 OK\r\n\r\n";
    decoder.feed(response.as_bytes()).unwrap();
    assert!(decoder.decode_headers().is_err());

    // "/" の後にドットがない
    let mut decoder = ResponseDecoder::new();
    let response = "HTTP/11 200 OK\r\n\r\n";
    decoder.feed(response.as_bytes()).unwrap();
    assert!(decoder.decode_headers().is_err());

    // ドットの後に数字がない
    let mut decoder = ResponseDecoder::new();
    let response = "HTTP/1. 200 OK\r\n\r\n";
    decoder.feed(response.as_bytes()).unwrap();
    assert!(decoder.decode_headers().is_err());
}

/// Content-Length に Unicode 空白 (NBSP / 全角空白) を含むレスポンスは拒否される
///
/// RFC 9110 Section 5.6.3: OWS = *( SP / HTAB )
/// `is_valid_field_value` は obs-text (0x80-0xFF) を許容するため NBSP の UTF-8 表現
/// `0xC2 0xA0` がヘッダー値に通り得るが、Content-Length のパースで OWS として扱うのは
/// SP / HTAB のみ。NBSP / 全角空白を含む値は DIGIT 検査で拒否されることを担保する
/// (HTTP Request Smuggling 経路の遮断)。
#[test]
fn test_response_content_length_with_nbsp_is_rejected() {
    let mut decoder = ResponseDecoder::new();
    let response = "HTTP/1.1 200 OK\r\nContent-Length: \u{A0}5\r\n\r\nhello";
    decoder.feed(response.as_bytes()).unwrap();
    assert!(
        decoder.decode_headers().is_err(),
        "NBSP を含む Content-Length は拒否される想定"
    );
}

#[test]
fn test_request_content_length_with_ideographic_space_is_rejected() {
    let mut decoder = RequestDecoder::new();
    let request = "POST / HTTP/1.1\r\nHost: example.com\r\nContent-Length: \u{3000}5\r\n\r\nhello";
    decoder.feed(request.as_bytes()).unwrap();
    assert!(
        decoder.decode_headers().is_err(),
        "全角空白を含む Content-Length は拒否される想定"
    );
}

/// 範囲外ステータスコード (600)
#[test]
fn test_response_status_code_out_of_range_error() {
    let mut decoder = ResponseDecoder::new();
    let response = "HTTP/1.1 600 Error\r\n\r\n";
    decoder.feed(response.as_bytes()).unwrap();

    let result = decoder.decode_headers();
    assert!(result.is_err());
}

/// 範囲外ステータスコード (99)
#[test]
fn test_response_status_code_too_low_error() {
    let mut decoder = ResponseDecoder::new();
    let response = "HTTP/1.1 099 Error\r\n\r\n";
    decoder.feed(response.as_bytes()).unwrap();

    let result = decoder.decode_headers();
    assert!(result.is_err());
}

// ========================================
// http/https 空 host 拒否テスト (RFC 9110 Section 4.2)
// ========================================

#[test]
fn test_request_http_empty_host_error() {
    // http:///path は空 host で不正
    let mut decoder = RequestDecoder::new();
    let request = "GET http:///path HTTP/1.1\r\nHost: \r\n\r\n";
    decoder.feed(request.as_bytes()).unwrap();

    let result = decoder.decode_headers();
    assert!(result.is_err());
}

#[test]
fn test_request_https_port_only_host_error() {
    // https://:443/path は空 host で不正
    let mut decoder = RequestDecoder::new();
    let request = "GET https://:443/path HTTP/1.1\r\nHost: \r\n\r\n";
    decoder.feed(request.as_bytes()).unwrap();

    let result = decoder.decode_headers();
    assert!(result.is_err());
}

// ========================================
// http/https URI の :// 必須検証 (RFC 9110 Section 4.2)
// ========================================

#[test]
fn test_request_http_without_double_slash_error() {
    // http:foo は "://" がないので不正
    let mut decoder = RequestDecoder::new();
    let request = "GET http:foo HTTP/1.1\r\nHost: \r\n\r\n";
    decoder.feed(request.as_bytes()).unwrap();

    let result = decoder.decode_headers();
    assert!(result.is_err());
}

#[test]
fn test_request_https_without_double_slash_error() {
    // https:path は "://" がないので不正
    let mut decoder = RequestDecoder::new();
    let request = "GET https:path HTTP/1.1\r\nHost: \r\n\r\n";
    decoder.feed(request.as_bytes()).unwrap();

    let result = decoder.decode_headers();
    assert!(result.is_err());
}

#[test]
fn test_request_http_with_double_slash_ok() {
    // http://example.com/path は正常
    let mut decoder = RequestDecoder::new();
    let request = "GET http://example.com/path HTTP/1.1\r\nHost: example.com\r\n\r\n";
    decoder.feed(request.as_bytes()).unwrap();

    let result = decoder.decode_headers();
    assert!(result.is_ok());
}

// ========================================
// HttpHead::content_length の差分検証 (issue 0044)
//
// decoder/body の parse_content_length と完全に整合した厳格パースを行うことを検証する。
// 旧実装の `.parse::<u64>().ok()` で漏れていた smuggling 経路を遮断する。
// ========================================

mod http_head_content_length {
    use shiguredo_http11::{Error, Request, Response};

    fn make_request_with_cl(values: &[&str]) -> Request {
        let mut req = Request::new("POST", "/").unwrap();
        req = req.header("Host", "example.com").unwrap();
        for v in values {
            req = req.add_header_clone("Content-Length", v);
        }
        req
    }

    fn make_response_with_cl(values: &[&str]) -> Response {
        let mut res = Response::new(200, "OK").unwrap();
        for v in values {
            res = res.add_header_clone("Content-Length", v);
        }
        res
    }

    trait AddHeaderClone: Sized {
        fn add_header_clone(self, name: &str, value: &str) -> Self;
    }
    impl AddHeaderClone for Request {
        fn add_header_clone(mut self, name: &str, value: &str) -> Self {
            self.add_header(name, value).unwrap();
            self
        }
    }
    impl AddHeaderClone for Response {
        fn add_header_clone(mut self, name: &str, value: &str) -> Self {
            self.add_header(name, value).unwrap();
            self
        }
    }

    #[test]
    fn test_request_content_length_single_value() {
        let req = make_request_with_cl(&["100"]);
        assert_eq!(req.content_length().unwrap(), Some(100));
    }

    #[test]
    fn test_request_content_length_plus_sign_rejected() {
        let req = make_request_with_cl(&["+100"]);
        assert!(matches!(req.content_length(), Err(Error::InvalidData(_))));
    }

    #[test]
    fn test_request_content_length_leading_zero_accepted() {
        let req = make_request_with_cl(&["0100"]);
        assert_eq!(req.content_length().unwrap(), Some(100));
    }

    #[test]
    fn test_request_content_length_ows_trimmed() {
        // ASCII OWS は trim 対象 (旧実装は None を返していた)
        let req = make_request_with_cl(&[" 100 "]);
        assert_eq!(req.content_length().unwrap(), Some(100));
    }

    #[test]
    fn test_request_content_length_comma_same_value_merged() {
        let req = make_request_with_cl(&["100, 100"]);
        assert_eq!(req.content_length().unwrap(), Some(100));
    }

    #[test]
    fn test_request_content_length_comma_mismatched_rejected() {
        // 旧実装は None、新実装は smuggling 検知で Err
        let req = make_request_with_cl(&["100, 101"]);
        assert!(matches!(req.content_length(), Err(Error::InvalidData(_))));
    }

    #[test]
    fn test_request_content_length_multi_line_same_value() {
        let req = make_request_with_cl(&["100", "100"]);
        assert_eq!(req.content_length().unwrap(), Some(100));
    }

    #[test]
    fn test_request_content_length_multi_line_mismatched_rejected() {
        // 旧実装は最初の値 (Some(100)) を黙って返していた smuggling 経路
        let req = make_request_with_cl(&["100", "101"]);
        assert!(matches!(req.content_length(), Err(Error::InvalidData(_))));
    }

    #[test]
    fn test_request_content_length_absent() {
        let req = Request::new("GET", "/")
            .unwrap()
            .header("Host", "example.com")
            .unwrap();
        assert_eq!(req.content_length().unwrap(), None);
    }

    #[test]
    fn test_response_content_length_mismatched_rejected() {
        let res = make_response_with_cl(&["100", "101"]);
        assert!(matches!(res.content_length(), Err(Error::InvalidData(_))));
    }

    #[test]
    fn test_response_content_length_ows_trimmed() {
        let res = make_response_with_cl(&[" 100 "]);
        assert_eq!(res.content_length().unwrap(), Some(100));
    }
}
