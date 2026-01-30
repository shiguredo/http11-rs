//! 条件付きリクエストのプロパティテスト

use proptest::prelude::*;
use shiguredo_http11::conditional::{
    ConditionalError, IfMatch, IfModifiedSince, IfNoneMatch, IfRange, IfUnmodifiedSince,
};
use shiguredo_http11::etag::EntityTag;

// ========================================
// Strategy 定義
// ========================================

// ETag 値 (有効な文字のみ)
fn etag_value() -> impl Strategy<Value = String> {
    "[a-zA-Z0-9_-]{1,16}".prop_map(|s| s)
}

// ETag 文字列 (引用符付き)
fn strong_etag_str() -> impl Strategy<Value = String> {
    etag_value().prop_map(|v| format!("\"{}\"", v))
}

// Weak ETag 文字列
fn weak_etag_str() -> impl Strategy<Value = String> {
    etag_value().prop_map(|v| format!("W/\"{}\"", v))
}

// 任意の ETag 文字列
fn any_etag_str() -> impl Strategy<Value = String> {
    prop_oneof![strong_etag_str(), weak_etag_str(),]
}

// HTTP 日付文字列
fn http_date_str() -> impl Strategy<Value = String> {
    (
        1u8..=28,
        1u8..=12,
        1990u16..=2100,
        0u8..=23,
        0u8..=59,
        0u8..=59,
    )
        .prop_map(|(day, month, year, hour, minute, second)| {
            let dow_names = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];
            let month_names = [
                "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
            ];
            let dow_idx = ((day as usize) + (month as usize) + (year as usize)) % 7;
            let dow = dow_names[dow_idx];
            let mon = month_names[(month - 1) as usize];

            format!(
                "{}, {:02} {} {:04} {:02}:{:02}:{:02} GMT",
                dow, day, mon, year, hour, minute, second
            )
        })
}

// ========================================
// ConditionalError のテスト
// ========================================

#[test]
fn prop_conditional_error_display() {
    let errors = [
        (ConditionalError::Empty, "empty conditional header"),
        (
            ConditionalError::InvalidFormat,
            "invalid conditional header format",
        ),
        (
            ConditionalError::ETagError,
            "invalid etag in conditional header",
        ),
        (
            ConditionalError::DateError,
            "invalid date in conditional header",
        ),
    ];

    for (error, expected) in errors {
        assert_eq!(error.to_string(), expected);
    }
}

#[test]
fn prop_conditional_error_is_error_trait() {
    let error: Box<dyn std::error::Error> = Box::new(ConditionalError::Empty);
    assert_eq!(error.to_string(), "empty conditional header");
}

// ========================================
// IfMatch のテスト
// ========================================

// 単一 ETag のラウンドトリップ
proptest! {
    #[test]
    fn prop_if_match_single_roundtrip(tag in etag_value()) {
        let input = format!("\"{}\"", tag);
        let im = IfMatch::parse(&input).unwrap();
        prop_assert!(!im.is_any());

        let displayed = im.to_string();
        let reparsed = IfMatch::parse(&displayed).unwrap();

        prop_assert_eq!(im, reparsed);
    }
}

// 複数 ETag のラウンドトリップ
proptest! {
    #[test]
    fn prop_if_match_multiple_roundtrip(tags in proptest::collection::vec(etag_value(), 1..4)) {
        let etag_strs: Vec<String> = tags.iter().map(|t| format!("\"{}\"", t)).collect();
        let list_str = etag_strs.join(", ");

        let im = IfMatch::parse(&list_str).unwrap();
        let displayed = im.to_string();
        let reparsed = IfMatch::parse(&displayed).unwrap();

        prop_assert_eq!(im, reparsed);
    }
}

// ワイルドカード
#[test]
fn prop_if_match_wildcard() {
    let im = IfMatch::parse("*").unwrap();
    assert!(im.is_any());
    assert!(im.matches(&EntityTag::strong("anything").unwrap()));
    assert_eq!(im.to_string(), "*");
}

