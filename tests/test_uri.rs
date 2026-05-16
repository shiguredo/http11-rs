//! URI のユニットテスト

use shiguredo_http11::uri::{Uri, UriError, normalize, percent_decode, resolve};

// ========================================
// UriError のテスト
// ========================================

#[test]
fn test_uri_error_display() {
    let errors = [
        (UriError::Empty, "empty URI"),
        (UriError::InvalidPercentEncoding, "invalid percent encoding"),
        (UriError::InvalidPort, "invalid port"),
        (UriError::InvalidCharacter('!'), "invalid character: '!'"),
        (UriError::InvalidScheme, "invalid scheme"),
        (UriError::InvalidHost, "invalid host"),
        (UriError::InvalidUtf8, "invalid UTF-8 sequence"),
        (UriError::InvalidUserinfo, "invalid userinfo"),
    ];

    for (error, expected) in errors {
        assert_eq!(error.to_string(), expected);
    }
}

// ========================================
// パーセントデコードのテスト
// ========================================

// percent_decode のエラーケース
#[test]
fn test_percent_decode_errors() {
    // 不完全なエンコーディング
    assert!(matches!(
        percent_decode("%"),
        Err(UriError::InvalidPercentEncoding)
    ));
    assert!(matches!(
        percent_decode("%2"),
        Err(UriError::InvalidPercentEncoding)
    ));

    // 不正な16進数
    assert!(matches!(
        percent_decode("%GG"),
        Err(UriError::InvalidPercentEncoding)
    ));
    assert!(matches!(
        percent_decode("%ZZ"),
        Err(UriError::InvalidPercentEncoding)
    ));
}

// percent_decode の InvalidUtf8 エラー
#[test]
fn test_percent_decode_invalid_utf8() {
    // 無効な UTF-8 シーケンス
    assert!(matches!(
        percent_decode("%FF%FE"),
        Err(UriError::InvalidUtf8)
    ));
}

// ========================================
// Uri::parse のテスト
// ========================================

// 空 URI のエラー
#[test]
fn test_uri_parse_empty() {
    assert!(matches!(Uri::parse(""), Err(UriError::Empty)));
}

// ========================================
// Uri::parse エラーケースのテスト
// ========================================

// 不正なスキーム (最初が数字)
#[test]
fn test_uri_parse_invalid_scheme_starts_with_digit() {
    assert!(matches!(
        Uri::parse("1http://example.com"),
        Err(UriError::InvalidScheme)
    ));
}

// 不正なスキーム (不正な文字) -> スキームとして認識されない
#[test]
fn test_uri_parse_invalid_scheme_invalid_char() {
    // ! はスキームに使えないため、スキームとして認識されず相対パスとして解釈される
    let uri = Uri::parse("ht!tp://example.com").unwrap();
    assert_eq!(uri.scheme(), None);
    assert_eq!(uri.path(), "ht!tp://example.com");
}

// 不正なポート番号
#[test]
fn test_uri_parse_invalid_port() {
    assert!(matches!(
        Uri::parse("http://example.com:abc/"),
        Err(UriError::InvalidPort)
    ));
    assert!(matches!(
        Uri::parse("http://example.com:99999/"),
        Err(UriError::InvalidPort)
    ));
}

// 不正な IPv6 ホスト (閉じ括弧なし)
#[test]
fn test_uri_parse_invalid_ipv6_no_closing_bracket() {
    assert!(matches!(
        Uri::parse("http://[::1/path"),
        Err(UriError::InvalidHost)
    ));
}

// 不正な IPv6 ホスト (括弧後に不正な文字)
#[test]
fn test_uri_parse_invalid_ipv6_invalid_after_bracket() {
    assert!(matches!(
        Uri::parse("http://[::1]abc/path"),
        Err(UriError::InvalidHost)
    ));
}

// ========================================
// resolve のテスト
// ========================================

// 空パス参照の解決 (base のパスを使用)
#[test]
fn test_uri_resolve_empty_path() {
    let base = Uri::parse("http://example.com/a/b/c").unwrap();

    // 空パス + クエリ
    let reference = Uri::parse("?newquery").unwrap();
    let resolved = resolve(&base, &reference).unwrap();
    assert_eq!(resolved.path(), "/a/b/c");
    assert_eq!(resolved.query(), Some("newquery"));

    // 空パス + フラグメント
    let reference = Uri::parse("#newfrag").unwrap();
    let resolved = resolve(&base, &reference).unwrap();
    assert_eq!(resolved.path(), "/a/b/c");
    assert_eq!(resolved.query(), None); // base のクエリを使用
    assert_eq!(resolved.fragment(), Some("newfrag"));
}

