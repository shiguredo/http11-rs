//! Scheme 型のプロパティテスト

use pbt::{invalid_scheme, valid_scheme};
use proptest::prelude::*;
use shiguredo_http11::Scheme;

proptest! {
    /// valid なバイト列は Scheme::new が Ok を返す
    #[test]
    fn new_accepts_valid_schemes(scheme in valid_scheme()) {
        prop_assert!(Scheme::new(&scheme).is_ok());
    }

    /// invalid なバイト列は Scheme::new が Err を返す
    #[test]
    fn new_rejects_invalid_schemes(scheme in invalid_scheme()) {
        prop_assert!(Scheme::new(&scheme).is_err());
    }

    /// 受理された値の as_bytes() は入力バイト列と一致する（非破壊性）
    #[test]
    fn as_bytes_returns_original(scheme in valid_scheme()) {
        let s = Scheme::new(&scheme).unwrap();
        prop_assert_eq!(s.as_bytes(), scheme.as_slice());
    }

    /// case-insensitive な Eq: 大文字小文字の違いを無視する
    #[test]
    fn eq_is_case_insensitive(scheme in valid_scheme()) {
        let lower: Vec<u8> = scheme.iter().map(|b| b.to_ascii_lowercase()).collect();
        let upper: Vec<u8> = scheme.iter().map(|b| b.to_ascii_uppercase()).collect();
        let h1 = Scheme::new(&lower).unwrap();
        let h2 = Scheme::new(&upper).unwrap();
        prop_assert_eq!(h1, h2);
    }
}
