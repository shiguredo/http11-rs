//! キャッシュヘッダーのプロパティテスト

use proptest::prelude::*;
use shiguredo_http11::cache::{Age, CacheControl, CacheError, Expires};

// ========================================
// Strategy 定義
// ========================================

// 秒数 (0 から 1 年)
fn seconds() -> impl Strategy<Value = u64> {
    0u64..31536001 // 1 年 + 1
}

// Cache-Control ディレクティブ
fn cache_directive() -> impl Strategy<Value = &'static str> {
    prop_oneof![
        Just("no-cache"),
        Just("no-store"),
        Just("no-transform"),
        Just("only-if-cached"),
        Just("must-revalidate"),
        Just("proxy-revalidate"),
        Just("must-understand"),
        Just("public"),
        Just("private"),
        Just("immutable"),
    ]
}

// 値付きディレクティブ
fn cache_directive_with_value() -> impl Strategy<Value = String> {
    prop_oneof![
        seconds().prop_map(|s| format!("max-age={}", s)),
        seconds().prop_map(|s| format!("s-maxage={}", s)),
        seconds().prop_map(|s| format!("max-stale={}", s)),
        seconds().prop_map(|s| format!("min-fresh={}", s)),
        seconds().prop_map(|s| format!("stale-while-revalidate={}", s)),
        seconds().prop_map(|s| format!("stale-if-error={}", s)),
    ]
}

// ========================================
// CacheError のテスト
// ========================================

#[test]
fn cache_error_display() {
    let errors = [
        (CacheError::Empty, "empty cache header"),
        (CacheError::InvalidFormat, "invalid cache header format"),
        (CacheError::InvalidNumber, "invalid number in cache header"),
        (CacheError::InvalidDate, "invalid date in cache header"),
    ];

    for (error, expected) in errors {
        assert_eq!(error.to_string(), expected);
    }
}

#[test]
fn cache_error_is_error_trait() {
    let error: Box<dyn std::error::Error> = Box::new(CacheError::Empty);
    assert_eq!(error.to_string(), "empty cache header");
}

// ========================================
// CacheControl のテスト
// ========================================

// max-age ラウンドトリップ
proptest! {
    #[test]
    fn cache_control_max_age_roundtrip(max_age in seconds()) {
        let cc = CacheControl::new().with_max_age(max_age);
        let header = cc.to_string();
        let reparsed = CacheControl::parse(&header).unwrap();

        prop_assert_eq!(cc.max_age(), reparsed.max_age());
    }
}

// s-maxage ラウンドトリップ
proptest! {
    #[test]
    fn cache_control_s_maxage_roundtrip(s_maxage in seconds()) {
        let cc = CacheControl::new().with_s_maxage(s_maxage);
        let header = cc.to_string();
        let reparsed = CacheControl::parse(&header).unwrap();

        prop_assert_eq!(cc.s_maxage(), reparsed.s_maxage());
    }
}

// 複合ラウンドトリップ
proptest! {
    #[test]
    fn cache_control_combined_roundtrip(
        max_age in seconds(),
        is_public in any::<bool>(),
        no_cache in any::<bool>(),
        must_revalidate in any::<bool>()
    ) {
        let mut cc = CacheControl::new().with_max_age(max_age);
        if is_public {
            cc = cc.with_public();
        }
        if no_cache {
            cc = cc.with_no_cache();
        }
        if must_revalidate {
            cc = cc.with_must_revalidate();
        }

        let header = cc.to_string();
        let reparsed = CacheControl::parse(&header).unwrap();

        prop_assert_eq!(cc.max_age(), reparsed.max_age());
        prop_assert_eq!(cc.is_public(), reparsed.is_public());
        prop_assert_eq!(cc.is_no_cache(), reparsed.is_no_cache());
        prop_assert_eq!(cc.is_must_revalidate(), reparsed.is_must_revalidate());
    }
}

