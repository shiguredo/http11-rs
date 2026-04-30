//! Cookie のプロパティテスト (cookie.rs)

use proptest::prelude::*;
use shiguredo_http11::cookie::{Cookie, SameSite, SetCookie};

// ========================================
// Cookie パースのテスト
// ========================================

// Cookie のラウンドトリップ
proptest! {
    #[test]
    fn prop_cookie_roundtrip(name in "[a-zA-Z][a-zA-Z0-9_-]{0,15}", value in "[a-zA-Z0-9_-]{0,32}") {
        let cookie = Cookie::new(&name, &value).unwrap();
        let displayed = cookie.to_string();
        let cookies = Cookie::parse(&displayed).unwrap();
        prop_assert_eq!(cookies.len(), 1);
        prop_assert_eq!(cookies[0].name(), name.as_str());
        prop_assert_eq!(cookies[0].value(), value.as_str());
    }
}

// 複数 Cookie のパース
proptest! {
    #[test]
    fn prop_cookie_parse_multiple(name1 in "[a-zA-Z][a-zA-Z0-9]{0,7}", value1 in "[a-zA-Z0-9]{0,16}", name2 in "[a-zA-Z][a-zA-Z0-9]{0,7}", value2 in "[a-zA-Z0-9]{0,16}") {
        let cookie_str = format!("{}={}; {}={}", name1, value1, name2, value2);
        let cookies = Cookie::parse(&cookie_str).unwrap();
        prop_assert_eq!(cookies.len(), 2);
        prop_assert_eq!(cookies[0].name(), name1.as_str());
        prop_assert_eq!(cookies[0].value(), value1.as_str());
        prop_assert_eq!(cookies[1].name(), name2.as_str());
        prop_assert_eq!(cookies[1].value(), value2.as_str());
    }
}

// SetCookie のラウンドトリップ
proptest! {
    #[test]
    fn prop_set_cookie_roundtrip(name in "[a-zA-Z][a-zA-Z0-9_-]{0,15}", value in "[a-zA-Z0-9_-]{0,32}") {
        let cookie = SetCookie::new(&name, &value).unwrap();
        let displayed = cookie.to_string();
        let reparsed = SetCookie::parse(&displayed, 2026).unwrap();
        prop_assert_eq!(reparsed.name(), name.as_str());
        prop_assert_eq!(reparsed.value(), value.as_str());
    }
}

// SetCookie 属性付きラウンドトリップ
proptest! {
    #[test]
    fn prop_set_cookie_with_attributes(name in "[a-zA-Z][a-zA-Z0-9]{0,7}", value in "[a-zA-Z0-9]{0,16}", path in "/[a-zA-Z0-9_-]{0,16}", max_age in 0i64..=86400) {
        let cookie = SetCookie::new(&name, &value)
            .unwrap()
            .with_path(&path)
            .with_max_age(max_age)
            .with_secure(true)
            .with_http_only(true);

        let displayed = cookie.to_string();
        let reparsed = SetCookie::parse(&displayed, 2026).unwrap();

        prop_assert_eq!(reparsed.name(), name.as_str());
        prop_assert_eq!(reparsed.value(), value.as_str());
        prop_assert_eq!(reparsed.path(), Some(path.as_str()));
        prop_assert_eq!(reparsed.max_age(), Some(max_age));
        prop_assert!(reparsed.secure());
        prop_assert!(reparsed.http_only());
    }
}

// SameSite 属性ラウンドトリップ
proptest! {
    #[test]
    fn prop_set_cookie_same_site(name in "[a-zA-Z][a-zA-Z0-9]{0,7}", value in "[a-zA-Z0-9]{0,16}", same_site in prop_oneof![Just(SameSite::Strict), Just(SameSite::Lax), Just(SameSite::None)]) {
        let cookie = SetCookie::new(&name, &value)
            .unwrap()
            .with_same_site(same_site);

        let displayed = cookie.to_string();
        let reparsed = SetCookie::parse(&displayed, 2026).unwrap();

        prop_assert_eq!(reparsed.same_site(), Some(same_site));
    }
}

// Domain 属性付き SetCookie
proptest! {
    #[test]
    fn prop_set_cookie_with_domain(name in "[a-zA-Z][a-zA-Z0-9]{0,7}", value in "[a-zA-Z0-9]{0,16}", domain in "[a-z]{1,8}\\.[a-z]{2,4}") {
        let cookie = SetCookie::new(&name, &value).unwrap().with_domain(&domain);

        let displayed = cookie.to_string();
        let reparsed = SetCookie::parse(&displayed, 2026).unwrap();

        prop_assert_eq!(reparsed.domain(), Some(domain.as_str()));
    }
}
