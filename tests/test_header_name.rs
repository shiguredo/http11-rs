//! HeaderName のユニットテスト

use shiguredo_http11::HeaderName;

#[test]
fn from_static_matches_new_known_inputs() {
    let names: &[&[u8]] = &[b"host", b"content-type", b"x-custom-header"];
    for &name in names {
        assert_eq!(
            HeaderName::new(name).unwrap(),
            HeaderName::from_static(name)
        );
    }
}

#[test]
fn new_rejects_empty() {
    let err = HeaderName::new(b"").unwrap_err();
    assert_eq!(err.input(), "");
}

#[test]
fn new_rejects_invalid_bytes() {
    let err = HeaderName::new(b"host name").unwrap_err();
    assert_eq!(err.input(), "host name");

    assert!(HeaderName::new(b"host:name").is_err());
    assert!(HeaderName::new(b"host\r\nname").is_err());
}

#[test]
fn as_bytes_returns_original() {
    let h = HeaderName::new(b"Host").unwrap();
    assert_eq!(h.as_bytes(), b"Host");
}

#[test]
fn eq_is_case_insensitive() {
    let h1 = HeaderName::new(b"host").unwrap();
    let h2 = HeaderName::new(b"HOST").unwrap();
    let h3 = HeaderName::new(b"Host").unwrap();
    assert_eq!(h1, h2);
    assert_eq!(h2, h3);
}

#[test]
fn try_from_static_str_valid() {
    let h: HeaderName = "Host".try_into().unwrap();
    assert_eq!(h.as_str(), "Host");
    assert_eq!(h.as_bytes(), b"Host");
}

#[test]
fn try_from_static_str_empty() {
    let err = HeaderName::try_from("").unwrap_err();
    assert_eq!(err.input(), "");
}

#[test]
fn try_from_static_str_invalid() {
    let err = HeaderName::try_from("host name").unwrap_err();
    assert_eq!(err.input(), "host name");
}

#[test]
fn try_from_static_bytes_valid() {
    let h: HeaderName = (b"Host" as &'static [u8]).try_into().unwrap();
    assert_eq!(h.as_str(), "Host");
}

#[test]
fn into_input_ownership() {
    let err = HeaderName::try_from("bad name").unwrap_err();
    let input = err.into_input();
    assert_eq!(input, "bad name");
}

#[test]
fn error_implements_std_error() {
    let err = HeaderName::try_from("").unwrap_err();
    let _: &dyn core::error::Error = &err;
}
