//! Upgrade ヘッダーのユニットテスト

use shiguredo_http11::upgrade::Upgrade;

/// RFC 9110 Section 5.6.1.2: 空フィールド値・空要素は受理する
#[test]
fn parse_empty_elements() {
    let upgrade = Upgrade::parse("").unwrap();
    assert!(upgrade.protocols().is_empty());

    let upgrade = Upgrade::parse(",").unwrap();
    assert!(upgrade.protocols().is_empty());

    let upgrade = Upgrade::parse("websocket,,h2c").unwrap();
    assert_eq!(upgrade.protocols().len(), 2);
}

#[test]
fn display() {
    let upgrade = Upgrade::parse("websocket, h2c/1.0").unwrap();
    assert_eq!(upgrade.to_string(), "websocket, h2c/1.0");
}