// matches 動作 (Strong 比較)
proptest! {
    #[test]
    fn prop_if_match_matches_strong(tags in proptest::collection::vec(etag_value(), 1..4), check_tag in etag_value()) {
        let etag_strs: Vec<String> = tags.iter().map(|t| format!("\"{}\"", t)).collect();
        let list_str = etag_strs.join(", ");

        let im = IfMatch::parse(&list_str).unwrap();
        let etag = EntityTag::strong(&check_tag).unwrap();

        // Strong 比較なので、tags に check_tag が含まれていれば true
        let expected = tags.contains(&check_tag);
        prop_assert_eq!(im.matches(&etag), expected);
    }
}

// Weak ETag は If-Match では一致しない
proptest! {
    #[test]
    fn prop_if_match_weak_not_match(tag in etag_value()) {
        let input = format!("W/\"{}\"", tag);
        let im = IfMatch::parse(&input).unwrap();
        let strong_etag = EntityTag::strong(&tag).unwrap();

        // If-Match は Strong 比較を使用するため、Weak ETag は一致しない
        prop_assert!(!im.matches(&strong_etag));
    }
}

// ========================================
// IfNoneMatch のテスト
// ========================================

// 単一 ETag のラウンドトリップ
proptest! {
    #[test]
    fn prop_if_none_match_single_roundtrip(tag in etag_value()) {
        let input = format!("\"{}\"", tag);
        let inm = IfNoneMatch::parse(&input).unwrap();
        let displayed = inm.to_string();
        let reparsed = IfNoneMatch::parse(&displayed).unwrap();

        prop_assert_eq!(inm, reparsed);
    }
}

// 複数 ETag のラウンドトリップ
proptest! {
    #[test]
    fn prop_if_none_match_multiple_roundtrip(tags in proptest::collection::vec(etag_value(), 1..4)) {
        let etag_strs: Vec<String> = tags.iter().map(|t| format!("\"{}\"", t)).collect();
        let list_str = etag_strs.join(", ");

        let inm = IfNoneMatch::parse(&list_str).unwrap();
        let displayed = inm.to_string();
        let reparsed = IfNoneMatch::parse(&displayed).unwrap();

        prop_assert_eq!(inm, reparsed);
    }
}

// ワイルドカード
#[test]
fn prop_if_none_match_wildcard() {
    let inm = IfNoneMatch::parse("*").unwrap();
    assert!(inm.is_any());
    // * は全てに一致するので、どの ETag でも処理しない
    assert!(!inm.matches(&EntityTag::strong("anything").unwrap()));
}

// matches 動作 (Weak 比較)
proptest! {
    #[test]
    fn prop_if_none_match_matches_weak(tags in proptest::collection::vec(etag_value(), 1..4), check_tag in etag_value()) {
        let etag_strs: Vec<String> = tags.iter().map(|t| format!("\"{}\"", t)).collect();
        let list_str = etag_strs.join(", ");

        let inm = IfNoneMatch::parse(&list_str).unwrap();
        let etag = EntityTag::strong(&check_tag).unwrap();

        // matches が true = 処理すべき = tags に含まれていない
        let expected = !tags.contains(&check_tag);
        prop_assert_eq!(inm.matches(&etag), expected);
    }
}

// Weak ETag は If-None-Match で一致する
proptest! {
    #[test]
    fn prop_if_none_match_weak_match(tag in etag_value()) {
        let input = format!("W/\"{}\"", tag);
        let inm = IfNoneMatch::parse(&input).unwrap();
        let strong_etag = EntityTag::strong(&tag).unwrap();

        // If-None-Match は Weak 比較を使用するため、同じタグなら一致
        // matches が false = 一致するので処理しない
        prop_assert!(!inm.matches(&strong_etag));
    }
}

// ========================================
// IfModifiedSince のテスト
// ========================================

