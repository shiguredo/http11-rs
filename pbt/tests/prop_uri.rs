//! URI とパーセントエンコードのプロパティテスト (uri.rs)

use proptest::prelude::*;
use shiguredo_http11::uri::{
    Uri, normalize, percent_decode, percent_decode_bytes, percent_encode, percent_encode_path,
    percent_encode_query, resolve,
};

// ========================================
// Strategy 定義
// ========================================

// スキーム (RFC 3986: ALPHA *( ALPHA / DIGIT / "+" / "-" / "." ))
fn scheme() -> impl Strategy<Value = String> {
    "[a-z][a-z0-9+.-]{0,7}".prop_map(|s| s)
}

// ホスト名
fn hostname() -> impl Strategy<Value = String> {
    prop_oneof![
        "[a-z0-9]{1,16}".prop_map(|s| s),
        "[a-z0-9]{1,8}\\.[a-z]{2,4}".prop_map(|s| s),
        "[a-z0-9]{1,8}\\.[a-z0-9]{1,8}\\.[a-z]{2,4}".prop_map(|s| s),
    ]
}

// IPv4 アドレス
fn ipv4() -> impl Strategy<Value = String> {
    (0u8..=255, 0u8..=255, 0u8..=255, 0u8..=255)
        .prop_map(|(a, b, c, d)| format!("{}.{}.{}.{}", a, b, c, d))
}

// IPv6 アドレス (簡略化)
fn ipv6() -> impl Strategy<Value = String> {
    prop_oneof![
        Just("[::1]".to_string()),
        Just("[::ffff:127.0.0.1]".to_string()),
        Just("[2001:db8::1]".to_string()),
        Just("[fe80::1]".to_string()),
    ]
}

// ポート番号
fn port() -> impl Strategy<Value = u16> {
    1u16..=65535
}

// パスセグメント (. と .. を除外)
fn path_segment() -> impl Strategy<Value = String> {
    "[a-zA-Z0-9._-]{1,16}".prop_filter("exclude . and ..", |s| s != "." && s != "..")
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
        "[a-z]{1,8}=[a-z0-9]{1,8}".prop_map(|s| s),
        "[a-z]{1,8}=[a-z0-9]{1,8}&[a-z]{1,8}=[a-z0-9]{1,8}".prop_map(|s| s),
    ]
}

// フラグメント
fn fragment() -> impl Strategy<Value = String> {
    "[a-zA-Z0-9_-]{1,16}".prop_map(|s| s)
}

// userinfo
fn userinfo() -> impl Strategy<Value = String> {
    prop_oneof![
        "[a-z]{1,8}".prop_map(|s| s),
        "[a-z]{1,8}:[a-z0-9]{1,8}".prop_map(|s| s),
    ]
}

// ========================================
// パーセントエンコード/デコードのテスト
// ========================================

// パーセントエンコード/デコードのラウンドトリップ
proptest! {
    #[test]
    fn prop_percent_encode_decode_roundtrip(s in "[ -~]{0,64}") {
        let encoded = percent_encode(&s);
        let decoded = percent_decode(&encoded).unwrap();
        prop_assert_eq!(decoded, s);
    }
}

// UTF-8 文字列のパーセントエンコード/デコードのラウンドトリップ
proptest! {
    #[test]
    fn prop_percent_encode_decode_utf8_roundtrip(s in "\\PC{0,32}") {
        let encoded = percent_encode(&s);
        let decoded = percent_decode(&encoded).unwrap();
        prop_assert_eq!(decoded, s);
    }
}

// パーセントエンコードされた文字列は安全な文字のみを含む
proptest! {
    #[test]
    fn prop_percent_encode_safe_chars(s in "\\PC{0,32}") {
        let encoded = percent_encode(&s);
        for c in encoded.chars() {
            prop_assert!(
                c.is_ascii_alphanumeric() || c == '-' || c == '.' || c == '_' || c == '~' || c == '%',
                "encoded contains unsafe char: {:?}",
                c
            );
        }
    }
}

// パス用エンコードは `/` を保持
proptest! {
    #[test]
    fn prop_percent_encode_path_preserves_slash(s in "[a-zA-Z0-9/]{1,32}") {
        let encoded = percent_encode_path(&s);
        prop_assert_eq!(s.matches('/').count(), encoded.matches('/').count());
    }
}

// パス用エンコードは特殊文字をエンコード
proptest! {
    #[test]
    fn prop_percent_encode_path_encodes_special(s in "[a-zA-Z0-9 ?#]{1,32}") {
        let encoded = percent_encode_path(&s);
        // スペース、?, # はエンコードされる
        prop_assert!(!encoded.contains(' '));
        prop_assert!(!encoded.contains('?'));
        prop_assert!(!encoded.contains('#'));
    }
}

