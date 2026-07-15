//! StatusClass / StatusCode の Property-Based Test
//!
//! `StatusClass::from_status_code` のパーティション性と、
//! `StatusCode::class` が `from_status_code` と整合することを検証する。

use proptest::prelude::*;
use shiguredo_http11::{StatusClass, StatusCode};

proptest! {
    /// 任意の u16 に対する from_status_code のパーティション性
    #[test]
    fn prop_status_class_partition(code: u16) {
        match StatusClass::from_status_code(code) {
            Some(StatusClass::Informational) => prop_assert!((100..=199).contains(&code)),
            Some(StatusClass::Successful)    => prop_assert!((200..=299).contains(&code)),
            Some(StatusClass::Redirection)   => prop_assert!((300..=399).contains(&code)),
            Some(StatusClass::ClientError)   => prop_assert!((400..=499).contains(&code)),
            Some(StatusClass::ServerError)   => prop_assert!((500..=599).contains(&code)),
            None => prop_assert!(!(100..=599).contains(&code)),
        }
    }

    /// IANA 登録済み code は必ず class が定まり、from_status_code と一致する
    #[test]
    fn prop_status_code_class_consistency(code in 100u16..=599) {
        if let Some(sc) = StatusCode::from_code(code) {
            let expected = StatusClass::from_status_code(code)
                .expect("100..=599 always classified");
            prop_assert_eq!(sc.class(), expected);
        }
    }
}
