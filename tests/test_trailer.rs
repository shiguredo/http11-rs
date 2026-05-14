//! Trailer ヘッダーのユニットテスト

use shiguredo_http11::trailer::{Trailer, TrailerError, is_prohibited_trailer_field};

#[test]
fn parse_fields() {
    let trailer = Trailer::parse("X-Checksum, X-Test").unwrap();
    assert_eq!(
        trailer.fields(),
        &["x-checksum".to_string(), "x-test".to_string()]
    );
}

#[test]
fn parse_invalid() {
    assert!(Trailer::parse("bad value").is_err());
}

/// RFC 9110 Section 5.6.1.2: 空フィールド値・空要素は受理する
#[test]
fn parse_empty_elements() {
    let trailer = Trailer::parse("").unwrap();
    assert!(trailer.fields().is_empty());

    let trailer = Trailer::parse(",").unwrap();
    assert!(trailer.fields().is_empty());

    let trailer = Trailer::parse("X-Checksum,,X-Test").unwrap();
    assert_eq!(trailer.fields().len(), 2);
}

#[test]
fn display() {
    let trailer = Trailer::parse("X-Checksum, X-Test").unwrap();
    assert_eq!(trailer.to_string(), "x-checksum, x-test");
}

#[test]
fn prohibited_field_transfer_encoding() {
    let result = Trailer::parse("Transfer-Encoding");
    assert!(matches!(
        result,
        Err(TrailerError::ProhibitedField(ref name)) if name == "transfer-encoding"
    ));
}

#[test]
fn prohibited_field_content_length() {
    let result = Trailer::parse("Content-Length");
    assert!(matches!(
        result,
        Err(TrailerError::ProhibitedField(ref name)) if name == "content-length"
    ));
}

#[test]
fn prohibited_field_host() {
    let result = Trailer::parse("Host");
    assert!(matches!(
        result,
        Err(TrailerError::ProhibitedField(ref name)) if name == "host"
    ));
}

#[test]
fn prohibited_field_trailer() {
    let result = Trailer::parse("Trailer");
    assert!(matches!(
        result,
        Err(TrailerError::ProhibitedField(ref name)) if name == "trailer"
    ));
}

#[test]
fn prohibited_field_content_encoding() {
    let result = Trailer::parse("Content-Encoding");
    assert!(matches!(
        result,
        Err(TrailerError::ProhibitedField(ref name)) if name == "content-encoding"
    ));
}

#[test]
fn prohibited_field_content_type() {
    let result = Trailer::parse("Content-Type");
    assert!(matches!(
        result,
        Err(TrailerError::ProhibitedField(ref name)) if name == "content-type"
    ));
}

#[test]
fn prohibited_field_content_range() {
    let result = Trailer::parse("Content-Range");
    assert!(matches!(
        result,
        Err(TrailerError::ProhibitedField(ref name)) if name == "content-range"
    ));
}

#[test]
fn prohibited_field_authorization() {
    // RFC 9110 Section 6.5.1 認証カテゴリ
    let result = Trailer::parse("Authorization");
    assert!(matches!(
        result,
        Err(TrailerError::ProhibitedField(ref name)) if name == "authorization"
    ));
}

#[test]
fn prohibited_field_proxy_authorization() {
    let result = Trailer::parse("Proxy-Authorization");
    assert!(matches!(
        result,
        Err(TrailerError::ProhibitedField(ref name)) if name == "proxy-authorization"
    ));
}

#[test]
fn prohibited_field_www_authenticate() {
    let result = Trailer::parse("WWW-Authenticate");
    assert!(matches!(
        result,
        Err(TrailerError::ProhibitedField(ref name)) if name == "www-authenticate"
    ));
}

#[test]
fn prohibited_field_request_modifier() {
    // RFC 9110 Section 6.5.1 リクエスト修飾子カテゴリ
    for name in [
        "If-Match",
        "If-None-Match",
        "If-Modified-Since",
        "If-Unmodified-Since",
        "If-Range",
        "Range",
        "Expect",
        "TE",
    ] {
        let result = Trailer::parse(name);
        assert!(
            matches!(result, Err(TrailerError::ProhibitedField(_))),
            "{} は禁止フィールドとして拒否されるべき",
            name
        );
    }
}

#[test]
fn prohibited_field_response_control() {
    // RFC 9110 Section 6.5.1 レスポンス制御カテゴリ
    for name in [
        "Cache-Control",
        "Vary",
        "Date",
        "Expires",
        "Age",
        "Set-Cookie",
    ] {
        let result = Trailer::parse(name);
        assert!(
            matches!(result, Err(TrailerError::ProhibitedField(_))),
            "{} は禁止フィールドとして拒否されるべき",
            name
        );
    }
}

#[test]
fn prohibited_field_connection_management() {
    // RFC 9110 Section 6.5.1 接続管理カテゴリ
    for name in ["Connection", "Upgrade"] {
        let result = Trailer::parse(name);
        assert!(
            matches!(result, Err(TrailerError::ProhibitedField(_))),
            "{} は禁止フィールドとして拒否されるべき",
            name
        );
    }
}

#[test]
fn prohibited_field_in_list() {
    // 複数フィールドの中に禁止フィールドがある場合
    let result = Trailer::parse("X-Custom, Content-Length, X-Other");
    assert!(matches!(
        result,
        Err(TrailerError::ProhibitedField(ref name)) if name == "content-length"
    ));
}

#[test]
fn allowed_fields() {
    // 許可されたフィールドは通る (拡張カテゴリのフィールドは含めない)
    let trailer = Trailer::parse("X-Checksum, X-Custom, X-Trace-Id").unwrap();
    assert_eq!(trailer.fields().len(), 3);
}

#[test]
fn is_prohibited_trailer_field_function() {
    assert!(is_prohibited_trailer_field("Transfer-Encoding"));
    assert!(is_prohibited_trailer_field("transfer-encoding"));
    assert!(is_prohibited_trailer_field("CONTENT-LENGTH"));
    // RFC 9110 Section 6.5.1 カテゴリ拡充の確認
    assert!(is_prohibited_trailer_field("Expires"));
    assert!(is_prohibited_trailer_field("Authorization"));
    assert!(is_prohibited_trailer_field("Cache-Control"));
    assert!(is_prohibited_trailer_field("Range"));
    assert!(is_prohibited_trailer_field("Connection"));
    // 拡張ヘッダーは引き続き許可
    assert!(!is_prohibited_trailer_field("X-Custom"));
    assert!(!is_prohibited_trailer_field("X-Checksum"));
}