// クエリ用エンコードは `=` と `&` を保持
proptest! {
    #[test]
    fn prop_percent_encode_query_preserves_special(s in "[a-zA-Z0-9=&]{1,32}") {
        let encoded = percent_encode_query(&s);
        prop_assert_eq!(s.matches('=').count(), encoded.matches('=').count());
        prop_assert_eq!(s.matches('&').count(), encoded.matches('&').count());
    }
}

// クエリ用エンコードは他の特殊文字をエンコード
proptest! {
    #[test]
    fn prop_percent_encode_query_encodes_other_special(s in "[a-zA-Z0-9 #]{1,32}") {
        let encoded = percent_encode_query(&s);
        prop_assert!(!encoded.contains(' '));
        prop_assert!(!encoded.contains('#'));
    }
}

// percent_decode_bytes のラウンドトリップ
proptest! {
    #[test]
    fn prop_percent_decode_bytes_roundtrip(data in proptest::collection::vec(any::<u8>(), 0..64)) {
        // バイト列をエンコード
        let encoded: String = data
            .iter()
            .map(|&b| {
                if b.is_ascii_alphanumeric() || b == b'-' || b == b'.' || b == b'_' || b == b'~' {
                    (b as char).to_string()
                } else {
                    format!("%{:02X}", b)
                }
            })
            .collect();

        let decoded = percent_decode_bytes(&encoded).unwrap();
        prop_assert_eq!(decoded, data);
    }
}

// ========================================
// Uri::parse のテスト
// ========================================

// 有効な絶対 URI のパース
proptest! {
    #[test]
    fn prop_uri_parse_absolute(s in scheme(), h in hostname(), p in path()) {
        let uri_str = format!("{}://{}{}", s, h, p);
        let uri = Uri::parse(&uri_str).unwrap();

        prop_assert_eq!(uri.scheme(), Some(s.as_str()));
        prop_assert_eq!(uri.host(), Some(h.as_str()));
        prop_assert_eq!(uri.path(), p.as_str());
        prop_assert!(uri.is_absolute());
        prop_assert!(!uri.is_relative());
    }
}

// ポート付き URI のパース
proptest! {
    #[test]
    fn prop_uri_parse_with_port(s in scheme(), h in hostname(), pt in port(), p in path()) {
        let uri_str = format!("{}://{}:{}{}", s, h, pt, p);
        let uri = Uri::parse(&uri_str).unwrap();

        prop_assert_eq!(uri.scheme(), Some(s.as_str()));
        prop_assert_eq!(uri.host(), Some(h.as_str()));
        prop_assert_eq!(uri.port(), Some(pt));
        prop_assert_eq!(uri.path(), p.as_str());
    }
}

// IPv4 ホスト付き URI のパース
proptest! {
    #[test]
    fn prop_uri_parse_ipv4_host(addr in ipv4(), p in path()) {
        let uri_str = format!("http://{}{}", addr, p);
        let uri = Uri::parse(&uri_str).unwrap();

        prop_assert_eq!(uri.host(), Some(addr.as_str()));
        prop_assert_eq!(uri.path(), p.as_str());
    }
}

// IPv6 ホスト付き URI のパース
proptest! {
    #[test]
    fn prop_uri_parse_ipv6_host(addr in ipv6(), p in path()) {
        let uri_str = format!("http://{}{}", addr, p);
        let uri = Uri::parse(&uri_str).unwrap();

        prop_assert_eq!(uri.host(), Some(addr.as_str()));
        prop_assert_eq!(uri.path(), p.as_str());
    }
}

// IPv6 ホスト + ポート付き URI のパース
proptest! {
    #[test]
    fn prop_uri_parse_ipv6_with_port(addr in ipv6(), pt in port(), p in path()) {
        let uri_str = format!("http://{}:{}{}", addr, pt, p);
        let uri = Uri::parse(&uri_str).unwrap();

        prop_assert_eq!(uri.host(), Some(addr.as_str()));
        prop_assert_eq!(uri.port(), Some(pt));
    }
}

// userinfo 付き URI のパース
proptest! {
    #[test]
    fn prop_uri_parse_with_userinfo(user in userinfo(), h in hostname(), p in path()) {
        let uri_str = format!("http://{}@{}{}", user, h, p);
        let uri = Uri::parse(&uri_str).unwrap();

        // host() は userinfo を除いた値を返す
        prop_assert_eq!(uri.host(), Some(h.as_str()));

        // authority() は userinfo を含む
        let expected_auth = format!("{}@{}", user, h);
        prop_assert_eq!(uri.authority(), Some(expected_auth.as_str()));
    }
}

