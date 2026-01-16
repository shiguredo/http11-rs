//! Content-Location ヘッダーのプロパティテスト

use proptest::prelude::*;
use shiguredo_http11::content_location::{ContentLocation, ContentLocationError};

// ========================================
// Strategy 定義
// ========================================

// スキーム
fn scheme() -> impl Strategy<Value = &'static str> {
    prop_oneof![Just("http"), Just("https"), Just("ftp"), Just("file"),]
}

// ホスト名
fn hostname() -> impl Strategy<Value = String> {
    prop_oneof![
        Just("localhost".to_string()),
        Just("example.com".to_string()),
        Just("test.example.org".to_string()),
        "[a-z]{1,8}(\\.[a-z]{1,8}){0,2}".prop_map(|s| s),
    ]
}

// IPv4 アドレス
fn ipv4() -> impl Strategy<Value = String> {
    (0u8..=255, 0u8..=255, 0u8..=255, 0u8..=255)
        .prop_map(|(a, b, c, d)| format!("{}.{}.{}.{}", a, b, c, d))
}

// パスセグメント
fn path_segment() -> impl Strategy<Value = String> {
    "[a-zA-Z0-9._-]{1,16}".prop_map(|s| s)
}

// パス
fn path() -> impl Strategy<Value = String> {
    prop_oneof![
        Just("/".to_string()),
        path_segment().prop_map(|s| format!("/{}", s)),
        (path_segment(), path_segment()).prop_map(|(a, b)| format!("/{}/{}", a, b)),
        (path_segment(), path_segment(), path_segment())
            .prop_map(|(a, b, c)| format!("/{}/{}/{}", a, b, c)),
    ]
}

// クエリ文字列
fn query() -> impl Strategy<Value = String> {
    prop_oneof![
        Just("".to_string()),
        "[a-z]{1,8}=[a-z0-9]{1,8}".prop_map(|s| format!("?{}", s)),
        "[a-z]{1,8}=[a-z0-9]{1,8}&[a-z]{1,8}=[a-z0-9]{1,8}".prop_map(|s| format!("?{}", s)),
    ]
}

// フラグメント
fn fragment() -> impl Strategy<Value = String> {
    prop_oneof![
        Just("".to_string()),
        "[a-z]{1,8}".prop_map(|s| format!("#{}", s)),
    ]
}

// 絶対 URI
fn absolute_uri() -> impl Strategy<Value = String> {
    (scheme(), hostname(), path(), query(), fragment())
        .prop_map(|(s, h, p, q, f)| format!("{}://{}{}{}{}", s, h, p, q, f))
}

// 相対 URI (パスのみ)
fn relative_uri() -> impl Strategy<Value = String> {
    (path(), query(), fragment()).prop_map(|(p, q, f)| format!("{}{}{}", p, q, f))
}

// 有効な URI
fn valid_uri() -> impl Strategy<Value = String> {
    prop_oneof![absolute_uri(), relative_uri(),]
}

// ========================================
// ContentLocationError のテスト
// ========================================

#[test]
fn content_location_error_display() {
    let errors = [
        (ContentLocationError::Empty, "empty Content-Location"),
        (
            ContentLocationError::InvalidUri,
            "invalid Content-Location URI",
        ),
    ];

    for (error, expected) in errors {
        assert_eq!(error.to_string(), expected);
    }
}

#[test]
fn content_location_error_is_error_trait() {
    let error: Box<dyn std::error::Error> = Box::new(ContentLocationError::Empty);
    assert_eq!(error.to_string(), "empty Content-Location");
}

// ========================================
// 絶対 URI のテスト
// ========================================

// 絶対 URI のラウンドトリップ
proptest! {
    #[test]
    fn content_location_absolute_uri_roundtrip(uri in absolute_uri()) {
        let cl = ContentLocation::parse(&uri).unwrap();
        let _display = cl.to_string();

        // URI が正しくパースされる
        prop_assert!(cl.uri().as_str().starts_with("http") || cl.uri().as_str().starts_with("ftp") || cl.uri().as_str().starts_with("file"));
    }
}