// ラウンドトリップ
proptest! {
    #[test]
    fn prop_if_modified_since_roundtrip(date_str in http_date_str()) {
        let ims = IfModifiedSince::parse(&date_str).unwrap();
        let displayed = ims.to_string();
        let reparsed = IfModifiedSince::parse(&displayed).unwrap();

        prop_assert_eq!(ims.date().day(), reparsed.date().day());
        prop_assert_eq!(ims.date().month(), reparsed.date().month());
        prop_assert_eq!(ims.date().year(), reparsed.date().year());
        prop_assert_eq!(ims.date().hour(), reparsed.date().hour());
        prop_assert_eq!(ims.date().minute(), reparsed.date().minute());
        prop_assert_eq!(ims.date().second(), reparsed.date().second());
    }
}

// is_modified
proptest! {
    #[test]
    fn prop_if_modified_since_is_modified(date_str in http_date_str()) {
        let ims = IfModifiedSince::parse(&date_str).unwrap();
        let same_date = ims.date();

        // 同じ日付なら modified ではない
        prop_assert!(!ims.is_modified(same_date));
    }
}

// パースエラー
#[test]
fn prop_if_modified_since_parse_errors() {
    assert!(matches!(
        IfModifiedSince::parse("invalid date"),
        Err(ConditionalError::DateError)
    ));
    assert!(matches!(
        IfModifiedSince::parse("2024-01-01"),
        Err(ConditionalError::DateError)
    ));
}

// ========================================
// IfUnmodifiedSince のテスト
// ========================================

// ラウンドトリップ
proptest! {
    #[test]
    fn prop_if_unmodified_since_roundtrip(date_str in http_date_str()) {
        let ius = IfUnmodifiedSince::parse(&date_str).unwrap();
        let displayed = ius.to_string();
        let reparsed = IfUnmodifiedSince::parse(&displayed).unwrap();

        prop_assert_eq!(ius.date().day(), reparsed.date().day());
        prop_assert_eq!(ius.date().month(), reparsed.date().month());
        prop_assert_eq!(ius.date().year(), reparsed.date().year());
    }
}

// パースエラー
#[test]
fn prop_if_unmodified_since_parse_errors() {
    assert!(matches!(
        IfUnmodifiedSince::parse("invalid date"),
        Err(ConditionalError::DateError)
    ));
}

// ========================================
// IfRange のテスト
// ========================================

// ETag ラウンドトリップ (Strong)
proptest! {
    #[test]
    fn prop_if_range_strong_etag_roundtrip(tag in etag_value()) {
        let input = format!("\"{}\"", tag);
        let ir = IfRange::parse(&input).unwrap();

        prop_assert!(ir.is_etag());
        prop_assert!(!ir.is_date());
        prop_assert_eq!(ir.etag().unwrap().tag(), tag.as_str());
        prop_assert!(ir.date().is_none());

        let displayed = ir.to_string();
        let reparsed = IfRange::parse(&displayed).unwrap();
        prop_assert!(reparsed.is_etag());
        prop_assert_eq!(ir.etag().unwrap().tag(), reparsed.etag().unwrap().tag());
    }
}

// ETag ラウンドトリップ (Weak)
proptest! {
    #[test]
    fn prop_if_range_weak_etag_roundtrip(tag in etag_value()) {
        let input = format!("W/\"{}\"", tag);
        let ir = IfRange::parse(&input).unwrap();

        prop_assert!(ir.is_etag());
        prop_assert!(ir.etag().unwrap().is_weak());
        prop_assert_eq!(ir.etag().unwrap().tag(), tag.as_str());

        let displayed = ir.to_string();
        let reparsed = IfRange::parse(&displayed).unwrap();
        prop_assert!(reparsed.is_etag());
        prop_assert!(reparsed.etag().unwrap().is_weak());
    }
}

