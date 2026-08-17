//! Host ヘッダーのプロパティテスト

use shiguredo_http11::host::Host;

// ========================================
// ジェネレータ定義
// ========================================

// 有効なホスト名文字
fn hostname_char(ctx: &mut noprop::TestCaseContext) -> char {
    match noprop::sample_usize_in(ctx, 0..4) {
        0 => char::from(b'a' + noprop::sample_usize_in(ctx, 0..26) as u8),
        1 => char::from(b'A' + noprop::sample_usize_in(ctx, 0..26) as u8),
        2 => char::from(b'0' + noprop::sample_usize_in(ctx, 0..10) as u8),
        _ => '-',
    }
}

fn hostname_label(ctx: &mut noprop::TestCaseContext) -> String {
    let len = noprop::sample_usize_in(ctx, 1..=15);
    let mut s = String::with_capacity(len);
    for _ in 0..len {
        s.push(hostname_char(ctx));
    }
    s
}

fn hostname(ctx: &mut noprop::TestCaseContext) -> String {
    let labels = noprop::sample_usize_in(ctx, 1..=3);
    let mut parts = Vec::with_capacity(labels);
    for _ in 0..labels {
        parts.push(hostname_label(ctx));
    }
    parts.join(".")
}

// 有効なポート番号
fn valid_port(ctx: &mut noprop::TestCaseContext) -> u16 {
    noprop::sample_usize_in(ctx, 1..=65535) as u16
}

// IPv4 アドレス
fn ipv4_addr(ctx: &mut noprop::TestCaseContext) -> String {
    let octets = [
        noprop::sample_u8(ctx),
        noprop::sample_u8(ctx),
        noprop::sample_u8(ctx),
        noprop::sample_u8(ctx),
    ];
    format!("{}.{}.{}.{}", octets[0], octets[1], octets[2], octets[3])
}

// IPv6 アドレス (簡易版)
fn ipv6_addr(ctx: &mut noprop::TestCaseContext) -> String {
    match noprop::sample_usize_in(ctx, 0..6) {
        0 => "::1".to_string(),
        1 => "::".to_string(),
        2 => "fe80::1".to_string(),
        3 => "2001:db8::1".to_string(),
        4 => "::ffff:192.168.1.1".to_string(),
        _ => format!(
            "2001:db8::{:x}:{:x}:{:x}:{:x}",
            noprop::sample_u16(ctx),
            noprop::sample_u16(ctx),
            noprop::sample_u16(ctx),
            noprop::sample_u16(ctx),
        ),
    }
}

// パーセントエンコーディング用の 16 進文字 (0-9 / A-F / a-f)
fn hex_digit_char(ctx: &mut noprop::TestCaseContext) -> char {
    const HEX: &[u8] = b"0123456789ABCDEFabcdef";
    HEX[noprop::sample_usize_in(ctx, 0..HEX.len())] as char
}

// ========================================
// ホスト名パースのテスト
// ========================================

// ホスト名ラウンドトリップ
#[test]
fn prop_host_hostname_roundtrip() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let name = hostname(ctx);
        let host = Host::parse(&name).expect("Host のパースは成功するはず (実装バグ)");
        assert_eq!(host.host(), name.as_str());
        assert_eq!(host.port(), None);
        assert!(!host.is_ipv6());

        // Display
        let display = host.to_string();
        assert_eq!(display, name);
        Ok(())
    })?;

    // ジェネレータは valid-by-construction であり、ケース棄却が発生しないことの検証
    assert_eq!(
        runner.stats().rejected_cases,
        0,
        "ジェネレータが valid-by-construction であること\n{runner}"
    );
    Ok(())
}

// ホスト名 + ポートラウンドトリップ
#[test]
fn prop_host_hostname_port_roundtrip() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let name = hostname(ctx);
        let port = valid_port(ctx);
        let input = format!("{}:{}", name, port);
        let host = Host::parse(&input).expect("Host のパースは成功するはず (実装バグ)");

        assert_eq!(host.host(), name.as_str());
        assert_eq!(host.port(), Some(port));
        assert!(!host.is_ipv6());

        // Display
        let display = host.to_string();
        assert_eq!(display, input);
        Ok(())
    })?;

    // ジェネレータは valid-by-construction であり、ケース棄却が発生しないことの検証
    assert_eq!(
        runner.stats().rejected_cases,
        0,
        "ジェネレータが valid-by-construction であること\n{runner}"
    );
    Ok(())
}