// userinfo + ポート付き URI のパース
proptest! {
    #[test]
    fn prop_uri_parse_with_userinfo_and_port(user in userinfo(), h in hostname(), pt in port()) {
        let uri_str = format!("http://{}@{}:{}/", user, h, pt);
        let uri = Uri::parse(&uri_str).unwrap();

        prop_assert_eq!(uri.host(), Some(h.as_str()));
        prop_assert_eq!(uri.port(), Some(pt));
    }
}

// 相対 URI のパース
proptest! {
    #[test]
    fn prop_uri_parse_relative(p in path()) {
        let uri = Uri::parse(&p).unwrap();

        prop_assert!(uri.is_relative());
        prop_assert!(!uri.is_absolute());
        prop_assert_eq!(uri.scheme(), None);
        prop_assert_eq!(uri.host(), None);
        prop_assert_eq!(uri.authority(), None);
        prop_assert_eq!(uri.path(), p.as_str());
    }
}

// クエリ付き URI のパース
proptest! {
    #[test]
    fn prop_uri_parse_with_query(p in path(), q in query()) {
        let uri_str = format!("{}?{}", p, q);
        let uri = Uri::parse(&uri_str).unwrap();

        prop_assert_eq!(uri.path(), p.as_str());
        prop_assert_eq!(uri.query(), Some(q.as_str()));
    }
}

// フラグメント付き URI のパース
proptest! {
    #[test]
    fn prop_uri_parse_with_fragment(p in path(), f in fragment()) {
        let uri_str = format!("{}#{}", p, f);
        let uri = Uri::parse(&uri_str).unwrap();

        prop_assert_eq!(uri.path(), p.as_str());
        prop_assert_eq!(uri.fragment(), Some(f.as_str()));
    }
}

// クエリ + フラグメント付き URI のパース
proptest! {
    #[test]
    fn prop_uri_parse_with_query_and_fragment(p in path(), q in query(), f in fragment()) {
        let uri_str = format!("{}?{}#{}", p, q, f);
        let uri = Uri::parse(&uri_str).unwrap();

        prop_assert_eq!(uri.path(), p.as_str());
        prop_assert_eq!(uri.query(), Some(q.as_str()));
        prop_assert_eq!(uri.fragment(), Some(f.as_str()));
    }
}

// フル URI (scheme, userinfo, host, port, path, query, fragment)
proptest! {
    #[test]
    fn prop_uri_parse_full(
        s in scheme(),
        user in userinfo(),
        h in hostname(),
        pt in port(),
        p in path(),
        q in query(),
        f in fragment()
    ) {
        let uri_str = format!("{}://{}@{}:{}{}?{}#{}", s, user, h, pt, p, q, f);
        let uri = Uri::parse(&uri_str).unwrap();

        prop_assert_eq!(uri.scheme(), Some(s.as_str()));
        prop_assert_eq!(uri.host(), Some(h.as_str()));
        prop_assert_eq!(uri.port(), Some(pt));
        prop_assert_eq!(uri.path(), p.as_str());
        prop_assert_eq!(uri.query(), Some(q.as_str()));
        prop_assert_eq!(uri.fragment(), Some(f.as_str()));
    }
}

// ========================================
// Uri メソッドのテスト
// ========================================

// as_str() は元の URI 文字列を返す
proptest! {
    #[test]
    fn prop_uri_as_str(s in scheme(), h in hostname(), p in path()) {
        let uri_str = format!("{}://{}{}", s, h, p);
        let uri = Uri::parse(&uri_str).unwrap();

        prop_assert_eq!(uri.as_str(), uri_str.as_str());
    }
}

// origin_form は path + query
proptest! {
    #[test]
    fn prop_uri_origin_form(h in hostname(), p in path(), q in query()) {
        let uri_str = format!("http://{}{}?{}", h, p, q);
        let uri = Uri::parse(&uri_str).unwrap();

        let expected = format!("{}?{}", p, q);
        prop_assert_eq!(uri.origin_form(), expected);
    }
}

// 空パスの origin_form は "/"
proptest! {
    #[test]
    fn prop_uri_origin_form_empty_path(h in hostname()) {
        let uri_str = format!("http://{}", h);
        let uri = Uri::parse(&uri_str).unwrap();

        prop_assert_eq!(uri.origin_form(), "/");
    }
}

// origin_form (クエリなし)
proptest! {
    #[test]
    fn prop_uri_origin_form_no_query(h in hostname(), p in path()) {
        let uri_str = format!("http://{}{}", h, p);
        let uri = Uri::parse(&uri_str).unwrap();

        prop_assert_eq!(uri.origin_form(), p.as_str());
    }
}

// ========================================
// resolve のテスト
// ========================================

