//! Method のユニットテスト

use shiguredo_http11::Method;

#[test]
fn from_static_matches_new_known_inputs() {
    let names: &[&[u8]] = &[b"GET", b"POST"];
    for &method in names {
        assert_eq!(Method::new(method).unwrap(), Method::from_static(method));
    }
}

#[test]
fn new_rejects_empty() {
    let err = Method::new(b"").unwrap_err();
    assert_eq!(err.input(), "");
}

#[test]
fn new_rejects_invalid_bytes() {
    let err = Method::new(b"GET\r").unwrap_err();
    assert_eq!(err.input(), "GET\r");

    assert!(Method::new(b"GET\n").is_err());
    assert!(Method::new(b"GET ").is_err());
}

#[test]
fn eq_is_case_sensitive() {
    let m1 = Method::new(b"GET").unwrap();
    let m2 = Method::new(b"get").unwrap();
    assert_ne!(m1, m2);
}

#[test]
fn try_from_static_str_valid() {
    let m: Method = "GET".try_into().unwrap();
    assert_eq!(m.as_str(), "GET");
    assert_eq!(m.as_bytes(), b"GET");
}

#[test]
fn try_from_static_str_empty() {
    let err = Method::try_from("").unwrap_err();
    assert_eq!(err.input(), "");
}

#[test]
fn try_from_static_str_invalid() {
    let err = Method::try_from("GET ").unwrap_err();
    assert_eq!(err.input(), "GET ");
}

#[test]
fn try_from_static_bytes_valid() {
    let m: Method = (b"POST" as &'static [u8]).try_into().unwrap();
    assert_eq!(m.as_str(), "POST");
}

#[test]
fn into_input_ownership() {
    let err = Method::try_from("GET ").unwrap_err();
    let input = err.into_input();
    assert_eq!(input, "GET ");
}

#[test]
fn error_implements_std_error() {
    let err = Method::try_from("").unwrap_err();
    let _: &dyn core::error::Error = &err;
}
