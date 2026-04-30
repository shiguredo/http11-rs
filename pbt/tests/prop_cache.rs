//! キャッシュヘッダーのプロパティテスト

use proptest::prelude::*;
use shiguredo_http11::cache::{Age, CacheControl, Expires};

// ========================================
// Strategy 定義
// ========================================

// 秒数 (0 から 1 年)
fn seconds() -> impl Strategy<Value = u64> {
    0u64..31536001 // 1 年 + 1
}

// ========================================
// CacheControl のテスト
// ========================================

// 全ディレクティブのラウンドトリップ
proptest! {
    #[test]
    fn prop_cache_control_all_directives_roundtrip(
        max_age in prop::option::of(seconds()),
        s_maxage in prop::option::of(seconds()),
        no_cache in any::<bool>(),
        no_store in any::<bool>(),
        no_transform in any::<bool>(),
        must_revalidate in any::<bool>(),
        proxy_revalidate in any::<bool>(),
        is_public in any::<bool>(),
        is_private in any::<bool>(),
        immutable in any::<bool>()
    ) {
        // 空の CacheControl もラウンドトリップ可能
        let mut cc = CacheControl::new();
        if let Some(ma) = max_age {
            cc = cc.with_max_age(ma);
        }
        if let Some(sma) = s_maxage {
            cc = cc.with_s_maxage(sma);
        }
        if no_cache { cc = cc.with_no_cache(); }
        if no_store { cc = cc.with_no_store(); }
        if no_transform { cc = cc.with_no_transform(); }
        if must_revalidate { cc = cc.with_must_revalidate(); }
        if proxy_revalidate { cc = cc.with_proxy_revalidate(); }
        if is_public { cc = cc.with_public(); }
        if is_private { cc = cc.with_private(); }
        if immutable { cc = cc.with_immutable(); }

        let header = cc.to_string();
        let reparsed = CacheControl::parse(&header).unwrap();

        prop_assert_eq!(cc.max_age(), reparsed.max_age());
        prop_assert_eq!(cc.s_maxage(), reparsed.s_maxage());
        prop_assert_eq!(cc.is_no_cache(), reparsed.is_no_cache());
        prop_assert_eq!(cc.is_no_store(), reparsed.is_no_store());
        prop_assert_eq!(cc.is_no_transform(), reparsed.is_no_transform());
        prop_assert_eq!(cc.is_must_revalidate(), reparsed.is_must_revalidate());
        prop_assert_eq!(cc.is_proxy_revalidate(), reparsed.is_proxy_revalidate());
        prop_assert_eq!(cc.is_public(), reparsed.is_public());
        prop_assert_eq!(cc.is_private(), reparsed.is_private());
        prop_assert_eq!(cc.is_immutable(), reparsed.is_immutable());
    }
}

// is_cacheable の正確性
proptest! {
    #[test]
    fn prop_cache_control_is_cacheable(
        max_age in prop::option::of(seconds()),
        s_maxage in prop::option::of(seconds()),
        no_store in any::<bool>(),
        is_public in any::<bool>()
    ) {
        let mut cc = CacheControl::new();
        if let Some(ma) = max_age {
            cc = cc.with_max_age(ma);
        }
        if let Some(sma) = s_maxage {
            cc = cc.with_s_maxage(sma);
        }
        if no_store {
            cc = cc.with_no_store();
        }
        if is_public {
            cc = cc.with_public();
        }

        // no-store があれば cacheable ではない
        // そうでなければ public または max-age または s-maxage があれば cacheable
        let expected = !no_store && (is_public || max_age.is_some() || s_maxage.is_some());
        prop_assert_eq!(cc.is_cacheable(), expected);
    }
}

// 大文字小文字混在
proptest! {
    #[test]
    fn prop_cache_control_case_insensitive(max_age in 0u64..86400) {
        let inputs = [
            format!("MAX-AGE={}", max_age),
            format!("Max-Age={}", max_age),
            format!("PUBLIC, max-age={}", max_age),
            format!("public, MAX-AGE={}", max_age),
        ];

        for input in inputs {
            let cc = CacheControl::parse(&input).unwrap();
            prop_assert_eq!(cc.max_age(), Some(max_age));
        }
    }
}

// max-stale 値あり
proptest! {
    #[test]
    fn prop_cache_control_max_stale_with_value(seconds in 0u64..86400) {
        let input = format!("max-stale={}", seconds);
        let cc = CacheControl::parse(&input).unwrap();
        prop_assert_eq!(cc.max_stale(), Some(seconds));
    }
}

// min-fresh
proptest! {
    #[test]
    fn prop_cache_control_min_fresh(seconds in 0u64..86400) {
        let input = format!("min-fresh={}", seconds);
        let cc = CacheControl::parse(&input).unwrap();
        prop_assert_eq!(cc.min_fresh(), Some(seconds));
    }
}

// stale-while-revalidate
proptest! {
    #[test]
    fn prop_cache_control_stale_while_revalidate(seconds in 0u64..86400) {
        let input = format!("stale-while-revalidate={}", seconds);
        let cc = CacheControl::parse(&input).unwrap();
        prop_assert_eq!(cc.stale_while_revalidate(), Some(seconds));
    }
}

// stale-if-error
proptest! {
    #[test]
    fn prop_cache_control_stale_if_error(seconds in 0u64..86400) {
        let input = format!("stale-if-error={}", seconds);
        let cc = CacheControl::parse(&input).unwrap();
        prop_assert_eq!(cc.stale_if_error(), Some(seconds));
    }
}

// ========================================
// Age のテスト
// ========================================

// Age ラウンドトリップ
proptest! {
    #[test]
    fn prop_age_roundtrip(secs in seconds()) {
        let age = Age::new(secs);
        let header = age.to_string();
        let reparsed = Age::parse(&header).unwrap();

        prop_assert_eq!(age.seconds(), reparsed.seconds());
    }
}

// ========================================
// Expires のテスト
// ========================================

// Expires ラウンドトリップ
proptest! {
    #[test]
    fn prop_expires_roundtrip(
        day in 1u8..=28,
        month in 1u8..=12,
        year in 1990u16..=2100,
        hour in 0u8..=23,
        minute in 0u8..=59,
        second in 0u8..=59
    ) {
        let dow_names = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];
        let month_names = [
            "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
        ];
        let dow_idx = ((day as usize) + (month as usize) + (year as usize)) % 7;
        let dow = dow_names[dow_idx];
        let mon = month_names[(month - 1) as usize];

        let date_str = format!(
            "{}, {:02} {} {:04} {:02}:{:02}:{:02} GMT",
            dow, day, mon, year, hour, minute, second
        );

        let expires = Expires::parse(&date_str, 2026).unwrap();
        let displayed = expires.to_string();
        let reparsed = Expires::parse(&displayed, 2026).unwrap();

        prop_assert_eq!(expires.date().day(), reparsed.date().day());
        prop_assert_eq!(expires.date().month(), reparsed.date().month());
        prop_assert_eq!(expires.date().year(), reparsed.date().year());
        prop_assert_eq!(expires.date().hour(), reparsed.date().hour());
        prop_assert_eq!(expires.date().minute(), reparsed.date().minute());
        prop_assert_eq!(expires.date().second(), reparsed.date().second());
    }
}