// 空パス参照 (base にクエリがある場合)
#[test]
fn test_uri_resolve_empty_path_with_base_query() {
    let base = Uri::parse("http://example.com/path?basequery").unwrap();

    // 空パス、クエリなし -> base のクエリを継承
    let reference = Uri::parse("#frag").unwrap();
    let resolved = resolve(&base, &reference).unwrap();
    assert_eq!(resolved.query(), Some("basequery"));
}

// `..` を含む相対パスの解決
#[test]
fn test_uri_resolve_dotdot() {
    let base = Uri::parse("http://example.com/a/b/c").unwrap();

    let reference = Uri::parse("../d").unwrap();
    let resolved = resolve(&base, &reference).unwrap();
    assert_eq!(resolved.path(), "/a/d");

    let reference = Uri::parse("../../d").unwrap();
    let resolved = resolve(&base, &reference).unwrap();
    assert_eq!(resolved.path(), "/d");

    let reference = Uri::parse("../../../d").unwrap();
    let resolved = resolve(&base, &reference).unwrap();
    assert_eq!(resolved.path(), "/d"); // ルートを超えない
}

// `.` を含む相対パスの解決
#[test]
fn test_uri_resolve_dot() {
    let base = Uri::parse("http://example.com/a/b/c").unwrap();

    let reference = Uri::parse("./d").unwrap();
    let resolved = resolve(&base, &reference).unwrap();
    assert_eq!(resolved.path(), "/a/b/d");

    let reference = Uri::parse("././d").unwrap();
    let resolved = resolve(&base, &reference).unwrap();
    assert_eq!(resolved.path(), "/a/b/d");
}

// base に authority があり、パスが空の場合
#[test]
fn test_uri_resolve_base_empty_path() {
    let base = Uri::parse("http://example.com").unwrap();

    let reference = Uri::parse("relative").unwrap();
    let resolved = resolve(&base, &reference).unwrap();
    assert_eq!(resolved.path(), "/relative");
}

// ========================================
// normalize のテスト
// ========================================

// 正規化でドットセグメントが除去される
#[test]
fn test_uri_normalize_removes_dot_segments() {
    let uri = Uri::parse("http://example.com/a/b/../c/./d").unwrap();
    let normalized = normalize(&uri).unwrap();
    assert_eq!(normalized.path(), "/a/c/d");
}

// 正規化でパーセントエンコーディングが正規化される
#[test]
fn test_uri_normalize_percent_encoding() {
    // unreserved 文字のエンコードはデコードされる
    let uri = Uri::parse("http://example.com/%61%62%63").unwrap(); // abc
    let normalized = normalize(&uri).unwrap();
    assert_eq!(normalized.path(), "/abc");

    // reserved 文字のエンコードは大文字で保持
    let uri = Uri::parse("http://example.com/%2f").unwrap(); // /
    let normalized = normalize(&uri).unwrap();
    assert_eq!(normalized.path(), "/%2F");
}

// authority なし + path-only URI で `..` + 空 segment が `//` 始まりになる
// ケースで normalize が冪等であること (RFC 3986 Section 3.3)
#[test]
fn test_uri_normalize_idempotent_with_dotdot_double_slash() {
    let uri = Uri::parse("/..//YYYYYYYY/#").unwrap();
    let n1 = normalize(&uri).unwrap();
    let n2 = normalize(&n1).unwrap();
    assert_eq!(n1.as_str(), n2.as_str(), "normalize は冪等であること");
    assert_eq!(
        n1.path(),
        "/.//YYYYYYYY/",
        "path が path-absolute 形であること"
    );
    assert!(n1.authority().is_none(), "authority が None で保たれること");
}

// relative-path reference で最初の segment が `:` を含むケース
// (RFC 3986 Section 4.2) で normalize が冪等であること
#[test]
fn test_uri_normalize_idempotent_with_colon_first_segment() {
    // base="S55", reference="%55:;:/." を resolve すると path="%55:;:/" になり、
    // normalize で %55 が "U" にデコードされ "U:;:/" となるため、
    // 再 parse 時に "U" が scheme として誤解釈されていた。
    let base = Uri::parse("S55").unwrap();
    let reference = Uri::parse("%55:;:/.").unwrap();
    let resolved = resolve(&base, &reference).unwrap();
    let n1 = normalize(&resolved).unwrap();
    let n2 = normalize(&n1).unwrap();
    assert_eq!(n1.as_str(), n2.as_str(), "normalize は冪等であること");
    assert!(n1.scheme().is_none(), "scheme が誤って注入されないこと");
    assert!(n1.authority().is_none(), "authority が None で保たれること");
}

