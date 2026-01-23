//! Host ヘッダーのプロパティテスト

use proptest::prelude::*;
use shiguredo_http11::host::{Host, HostError};

// ========================================
// Strategy 定義
// ========================================

// 有効なホスト名文字
fn hostname_char() -> impl Strategy<Value = char> {
    prop_oneof![
        prop::char::range('a', 'z'),
        prop::char::range('A', 'Z'),
        prop::char::range('0', '9'),
        Just('-'),
    ]
}

fn hostname_label() -> impl Strategy<Value = String> {
    proptest::collection::vec(hostname_char(), 1..16).prop_map(|chars| chars.into_iter().collect())
}

fn hostname() -> impl Strategy<Value = String> {
    proptest::collection::vec(hostname_label(), 1..4).prop_map(|labels| labels.join("."))
}

// 有効なポート番号
fn valid_port() -> impl Strategy<Value = u16> {
    1u16..=65535
}

// IPv4 アドレス
fn ipv4_addr() -> impl Strategy<Value = String> {
    (0u8..=255, 0u8..=255, 0u8..=255, 0u8..=255)
        .prop_map(|(a, b, c, d)| format!("{}.{}.{}.{}", a, b, c, d))
}

// IPv6 アドレス (簡易版)
fn ipv6_addr() -> impl Strategy<Value = String> {
    prop_oneof![
        Just("::1".to_string()),
        Just("::".to_string()),
        Just("fe80::1".to_string()),
        Just("2001:db8::1".to_string()),
        Just("::ffff:192.168.1.1".to_string()),
        (0u16..=0xffff, 0u16..=0xffff, 0u16..=0xffff, 0u16..=0xffff)
            .prop_map(|(a, b, c, d)| format!("2001:db8::{:x}:{:x}:{:x}:{:x}", a, b, c, d)),
    ]
}

// ========================================
// HostError のテスト
// ========================================

#[test]
fn prop_host_error_display() {
    let errors = [
        (HostError::Empty, "empty Host header"),
        (HostError::InvalidFormat, "invalid Host header format"),
        (HostError::InvalidHost, "invalid Host header host"),
        (HostError::InvalidPort, "invalid Host header port"),
    ];

    for (error, expected) in errors {
        assert_eq!(error.to_string(), expected);
    }
}

#[test]
fn prop_host_error_is_error_trait() {
    let error: Box<dyn std::error::Error> = Box::new(HostError::Empty);
    assert_eq!(error.to_string(), "empty Host header");
}

// ========================================
// ホスト名パースのテスト
// ========================================

// ホスト名ラウンドトリップ
proptest! {
    #[test]
    fn prop_host_hostname_roundtrip(name in hostname()) {
        let host = Host::parse(&name).unwrap();
        prop_assert_eq!(host.host(), name.as_str());
        prop_assert_eq!(host.port(), None);
        prop_assert!(!host.is_ipv6());

        // Display
        let display = host.to_string();
        prop_assert_eq!(display, name);
    }
}

// ホスト名 + ポートラウンドトリップ
proptest! {
    #[test]
    fn prop_host_hostname_port_roundtrip(name in hostname(), port in valid_port()) {
        let input = format!("{}:{}", name, port);
        let host = Host::parse(&input).unwrap();

        prop_assert_eq!(host.host(), name.as_str());
        prop_assert_eq!(host.port(), Some(port));
        prop_assert!(!host.is_ipv6());

        // Display
        let display = host.to_string();
        prop_assert_eq!(display, input);
    }
}

// ========================================
// IPv4 パースのテスト
// ========================================

// IPv4 ラウンドトリップ
proptest! {
    #[test]
    fn prop_host_ipv4_roundtrip(addr in ipv4_addr()) {
        let host = Host::parse(&addr).unwrap();

        prop_assert_eq!(host.host(), addr.as_str());
        prop_assert_eq!(host.port(), None);
        prop_assert!(!host.is_ipv6());
    }
}

// IPv4 + ポートラウンドトリップ
proptest! {
    #[test]
    fn prop_host_ipv4_port_roundtrip(addr in ipv4_addr(), port in valid_port()) {
        let input = format!("{}:{}", addr, port);
        let host = Host::parse(&input).unwrap();

        prop_assert_eq!(host.host(), addr.as_str());
        prop_assert_eq!(host.port(), Some(port));
    }
}

// ========================================
// IPv6 パースのテスト
// ========================================

// IPv6 ラウンドトリップ
proptest! {
    #[test]
    fn prop_host_ipv6_roundtrip(addr in ipv6_addr()) {
        let input = format!("[{}]", addr);
        let host = Host::parse(&input).unwrap();

        prop_assert_eq!(host.host(), input.as_str());
        prop_assert_eq!(host.port(), None);
        prop_assert!(host.is_ipv6());
    }
}

