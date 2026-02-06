//! Host ヘッダーのユニットテスト

use shiguredo_http11::host::{Host, HostError};

// ========================================
// HostError のテスト
// ========================================

#[test]
fn test_host_error_display() {
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

// ========================================
// パースエラーのテスト
// ========================================

#[test]
fn test_host_parse_errors() {
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

// sub-delims を含むホスト名
#[test]
fn test_host_with_sub_delims() {
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
fn test_host_ipvfuture() {
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
fn test_host_port_boundary() {
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