// percent-encoded dot (`%2E`) が含まれていても normalize が冪等であること
// (RFC 3986 Section 6.2.2: percent-encoding 正規化 → dot-segment 除去の順)
#[test]
fn test_uri_normalize_idempotent_with_encoded_dot_segment() {
    let uri = Uri::parse("/a/%2E/b/%2E%2E/c").unwrap();
    let n1 = normalize(&uri).unwrap();
    let n2 = normalize(&n1).unwrap();
    assert_eq!(n1.as_str(), n2.as_str(), "normalize は冪等であること");
    assert_eq!(
        n1.path(),
        "/a/c",
        "%2E は . にデコードされた後 dot-segment として除去されること"
    );
}

// scheme 付き + authority なしでも同様に冪等であること
#[test]
fn test_uri_normalize_scheme_only_double_slash() {
    let uri = Uri::parse("file:/..//Y/").unwrap();
    let n1 = normalize(&uri).unwrap();
    let n2 = normalize(&n1).unwrap();
    assert_eq!(n1.scheme(), Some("file"), "scheme が file で保たれること");
    assert!(
        n1.authority().is_none(),
        "scheme 付きでも authority が None で保たれること"
    );
    assert_eq!(n1.path(), "/.//Y/", "path が path-absolute 形であること");
    assert_eq!(
        n1.as_str(),
        "file:/.//Y/",
        "再 parse 可能な canonical 文字列であること"
    );
    assert_eq!(n1.as_str(), n2.as_str(), "scheme 付きでも冪等であること");
}

// ========================================
// remove_dot_segments の追加テスト
// ========================================

#[test]
fn test_remove_dot_segments_edge_cases() {
    // RFC 3986 Section 5.2.4 の remove_dot_segments エッジケース
    let base = Uri::parse("http://example.com/base/").unwrap();

    // . のみ
    let reference = Uri::parse(".").unwrap();
    let resolved = resolve(&base, &reference).unwrap();
    assert_eq!(resolved.path(), "/base/");

    // .. のみ
    let reference = Uri::parse("..").unwrap();
    let resolved = resolve(&base, &reference).unwrap();
    assert_eq!(resolved.path(), "/");

    // /. で終わる
    let uri = Uri::parse("http://example.com/a/b/.").unwrap();
    let normalized = normalize(&uri).unwrap();
    assert_eq!(normalized.path(), "/a/b/");

    // /.. で終わる
    let uri = Uri::parse("http://example.com/a/b/..").unwrap();
    let normalized = normalize(&uri).unwrap();
    assert_eq!(normalized.path(), "/a/");
}

// ./ と ../ で始まるパス
#[test]
fn test_remove_dot_segments_leading_dots() {
    let base = Uri::parse("http://example.com/a/b/c").unwrap();

    // ./ で始まる
    let reference = Uri::parse("./x").unwrap();
    let resolved = resolve(&base, &reference).unwrap();
    assert_eq!(resolved.path(), "/a/b/x");

    // ../ で始まる
    let reference = Uri::parse("../x").unwrap();
    let resolved = resolve(&base, &reference).unwrap();
    assert_eq!(resolved.path(), "/a/x");
}

// ========================================
// スキーム検出のエッジケース
// ========================================

#[test]
fn test_uri_scheme_edge_cases() {
    // スキームに + - . を含む
    let uri = Uri::parse("custom+scheme://host/path").unwrap();
    assert_eq!(uri.scheme(), Some("custom+scheme"));

    let uri = Uri::parse("my-scheme://host/path").unwrap();
    assert_eq!(uri.scheme(), Some("my-scheme"));

    let uri = Uri::parse("my.scheme://host/path").unwrap();
    assert_eq!(uri.scheme(), Some("my.scheme"));

    // コロンで始まる (スキームなし)
    let uri = Uri::parse(":path").unwrap();
    assert_eq!(uri.scheme(), None);
    assert_eq!(uri.path(), ":path");
}

// ========================================
// 空の authority
// ========================================

#[test]
fn test_uri_empty_authority() {
    let uri = Uri::parse("file:///path/to/file").unwrap();
    assert_eq!(uri.scheme(), Some("file"));
    assert_eq!(uri.authority(), Some(""));
    // 空の authority の場合、host() は空文字列の Some を返す
    assert_eq!(uri.host(), Some(""));
    assert_eq!(uri.path(), "/path/to/file");
}

// ========================================
// host 文字種検証のテスト (RFC 3986 Section 3.2.2)
// ========================================

// スペースを含む host は拒否
#[test]
fn test_uri_parse_invalid_host_space() {
    assert!(matches!(
        Uri::parse("http://exa mple.com/path"),
        Err(UriError::InvalidHost)
    ));
}