// HTTP/HTTPS URI
proptest! {
    #[test]
    fn content_location_http_uri(
        secure in proptest::bool::ANY,
        host in hostname(),
        p in path()
    ) {
        let scheme = if secure { "https" } else { "http" };
        let uri = format!("{}://{}{}", scheme, host, p);
        let cl = ContentLocation::parse(&uri).unwrap();

        prop_assert!(cl.uri().as_str().contains(&host));
    }
}

// IPv4 ホスト
proptest! {
    #[test]
    fn content_location_ipv4_host(addr in ipv4(), p in path()) {
        let uri = format!("http://{}{}", addr, p);
        let cl = ContentLocation::parse(&uri).unwrap();

        prop_assert!(cl.uri().as_str().contains(&addr));
    }
}

// ========================================
// 相対 URI のテスト
// ========================================

// 相対 URI のラウンドトリップ
proptest! {
    #[test]
    fn content_location_relative_uri_roundtrip(uri in relative_uri()) {
        let cl = ContentLocation::parse(&uri).unwrap();

        // パスが正しく取得できる
        prop_assert!(cl.uri().path().starts_with('/'));
    }
}

// パスのみの URI
proptest! {
    #[test]
    fn content_location_path_only(p in path()) {
        let cl = ContentLocation::parse(&p).unwrap();
        prop_assert_eq!(cl.uri().path(), p.as_str());
    }
}

// パス + クエリ
proptest! {
    #[test]
    fn content_location_path_with_query(p in path(), q in "[a-z]{1,8}=[a-z0-9]{1,8}") {
        let uri = format!("{}?{}", p, q);
        let cl = ContentLocation::parse(&uri).unwrap();

        prop_assert_eq!(cl.uri().path(), p.as_str());
        prop_assert_eq!(cl.uri().query(), Some(q.as_str()));
    }
}

// パス + フラグメント
proptest! {
    #[test]
    fn content_location_path_with_fragment(p in path(), frag in "[a-z]{1,8}") {
        let uri = format!("{}#{}", p, frag);
        let cl = ContentLocation::parse(&uri).unwrap();

        prop_assert_eq!(cl.uri().path(), p.as_str());
        prop_assert_eq!(cl.uri().fragment(), Some(frag.as_str()));
    }
}

// ========================================
// Display のテスト
// ========================================

// Display は元の URI を返す
proptest! {
    #[test]
    fn content_location_display(uri in valid_uri()) {
        let cl = ContentLocation::parse(&uri).unwrap();
        let display = cl.to_string();

        // Display 結果を再パースできる
        let reparsed = ContentLocation::parse(&display);
        prop_assert!(reparsed.is_ok());
    }
}

// ========================================
// エラーケースのテスト
// ========================================

#[test]
fn content_location_parse_errors() {
    // 空
    assert!(matches!(
        ContentLocation::parse(""),
        Err(ContentLocationError::Empty)
    ));
    assert!(matches!(
        ContentLocation::parse("   "),
        Err(ContentLocationError::Empty)
    ));

    // 不正な URI (IPv6 閉じ括弧なし)
    assert!(matches!(
        ContentLocation::parse("http://[::1"),
        Err(ContentLocationError::InvalidUri)
    ));
}

// ========================================
// Clone と PartialEq のテスト
// ========================================

proptest! {
    #[test]
    fn content_location_clone_eq(uri in valid_uri()) {
        let cl = ContentLocation::parse(&uri).unwrap();
        let cloned = cl.clone();

        prop_assert_eq!(cl, cloned);
    }
}

// ========================================
// no_panic テスト
// ========================================

proptest! {
    #[test]
    fn content_location_parse_no_panic(s in "[ -~]{0,128}") {
        let _ = ContentLocation::parse(&s);
    }
}