// 全ディレクティブのラウンドトリップ
proptest! {
    #[test]
    fn cache_control_all_directives_roundtrip(
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
    fn cache_control_is_cacheable(
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

// 単一ディレクティブのパース
proptest! {
    #[test]
    fn cache_control_single_directive(directive in cache_directive()) {
        let cc = CacheControl::parse(directive).unwrap();

        match directive {
            "no-cache" => prop_assert!(cc.is_no_cache()),
            "no-store" => prop_assert!(cc.is_no_store()),
            "no-transform" => prop_assert!(cc.is_no_transform()),
            "only-if-cached" => prop_assert!(cc.is_only_if_cached()),
            "must-revalidate" => prop_assert!(cc.is_must_revalidate()),
            "proxy-revalidate" => prop_assert!(cc.is_proxy_revalidate()),
            "must-understand" => prop_assert!(cc.is_must_understand()),
            "public" => prop_assert!(cc.is_public()),
            "private" => prop_assert!(cc.is_private()),
            "immutable" => prop_assert!(cc.is_immutable()),
            _ => unreachable!(),
        }
    }
}

// 値付きディレクティブのパース
proptest! {
    #[test]
    fn cache_control_directive_with_value(directive in cache_directive_with_value()) {
        let result = CacheControl::parse(&directive);
        prop_assert!(result.is_ok());
    }
}

// 大文字小文字混在
proptest! {
    #[test]
    fn cache_control_case_insensitive(max_age in 0u64..86400) {
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

// max-stale 値なし
#[test]
fn cache_control_max_stale_without_value() {
    let cc = CacheControl::parse("max-stale").unwrap();
    assert_eq!(cc.max_stale(), Some(u64::MAX));
}

// max-stale 値あり
proptest! {
    #[test]
    fn cache_control_max_stale_with_value(seconds in 0u64..86400) {
        let input = format!("max-stale={}", seconds);
        let cc = CacheControl::parse(&input).unwrap();
        prop_assert_eq!(cc.max_stale(), Some(seconds));
    }
}

// min-fresh
proptest! {
    #[test]
    fn cache_control_min_fresh(seconds in 0u64..86400) {
        let input = format!("min-fresh={}", seconds);
        let cc = CacheControl::parse(&input).unwrap();
        prop_assert_eq!(cc.min_fresh(), Some(seconds));
    }
}

// stale-while-revalidate
proptest! {
    #[test]
    fn cache_control_stale_while_revalidate(seconds in 0u64..86400) {
        let input = format!("stale-while-revalidate={}", seconds);
        let cc = CacheControl::parse(&input).unwrap();
        prop_assert_eq!(cc.stale_while_revalidate(), Some(seconds));
    }
}

// stale-if-error
proptest! {
    #[test]
    fn cache_control_stale_if_error(seconds in 0u64..86400) {
        let input = format!("stale-if-error={}", seconds);
        let cc = CacheControl::parse(&input).unwrap();
        prop_assert_eq!(cc.stale_if_error(), Some(seconds));
    }
}

// パースエラー
#[test]
fn cache_control_parse_errors() {
    // 空文字列はデフォルトの CacheControl として扱う
    let cc = CacheControl::parse("").unwrap();
    assert_eq!(cc, CacheControl::default());
    let cc = CacheControl::parse("   ").unwrap();
    assert_eq!(cc, CacheControl::default());

    // 不正な数値
    assert!(matches!(
        CacheControl::parse("max-age=abc"),
        Err(CacheError::InvalidNumber)
    ));
    assert!(matches!(
        CacheControl::parse("max-age=-1"),
        Err(CacheError::InvalidNumber)
    ));
}

// to_header_value
proptest! {
    #[test]
    fn cache_control_to_header_value(ma in seconds()) {
        let cc = CacheControl::new().with_max_age(ma);
        let header = cc.to_header_value();
        let expected = format!("max-age={}", ma);
        prop_assert!(header.contains(&expected));
    }
}

// Default trait
#[test]
fn cache_control_default() {
    let cc = CacheControl::default();
    assert_eq!(cc.max_age(), None);
    assert!(!cc.is_no_cache());
    assert!(!cc.is_public());
}

// Clone と PartialEq
proptest! {
    #[test]
    fn cache_control_clone_eq(max_age in prop::option::of(seconds()), is_public in any::<bool>()) {
        let mut cc = CacheControl::new();
        if let Some(ma) = max_age {
            cc = cc.with_max_age(ma);
        }
        if is_public {
            cc = cc.with_public();
        }

        let cloned = cc.clone();
        prop_assert_eq!(cc, cloned);
    }
}

// ========================================
// Age のテスト
// ========================================

// Age ラウンドトリップ
proptest! {
    #[test]
    fn age_roundtrip(secs in seconds()) {
        let age = Age::new(secs);
        let header = age.to_string();
        let reparsed = Age::parse(&header).unwrap();

        prop_assert_eq!(age.seconds(), reparsed.seconds());
    }
}

// Age 0
#[test]
fn age_zero() {
    let age = Age::new(0);
    assert_eq!(age.seconds(), 0);
    assert_eq!(age.to_string(), "0");

    let parsed = Age::parse("0").unwrap();
    assert_eq!(parsed.seconds(), 0);
}

// Age パースエラー
#[test]
fn age_parse_errors() {
    // 空
    assert!(matches!(Age::parse(""), Err(CacheError::Empty)));
    assert!(matches!(Age::parse("   "), Err(CacheError::Empty)));

    // 不正な数値
    assert!(matches!(Age::parse("abc"), Err(CacheError::InvalidNumber)));
    assert!(matches!(Age::parse("-1"), Err(CacheError::InvalidNumber)));
    assert!(matches!(Age::parse("1.5"), Err(CacheError::InvalidNumber)));
}

// Age to_header_value
proptest! {
    #[test]
    fn age_to_header_value(secs in seconds()) {
        let age = Age::new(secs);
        prop_assert_eq!(age.to_header_value(), secs.to_string());
    }
}

// Age Clone と PartialEq
proptest! {
    #[test]
    fn age_clone_eq(secs in seconds()) {
        let age = Age::new(secs);
        let cloned = age.clone();
        prop_assert_eq!(age, cloned);
    }
}

// ========================================
// Expires のテスト
// ========================================

// Expires ラウンドトリップ
proptest! {
    #[test]
    fn expires_roundtrip(
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

        let expires = Expires::parse(&date_str).unwrap();
        let displayed = expires.to_string();
        let reparsed = Expires::parse(&displayed).unwrap();

        prop_assert_eq!(expires.date().day(), reparsed.date().day());
        prop_assert_eq!(expires.date().month(), reparsed.date().month());
        prop_assert_eq!(expires.date().year(), reparsed.date().year());
        prop_assert_eq!(expires.date().hour(), reparsed.date().hour());
        prop_assert_eq!(expires.date().minute(), reparsed.date().minute());
        prop_assert_eq!(expires.date().second(), reparsed.date().second());
    }
}

// Expires パースエラー
#[test]
fn expires_parse_errors() {
    // 不正な日付形式
    assert!(matches!(
        Expires::parse("invalid date"),
        Err(CacheError::InvalidDate)
    ));
    assert!(matches!(
        Expires::parse("2024-01-01"),
        Err(CacheError::InvalidDate)
    ));
}

// Expires to_header_value
#[test]
fn expires_to_header_value() {
    let expires = Expires::parse("Sun, 06 Nov 1994 08:49:37 GMT").unwrap();
    let header = expires.to_header_value();
    assert!(header.contains("1994"));
    assert!(header.contains("Nov"));
}

// Expires Clone と PartialEq
#[test]
fn expires_clone_eq() {
    let expires = Expires::parse("Sun, 06 Nov 1994 08:49:37 GMT").unwrap();
    let cloned = expires.clone();
    assert_eq!(expires, cloned);
}

// ========================================
// no_panic テスト
// ========================================

proptest! {
    #[test]
    fn cache_control_parse_no_panic(s in "[ -~]{0,128}") {
        let _ = CacheControl::parse(&s);
    }
}

proptest! {
    #[test]
    fn age_parse_no_panic(s in "[ -~]{0,32}") {
        let _ = Age::parse(&s);
    }
}

proptest! {
    #[test]
    fn expires_parse_no_panic(s in "[ -~]{0,64}") {
        let _ = Expires::parse(&s);
    }
}