// 制御文字を含む host は拒否
#[test]
fn test_uri_parse_invalid_host_control_char() {
    assert!(matches!(
        Uri::parse("http://exam\x01ple.com"),
        Err(UriError::InvalidHost)
    ));
}

// reg-name に許可されない文字 (角括弧) は拒否
#[test]
fn test_uri_parse_invalid_host_bracket_in_reg_name() {
    // "[" は IP-literal の開始でのみ許可される
    // reg-name 中の "[" は不正
    assert!(matches!(
        Uri::parse("http://exam[ple.com/path"),
        Err(UriError::InvalidHost)
    ));
}

// パーセントエンコーディングを含む reg-name は受理
#[test]
fn test_uri_parse_valid_host_percent_encoded() {
    let uri = Uri::parse("http://exam%70le.com/path").unwrap();
    assert_eq!(uri.host(), Some("exam%70le.com"));
}

// ポート付きの不正 host は拒否
#[test]
fn test_uri_parse_invalid_host_with_port() {
    assert!(matches!(
        Uri::parse("http://exa mple.com:8080/path"),
        Err(UriError::InvalidHost)
    ));
}

// ========================================
// IP-literal 検証のテスト (RFC 3986 Section 3.2.2)
// ========================================

// 不正な IP-literal は拒否
#[test]
fn test_uri_parse_invalid_ip_literal() {
    // IPv6address でも IPvFuture でもない文字列
    assert!(matches!(
        Uri::parse("http://[not-an-ip]/"),
        Err(UriError::InvalidHost)
    ));
}

// 空の IP-literal は拒否
#[test]
fn test_uri_parse_empty_ip_literal() {
    assert!(matches!(
        Uri::parse("http://[]/"),
        Err(UriError::InvalidHost)
    ));
}

// 有効な IPv6 アドレスは受理
#[test]
fn test_uri_parse_valid_ipv6() {
    let uri = Uri::parse("http://[2001:db8::1]/path").unwrap();
    assert_eq!(uri.host(), Some("[2001:db8::1]"));

    let uri = Uri::parse("http://[::ffff:192.168.1.1]:8080/path").unwrap();
    assert_eq!(uri.host(), Some("[::ffff:192.168.1.1]"));
    assert_eq!(uri.port(), Some(8080));
}

// 有効な IPvFuture は受理
#[test]
fn test_uri_parse_valid_ipv_future() {
    let uri = Uri::parse("http://[v1.test]/path").unwrap();
    assert_eq!(uri.host(), Some("[v1.test]"));
}

// 不正な IPvFuture は拒否 (HEXDIG なし)
#[test]
fn test_uri_parse_invalid_ipv_future_no_hexdig() {
    assert!(matches!(
        Uri::parse("http://[v.test]/path"),
        Err(UriError::InvalidHost)
    ));
}

// 不正な IPvFuture は拒否 (ドット後が空)
#[test]
fn test_uri_parse_invalid_ipv_future_empty_after_dot() {
    assert!(matches!(
        Uri::parse("http://[vFF.]/path"),
        Err(UriError::InvalidHost)
    ));
}

// ========================================
// userinfo 検証のテスト (RFC 3986 Section 3.2.1)
// ========================================

// 空白を含む userinfo は拒否
#[test]
fn test_uri_parse_invalid_userinfo_space() {
    assert!(matches!(
        Uri::parse("http://user name@example.com/"),
        Err(UriError::InvalidUserinfo)
    ));
}

// 制御文字を含む userinfo は拒否
#[test]
fn test_uri_parse_invalid_userinfo_control_char() {
    assert!(matches!(
        Uri::parse("http://user\x01@example.com/"),
        Err(UriError::InvalidUserinfo)
    ));
}

// 有効な userinfo は受理
#[test]
fn test_uri_parse_valid_userinfo() {
    let uri = Uri::parse("http://user:pass@example.com/path").unwrap();
    assert_eq!(uri.host(), Some("example.com"));

    // パーセントエンコーディング含む userinfo
    let uri = Uri::parse("http://user%40name@example.com/").unwrap();
    assert_eq!(uri.host(), Some("example.com"));

    // sub-delims を含む userinfo
    let uri = Uri::parse("http://u$er!@example.com/").unwrap();
    assert_eq!(uri.host(), Some("example.com"));
}

// ========================================
// ポートが空の場合
// ========================================

#[test]
fn test_uri_empty_port() {
    let uri = Uri::parse("http://example.com:/path").unwrap();
    // 空ポートの場合、host には : が含まれる (実装の動作)
    assert_eq!(uri.host(), Some("example.com:"));
    assert_eq!(uri.port(), None);
    assert_eq!(uri.path(), "/path");
}
