//! Vary ヘッダーのユニットテスト

use shiguredo_http11::vary::Vary;

#[test]
fn parse_any() {
    let vary = Vary::parse("*").unwrap();
    assert!(vary.is_any());
}

#[test]
fn parse_fields() {
    let vary = Vary::parse("Accept-Encoding, User-Agent").unwrap();
    assert_eq!(
        vary.fields(),
        &["accept-encoding".to_string(), "user-agent".to_string()]
    );
}

#[test]
fn parse_invalid() {
    assert!(Vary::parse("bad value").is_err());
}

/// RFC 9110 Section 5.6.1.2: 空フィールド値・空要素は受理する
#[test]
fn parse_empty_elements() {
    let vary = Vary::parse("").unwrap();
    assert!(!vary.is_any());
    assert!(vary.fields().is_empty());

    let vary = Vary::parse(",").unwrap();
    assert!(vary.fields().is_empty());

    let vary = Vary::parse("Accept,,User-Agent").unwrap();
    assert_eq!(vary.fields().len(), 2);
}

/// RFC 9110 Section 12.5.5: リスト内の "*" はワイルドカード
#[test]
fn parse_wildcard_in_list() {
    let vary = Vary::parse("*, Accept").unwrap();
    assert!(vary.is_any());
    assert!(vary.fields().is_empty());

    let vary = Vary::parse("Accept, *").unwrap();
    assert!(vary.is_any());
    assert!(vary.fields().is_empty());
}
