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
        let m = Method::new(&method).expect("メソッドのパースは成功するはず (実装バグ)");
        prop_assert_eq!(m.as_bytes(), method.as_slice());
    }

    /// case-sensitive な Eq: 大文字小文字を区別する
    #[test]
    fn eq_is_case_sensitive(method in valid_method()) {
        let lower: Vec<u8> = method.iter().map(|b| b.to_ascii_lowercase()).collect();
        let upper: Vec<u8> = method.iter().map(|b| b.to_ascii_uppercase()).collect();
        let m1 = Method::new(&lower).expect("メソッドのパースは成功するはず (実装バグ)");
        let m2 = Method::new(&upper).expect("メソッドのパースは成功するはず (実装バグ)");

        // 大文字小文字の変換で変化があった場合のみ NE になる
        if lower == upper {
            prop_assert_eq!(m1, m2);
        } else {
            prop_assert_ne!(m1, m2);
        }
    }

    /// TryFrom<&'static [u8]> と new() の受理集合が一致する
    #[test]
    fn try_from_static_bytes_acceptance_equals_new(method in valid_method()) {
        let static_bytes: &'static [u8] = Box::leak(method.clone().into_boxed_slice());
        let r1 = Method::new(&method);
        let r2: Result<Method, _> = static_bytes.try_into();
        prop_assert!(r1.is_ok());
        prop_assert!(r2.is_ok());
        let b1 = r1.expect("メソッドのパースは成功するはず (実装バグ)").as_bytes().to_vec();
        let b2 = r2.expect("メソッドのパースは成功するはず (実装バグ)").as_bytes().to_vec();
        prop_assert_eq!(b1, b2);
    }

    /// TryFrom<&'static str> と new() の受理集合が一致する（valid な入力）
    #[test]
    fn try_from_static_str_acceptance_equals_new(method in valid_method()) {
        if method.is_empty() {
            return Ok(());
        }
        let method_str = String::from_utf8_lossy(&method).into_owned();
        let static_str: &'static str = Box::leak(method_str.into_boxed_str());
        let r1 = Method::new(&method);
        let r2: Result<Method, _> = static_str.try_into();
        prop_assert!(r1.is_ok());
        prop_assert!(r2.is_ok());
        let b1 = r1.expect("メソッドのパースは成功するはず (実装バグ)").as_bytes().to_vec();
        let b2 = r2.expect("メソッドのパースは成功するはず (実装バグ)").as_bytes().to_vec();
        prop_assert_eq!(b1, b2);
    }
}