// IPv6 + ポートラウンドトリップ
proptest! {
    #[test]
    fn prop_host_ipv6_port_roundtrip(addr in ipv6_addr(), port in valid_port()) {
        let input = format!("[{}]:{}", addr, port);
        let host = Host::parse(&input).unwrap();

        prop_assert_eq!(host.host(), format!("[{}]", addr));
        prop_assert_eq!(host.port(), Some(port));
        prop_assert!(host.is_ipv6());
    }
}

// ========================================
// パースエラーのテスト
// ========================================

#[test]
fn prop_host_parse_errors() {
    // 空
    assert!(matches!(Host::parse(""), Err(HostError::Empty)));
    assert!(matches!(Host::parse("   "), Err(HostError::Empty)));

    // 空白を含む
    assert!(matches!(
        Host::parse("example .com"),
        Err(HostError::InvalidFormat)
    ));
    assert!(matches!(
        Host::parse("example\tcom"),
        Err(HostError::InvalidFormat)
    ));

    // ポートが空
    assert!(matches!(
        Host::parse("example.com:"),
        Err(HostError::InvalidPort)
    ));

    // ポートが数字でない
    assert!(matches!(
        Host::parse("example.com:abc"),
        Err(HostError::InvalidPort)
    ));

    // ポートがオーバーフロー
    assert!(matches!(
        Host::parse("example.com:99999"),
        Err(HostError::InvalidPort)
    ));

    // 不正なホスト (@ を含む)
    assert!(matches!(
        Host::parse("user@example.com"),
        Err(HostError::InvalidHost)
    ));

    // IPv6 の閉じ括弧がない
    assert!(matches!(Host::parse("[::1"), Err(HostError::InvalidHost)));

    // 空の IPv6
    assert!(matches!(Host::parse("[]"), Err(HostError::InvalidHost)));

    // IPv6 の後に不正な文字
    assert!(matches!(
        Host::parse("[::1]abc"),
        Err(HostError::InvalidHost)
    ));

    // 複数のコロン (IPv6 でない)
    assert!(matches!(Host::parse("a:b:c"), Err(HostError::InvalidHost)));
}

// ========================================
// 特殊なホスト名のテスト
// ========================================

// パーセントエンコーディングを含むホスト名
proptest! {
    #[test]
    fn prop_host_percent_encoded(hex1 in "[0-9A-Fa-f]{2}", hex2 in "[0-9A-Fa-f]{2}") {
        let input = format!("example%{}.test%{}", hex1, hex2);
        let host = Host::parse(&input).unwrap();

        prop_assert_eq!(host.host(), input.as_str());
    }
}

// sub-delims を含むホスト名
#[test]
fn prop_host_with_sub_delims() {
    let sub_delims = ["!", "$", "&", "'", "(", ")", "*", "+", ",", ";", "="];
    for delim in sub_delims {
        let input = format!("example{}test.com", delim);
        let result = Host::parse(&input);
        assert!(result.is_ok(), "Failed for delim: {}", delim);
    }
}

// ========================================
// IPvFuture のテスト
// ========================================

#[test]
fn prop_host_ipvfuture() {
    // IPvFuture の基本形式: v{HEXDIG}+.{unreserved | sub-delims | ":"} +
    let result = Host::parse("[v1.test]");
    assert!(result.is_ok());

    let result = Host::parse("[vFF.a:b:c]");
    assert!(result.is_ok());

    // 不正な IPvFuture
    let result = Host::parse("[v.]"); // ドットの後が空
    assert!(result.is_err());

    let result = Host::parse("[v]"); // バージョン番号がない
    assert!(result.is_err());
}

// ========================================
// 境界値テスト
// ========================================

// ポート番号の境界値
#[test]
fn prop_host_port_boundary() {
    // 最小値
    let host = Host::parse("example.com:1").unwrap();
    assert_eq!(host.port(), Some(1));

    // 最大値
    let host = Host::parse("example.com:65535").unwrap();
    assert_eq!(host.port(), Some(65535));

    // オーバーフロー
    assert!(Host::parse("example.com:65536").is_err());
    assert!(Host::parse("example.com:100000").is_err());
}

// ========================================
// Clone と PartialEq のテスト
// ========================================

proptest! {
    #[test]
    fn prop_host_clone_eq(name in hostname(), port in prop::option::of(valid_port())) {
        let input = if let Some(p) = port {
            format!("{}:{}", name, p)
        } else {
            name.clone()
        };
        let host = Host::parse(&input).unwrap();
        let cloned = host.clone();

        prop_assert_eq!(host, cloned);
    }
}

// ========================================
// no_panic テスト
// ========================================

proptest! {
    #[test]
    fn prop_host_parse_no_panic(s in "[ -~]{0,64}") {
        let _ = Host::parse(&s);
    }
}