// 日付ラウンドトリップ
proptest! {
    #[test]
    fn prop_if_range_date_roundtrip(date_str in http_date_str()) {
        let ir = IfRange::parse(&date_str).unwrap();

        prop_assert!(ir.is_date());
        prop_assert!(!ir.is_etag());
        prop_assert!(ir.etag().is_none());
        prop_assert!(ir.date().is_some());

        let displayed = ir.to_string();
        let reparsed = IfRange::parse(&displayed).unwrap();
        prop_assert!(reparsed.is_date());
        prop_assert_eq!(
            ir.date().unwrap().day(),
            reparsed.date().unwrap().day()
        );
    }
}

// パースエラー
#[test]
fn prop_if_range_parse_errors() {
    // 空
    assert!(matches!(IfRange::parse(""), Err(ConditionalError::Empty)));
    assert!(matches!(
        IfRange::parse("   "),
        Err(ConditionalError::Empty)
    ));

    // 不正な形式
    assert!(matches!(
        IfRange::parse("invalid"),
        Err(ConditionalError::DateError)
    ));
}

// RFC 9110 Section 8.8.3: W/ は case-sensitive (小文字 w/ は拒否)
proptest! {
    #[test]
    fn prop_if_range_weak_lowercase_rejected(tag in etag_value()) {
        let input = format!("w/\"{}\"", tag);
        // 小文字 w/ は RFC 非準拠のため拒否される
        prop_assert!(IfRange::parse(&input).is_err());
    }
}

// ========================================
// Clone と PartialEq のテスト
// ========================================

proptest! {
    #[test]
    fn prop_if_match_clone_eq(tags in proptest::collection::vec(etag_value(), 1..4)) {
        let etag_strs: Vec<String> = tags.iter().map(|t| format!("\"{}\"", t)).collect();
        let list_str = etag_strs.join(", ");

        let im = IfMatch::parse(&list_str).unwrap();
        let cloned = im.clone();
        prop_assert_eq!(im, cloned);
    }
}

proptest! {
    #[test]
    fn prop_if_none_match_clone_eq(tags in proptest::collection::vec(etag_value(), 1..4)) {
        let etag_strs: Vec<String> = tags.iter().map(|t| format!("\"{}\"", t)).collect();
        let list_str = etag_strs.join(", ");

        let inm = IfNoneMatch::parse(&list_str).unwrap();
        let cloned = inm.clone();
        prop_assert_eq!(inm, cloned);
    }
}

proptest! {
    #[test]
    fn prop_if_modified_since_clone_eq(date_str in http_date_str()) {
        let ims = IfModifiedSince::parse(&date_str).unwrap();
        let cloned = ims.clone();
        prop_assert_eq!(ims, cloned);
    }
}

proptest! {
    #[test]
    fn prop_if_unmodified_since_clone_eq(date_str in http_date_str()) {
        let ius = IfUnmodifiedSince::parse(&date_str).unwrap();
        let cloned = ius.clone();
        prop_assert_eq!(ius, cloned);
    }
}

proptest! {
    #[test]
    fn prop_if_range_clone_eq(etag_str in any_etag_str()) {
        let ir = IfRange::parse(&etag_str).unwrap();
        let cloned = ir.clone();
        prop_assert_eq!(ir, cloned);
    }
}

// ========================================
// no_panic テスト
// ========================================

proptest! {
    #[test]
    fn prop_if_match_parse_no_panic(s in "[ -~]{0,128}") {
        let _ = IfMatch::parse(&s);
    }
}

proptest! {
    #[test]
    fn prop_if_none_match_parse_no_panic(s in "[ -~]{0,128}") {
        let _ = IfNoneMatch::parse(&s);
    }
}

proptest! {
    #[test]
    fn prop_if_modified_since_parse_no_panic(s in "[ -~]{0,64}") {
        let _ = IfModifiedSince::parse(&s);
    }
}

proptest! {
    #[test]
    fn prop_if_unmodified_since_parse_no_panic(s in "[ -~]{0,64}") {
        let _ = IfUnmodifiedSince::parse(&s);
    }
}

proptest! {
    #[test]
    fn prop_if_range_parse_no_panic(s in "[ -~]{0,64}") {
        let _ = IfRange::parse(&s);
    }
}
