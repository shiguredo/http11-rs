//! Upgrade ヘッダーのユニットテスト

use shiguredo_http11::upgrade::Upgrade;

#[test]
fn parse_simple() {
    let upgrade = Upgrade::parse("websocket").unwrap();
    assert!(upgrade.has_protocol("websocket"));
    assert_eq!(upgrade.protocols().len(), 1);
}

#[test]
fn parse_with_version() {
    let upgrade = Upgrade::parse("h2c/1.0, websocket").unwrap();
    assert_eq!(upgrade.protocols()[0].name(), "h2c");
    assert_eq!(upgrade.protocols()[0].version(), Some("1.0"));
}

#[test]
fn parse_invalid() {
    assert!(Upgrade::parse("bad value").is_err());
    assert!(Upgrade::parse("websocket/").is_err());
    assert!(Upgrade::parse("websocket/1/2").is_err());
}

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
