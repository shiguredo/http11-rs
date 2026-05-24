//! HeaderName 型のプロパティテスト

use pbt::{invalid_header_name, valid_header_name};
use proptest::prelude::*;
use shiguredo_http11::HeaderName;

proptest! {
    /// valid なバイト列は HeaderName::new が Ok を返す
    #[test]
    fn new_accepts_valid_names(name in valid_header_name()) {
        prop_assert!(HeaderName::new(&name).is_ok());
    }

    /// invalid なバイト列は HeaderName::new が Err を返す
    #[test]
    fn new_rejects_invalid_names(name in invalid_header_name()) {
        prop_assert!(HeaderName::new(&name).is_err());
    }

    /// 受理された値の as_bytes() は入力バイト列と一致する（非破壊性）
    #[test]
    fn as_bytes_returns_original(name in valid_header_name()) {
        let h = HeaderName::new(&name).unwrap();
        prop_assert_eq!(h.as_bytes(), name.as_slice());
    }

    /// case-insensitive な Eq: 大文字小文字の違いを無視する
    #[test]
    fn eq_is_case_insensitive(name in valid_header_name()) {
        let lower: Vec<u8> = name.iter().map(|b| b.to_ascii_lowercase()).collect();
        let upper: Vec<u8> = name.iter().map(|b| b.to_ascii_uppercase()).collect();
        let h1 = HeaderName::new(&lower).unwrap();
        let h2 = HeaderName::new(&upper).unwrap();
        prop_assert_eq!(h1, h2);
    }
}
