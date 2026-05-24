//! Method 型のプロパティテスト

use pbt::{invalid_method, valid_method};
use proptest::prelude::*;
use shiguredo_http11::Method;

proptest! {
    /// valid なバイト列は Method::new が Ok を返す
    #[test]
    fn new_accepts_valid_methods(method in valid_method()) {
        prop_assert!(Method::new(&method).is_ok());
    }

    /// invalid なバイト列は Method::new が Err を返す
    #[test]
    fn new_rejects_invalid_methods(method in invalid_method()) {
        prop_assert!(Method::new(&method).is_err());
    }

    /// 受理された値の as_bytes() は入力バイト列と一致する（非破壊性）
    #[test]
    fn as_bytes_returns_original(method in valid_method()) {
        let m = Method::new(&method).unwrap();
        prop_assert_eq!(m.as_bytes(), method.as_slice());
    }

    /// case-sensitive な Eq: 大文字小文字を区別する
    #[test]
    fn eq_is_case_sensitive(method in valid_method()) {
        let lower: Vec<u8> = method.iter().map(|b| b.to_ascii_lowercase()).collect();
        let upper: Vec<u8> = method.iter().map(|b| b.to_ascii_uppercase()).collect();
        let m1 = Method::new(&lower).unwrap();
        let m2 = Method::new(&upper).unwrap();

        // 大文字小文字の変換で変化があった場合のみ NE になる
        if lower == upper {
            prop_assert_eq!(m1, m2);
        } else {
            prop_assert_ne!(m1, m2);
        }
    }
}
