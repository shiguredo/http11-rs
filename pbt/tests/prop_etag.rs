//! ETag のプロパティテスト

use proptest::prelude::*;
use shiguredo_http11::etag::{EntityTag, parse_etag_list};

// ========================================
// ETag パースのテスト
// ========================================

// Strong ETag のラウンドトリップ
proptest! {
    #[test]
    fn prop_etag_strong_roundtrip(tag in "[a-zA-Z0-9_-]{0,32}") {
        let etag = EntityTag::strong(&tag).unwrap();
        let displayed = etag.to_string();
        let reparsed = EntityTag::parse(&displayed).unwrap();

        prop_assert!(reparsed.is_strong());
        prop_assert_eq!(reparsed.tag(), tag.as_str());
    }
}

// Weak ETag のラウンドトリップ
proptest! {
    #[test]
    fn prop_etag_weak_roundtrip(tag in "[a-zA-Z0-9_-]{0,32}") {
        let etag = EntityTag::weak(&tag).unwrap();
        let displayed = etag.to_string();
        let reparsed = EntityTag::parse(&displayed).unwrap();

        prop_assert!(reparsed.is_weak());
        prop_assert_eq!(reparsed.tag(), tag.as_str());
    }
}

// Strong 比較の正確性
proptest! {
    #[test]
    fn prop_etag_strong_compare(tag1 in "[a-zA-Z0-9]{1,16}", tag2 in "[a-zA-Z0-9]{1,16}", weak1 in any::<bool>(), weak2 in any::<bool>()) {
        let e1 = if weak1 {
            EntityTag::weak(&tag1)
        } else {
            EntityTag::strong(&tag1)
        }
        .unwrap();
        let e2 = if weak2 {
            EntityTag::weak(&tag2)
        } else {
            EntityTag::strong(&tag2)
        }
        .unwrap();

        // Strong 比較: 両方 strong で tag が同じ場合のみ true
        let expected = !weak1 && !weak2 && tag1 == tag2;
        prop_assert_eq!(e1.strong_compare(&e2), expected);
    }
}

// Weak 比較の正確性
proptest! {
    #[test]
    fn prop_etag_weak_compare(tag1 in "[a-zA-Z0-9]{1,16}", tag2 in "[a-zA-Z0-9]{1,16}", weak1 in any::<bool>(), weak2 in any::<bool>()) {
        let e1 = if weak1 {
            EntityTag::weak(&tag1)
        } else {
            EntityTag::strong(&tag1)
        }
        .unwrap();
        let e2 = if weak2 {
            EntityTag::weak(&tag2)
        } else {
            EntityTag::strong(&tag2)
        }
        .unwrap();

        // Weak 比較: tag が同じ場合は true (weak フラグは無視)
        let expected = tag1 == tag2;
        prop_assert_eq!(e1.weak_compare(&e2), expected);
    }
}

// ETag リストのパース
proptest! {
    #[test]
    fn prop_etag_list_roundtrip(tags in proptest::collection::vec("[a-zA-Z0-9]{1,8}", 1..5)) {
        let etag_strs: Vec<String> = tags.iter().map(|t| format!("\"{}\"", t)).collect();
        let list_str = etag_strs.join(", ");

        let list = parse_etag_list(&list_str).unwrap();
        let displayed = list.to_string();

        // 再パース
        let reparsed = parse_etag_list(&displayed).unwrap();
        prop_assert_eq!(list, reparsed);
    }
}

// 任意の文字列で ETag パースがパニックしない
proptest! {
    #[test]
    fn prop_etag_parse_no_panic(s in "[ -~]{0,64}") {
        let _ = EntityTag::parse(&s);
    }
}

// 任意の文字列で ETag リストパースがパニックしない
proptest! {
    #[test]
    fn prop_etag_list_parse_no_panic(s in "[ -~]{0,64}") {
        let _ = parse_etag_list(&s);
    }
}