// ========================================
// IPv4 パースのテスト
// ========================================

// IPv4 ラウンドトリップ
#[test]
fn prop_host_ipv4_roundtrip() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let addr = ipv4_addr(ctx);
        let host = Host::parse(&addr).expect("Host のパースは成功するはず (実装バグ)");

        assert_eq!(host.host(), addr.as_str());
        assert_eq!(host.port(), None);
        assert!(!host.is_ipv6());
        Ok(())
    })?;

    // ジェネレータは valid-by-construction であり、ケース棄却が発生しないことの検証
    assert_eq!(
        runner.stats().rejected_cases,
        0,
        "ジェネレータが valid-by-construction であること\n{runner}"
    );
    Ok(())
}

// IPv4 + ポートラウンドトリップ
#[test]
fn prop_host_ipv4_port_roundtrip() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let addr = ipv4_addr(ctx);
        let port = valid_port(ctx);
        let input = format!("{}:{}", addr, port);
        let host = Host::parse(&input).expect("Host のパースは成功するはず (実装バグ)");

        assert_eq!(host.host(), addr.as_str());
        assert_eq!(host.port(), Some(port));
        Ok(())
    })?;

    // ジェネレータは valid-by-construction であり、ケース棄却が発生しないことの検証
    assert_eq!(
        runner.stats().rejected_cases,
        0,
        "ジェネレータが valid-by-construction であること\n{runner}"
    );
    Ok(())
}

// ========================================
// IPv6 パースのテスト
// ========================================

// IPv6 ラウンドトリップ
#[test]
fn prop_host_ipv6_roundtrip() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let addr = ipv6_addr(ctx);
        let input = format!("[{}]", addr);
        let host = Host::parse(&input).expect("Host のパースは成功するはず (実装バグ)");

        assert_eq!(host.host(), input.as_str());
        assert_eq!(host.port(), None);
        assert!(host.is_ipv6());
        Ok(())
    })?;

    // ジェネレータは valid-by-construction であり、ケース棄却が発生しないことの検証
    assert_eq!(
        runner.stats().rejected_cases,
        0,
        "ジェネレータが valid-by-construction であること\n{runner}"
    );
    Ok(())
}

// IPv6 + ポートラウンドトリップ
#[test]
fn prop_host_ipv6_port_roundtrip() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let addr = ipv6_addr(ctx);
        let port = valid_port(ctx);
        let input = format!("[{}]:{}", addr, port);
        let host = Host::parse(&input).expect("Host のパースは成功するはず (実装バグ)");

        assert_eq!(host.host(), format!("[{}]", addr));
        assert_eq!(host.port(), Some(port));
        assert!(host.is_ipv6());
        Ok(())
    })?;

    // ジェネレータは valid-by-construction であり、ケース棄却が発生しないことの検証
    assert_eq!(
        runner.stats().rejected_cases,
        0,
        "ジェネレータが valid-by-construction であること\n{runner}"
    );
    Ok(())
}

// ========================================
// 特殊なホスト名のテスト
// ========================================

// パーセントエンコーディングを含むホスト名
#[test]
fn prop_host_percent_encoded() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let hex1 = [hex_digit_char(ctx), hex_digit_char(ctx)];
        let hex2 = [hex_digit_char(ctx), hex_digit_char(ctx)];
        let input = format!("example%{}{}.test%{}{}", hex1[0], hex1[1], hex2[0], hex2[1]);
        let host = Host::parse(&input).expect("Host のパースは成功するはず (実装バグ)");

        assert_eq!(host.host(), input.as_str());
        Ok(())
    })?;

    // ジェネレータは valid-by-construction であり、ケース棄却が発生しないことの検証
    assert_eq!(
        runner.stats().rejected_cases,
        0,
        "ジェネレータが valid-by-construction であること\n{runner}"
    );
    Ok(())
}
