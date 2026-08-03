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
        let h = HeaderName::new(&name).expect("ヘッダー名のパースは成功するはず (実装バグ)");
        prop_assert_eq!(h.as_bytes(), name.as_slice());
    }

    /// case-insensitive な Eq: 大文字小文字の違いを無視する
    #[test]
    fn eq_is_case_insensitive(name in valid_header_name()) {
        let lower: Vec<u8> = name.iter().map(|b| b.to_ascii_lowercase()).collect();
        let upper: Vec<u8> = name.iter().map(|b| b.to_ascii_uppercase()).collect();
        let h1 = HeaderName::new(&lower).expect("ヘッダー名のパースは成功するはず (実装バグ)");
        let h2 = HeaderName::new(&upper).expect("ヘッダー名のパースは成功するはず (実装バグ)");
        prop_assert_eq!(h1, h2);
    }

    /// TryFrom<&'static [u8]> と new() の受理集合が一致する
    #[test]
    fn try_from_static_bytes_acceptance_equals_new(name in valid_header_name()) {
        let static_bytes: &'static [u8] = Box::leak(name.clone().into_boxed_slice());
        let r1 = HeaderName::new(&name);
        let r2: Result<HeaderName, _> = static_bytes.try_into();
        prop_assert!(r1.is_ok());
        prop_assert!(r2.is_ok());
        let b1 = r1.expect("ヘッダー名のパースは成功するはず (実装バグ)").as_bytes().to_vec();
        let b2 = r2.expect("ヘッダー名のパースは成功するはず (実装バグ)").as_bytes().to_vec();
        prop_assert_eq!(b1, b2);
    }

    /// TryFrom<&'static str> と new() の受理集合が一致する（valid な入力）
    #[test]
    fn try_from_static_str_acceptance_equals_new(name in valid_header_name()) {
        if name.is_empty() {
            return Ok(());
        }
        let name_str = String::from_utf8_lossy(&name).into_owned();
        let static_str: &'static str = Box::leak(name_str.into_boxed_str());
        let r1 = HeaderName::new(&name);
        let r2: Result<HeaderName, _> = static_str.try_into();
        prop_assert!(r1.is_ok());
        prop_assert!(r2.is_ok());
        let b1 = r1.expect("ヘッダー名のパースは成功するはず (実装バグ)").as_bytes().to_vec();
        let b2 = r2.expect("ヘッダー名のパースは成功するはず (実装バグ)").as_bytes().to_vec();
        prop_assert_eq!(b1, b2);
    }
}
