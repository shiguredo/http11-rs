//! Host ヘッダーのプロパティテスト

use proptest::prelude::*;
use shiguredo_http11::host::Host;

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