// 絶対参照の解決 (そのまま返る)
proptest! {
    #[test]
    fn prop_uri_resolve_absolute(s in scheme(), h in hostname(), p in path()) {
        let base = Uri::parse("http://example.com/a/b").unwrap();
        let reference = Uri::parse(&format!("{}://{}{}", s, h, p)).unwrap();
        let resolved = resolve(&base, &reference).unwrap();

        prop_assert_eq!(resolved.scheme(), Some(s.as_str()));
        prop_assert_eq!(resolved.host(), Some(h.as_str()));
    }
}

// authority 付き参照の解決 (base のスキームのみ使用)
proptest! {
    #[test]
    fn prop_uri_resolve_with_authority(h in hostname(), p in path()) {
        let base = Uri::parse("http://example.com/a/b").unwrap();
        let ref_str = format!("//{}{}", h, p);
        let reference = Uri::parse(&ref_str).unwrap();
        let resolved = resolve(&base, &reference).unwrap();

        prop_assert_eq!(resolved.scheme(), Some("http"));
        prop_assert_eq!(resolved.host(), Some(h.as_str()));
    }
}

// 絶対パス参照の解決
proptest! {
    #[test]
    fn prop_uri_resolve_absolute_path(segment in path_segment()) {
        // ドットセグメントを含まないシンプルなパスでテスト
        let p = format!("/{}", segment);
        let base = Uri::parse("http://example.com/a/b/c").unwrap();
        let reference = Uri::parse(&p).unwrap();
        let resolved = resolve(&base, &reference).unwrap();

        prop_assert_eq!(resolved.scheme(), Some("http"));
        prop_assert_eq!(resolved.host(), Some("example.com"));
        prop_assert_eq!(resolved.path(), p.as_str());
    }
}

// 相対パス参照の解決
proptest! {
    #[test]
    fn prop_uri_resolve_relative_path(segment in path_segment()) {
        let base = Uri::parse("http://example.com/a/b/c").unwrap();
        let reference = Uri::parse(&segment).unwrap();
        let resolved = resolve(&base, &reference).unwrap();

        prop_assert!(resolved.is_absolute());
        prop_assert_eq!(resolved.scheme(), Some("http"));
        prop_assert_eq!(resolved.host(), Some("example.com"));
        // パスは /a/b/{segment}
        let expected_path = format!("/a/b/{}", segment);
        prop_assert_eq!(resolved.path(), expected_path.as_str());
    }
}

// ========================================
// normalize のテスト
// ========================================

// 正規化後のスキームとホストは小文字
proptest! {
    #[test]
    fn prop_uri_normalize_lowercase(s in "[A-Z]{1,8}", h in "[A-Z]{1,16}") {
        let uri_str = format!("{}://{}/path", s, h);
        let uri = Uri::parse(&uri_str).unwrap();
        let normalized = normalize(&uri).unwrap();

        let expected_scheme = s.to_ascii_lowercase();
        let expected_host = h.to_ascii_lowercase();
        prop_assert_eq!(normalized.scheme(), Some(expected_scheme.as_str()));
        prop_assert_eq!(normalized.host(), Some(expected_host.as_str()));
    }
}

// クエリとフラグメントの正規化
proptest! {
    #[test]
    fn prop_uri_normalize_with_query_and_fragment(s in scheme(), h in hostname(), q in query(), f in fragment()) {
        let uri_str = format!("{}://{}/path?{}#{}", s.to_uppercase(), h.to_uppercase(), q, f);
        let uri = Uri::parse(&uri_str).unwrap();
        let normalized = normalize(&uri).unwrap();

        prop_assert_eq!(normalized.query(), Some(q.as_str()));
        prop_assert_eq!(normalized.fragment(), Some(f.as_str()));
    }
}

// ========================================
// パニック安全性テスト
// ========================================

// 任意のバイト列で URI パースがパニックしない
proptest! {
    #[test]
    fn prop_uri_parse_no_panic(data in proptest::collection::vec(any::<u8>(), 0..128)) {
        if let Ok(s) = std::str::from_utf8(&data) {
            let _ = Uri::parse(s);
        }
    }
}

// 任意の文字列でパーセントデコードがパニックしない
proptest! {
    #[test]
    fn prop_percent_decode_no_panic(s in "[ -~]{0,64}") {
        let _ = percent_decode(&s);
    }
}

// 任意の文字列で正規化がパニックしない
proptest! {
    #[test]
    fn prop_uri_normalize_no_panic(s in "[a-z]{1,8}://[a-z]{1,16}/[a-zA-Z0-9%._-]{0,32}") {
        if let Ok(uri) = Uri::parse(&s) {
            let _ = normalize(&uri);
        }
    }
}
