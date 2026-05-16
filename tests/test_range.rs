//! Range のユニットテスト

use shiguredo_http11::range::{AcceptRanges, ContentRange, Range, RangeError, RangeSpec};

// ========================================
// RangeError のテスト
// ========================================

#[test]
fn test_range_error_display() {
    let errors = [
        (RangeError::Empty, "empty range header"),
        (RangeError::InvalidFormat, "invalid range header format"),
        (RangeError::InvalidUnit, "invalid range unit"),
        (RangeError::InvalidRange, "invalid range specification"),
        (RangeError::InvalidBounds, "invalid range bounds"),
    ];

    for (error, expected) in errors {
        assert_eq!(error.to_string(), expected);
    }
}

// ========================================
// RangeSpec のテスト
// ========================================

// RangeSpec::Suffix length=0 のケース
#[test]
fn test_range_spec_suffix_zero_length() {
    let spec = RangeSpec::Suffix { length: 0 };
    assert!(spec.to_bounds(1000).is_none());
}

// ========================================
// AcceptRanges のテスト
// ========================================

// AcceptRanges::bytes のテスト
#[test]
fn test_accept_ranges_bytes() {
    let ar = AcceptRanges::bytes();
    assert!(ar.accepts_bytes());
    assert!(!ar.is_none());
    assert_eq!(ar.units().len(), 1);
    assert_eq!(ar.units()[0], "bytes");
}

// AcceptRanges::none のテスト
#[test]
fn test_accept_ranges_none() {
    let ar = AcceptRanges::none();
    assert!(!ar.accepts_bytes());
    assert!(ar.is_none());
    assert_eq!(ar.units().len(), 1);
    assert_eq!(ar.units()[0], "none");
}

// AcceptRanges ラウンドトリップ
#[test]
fn test_accept_ranges_bytes_roundtrip() {
    let ar = AcceptRanges::bytes();
    let displayed = ar.to_string();
    let reparsed = AcceptRanges::parse(&displayed).unwrap();
    assert!(reparsed.accepts_bytes());
}

// AcceptRanges Display
#[test]
fn test_accept_ranges_display() {
    let ar = AcceptRanges::bytes();
    assert_eq!(ar.to_string(), "bytes");

    let ar2 = AcceptRanges::none();
    assert_eq!(ar2.to_string(), "none");
}

// AcceptRanges 複数単位
#[test]
fn test_accept_ranges_multiple_units() {
    let ar = AcceptRanges::parse("bytes, custom").unwrap();
    assert_eq!(ar.units().len(), 2);
    assert!(ar.accepts_bytes());
}

// ========================================
// エラーケースのテスト
// ========================================

#[test]
fn test_range_parse_errors() {
    // 空
    assert!(matches!(Range::parse(""), Err(RangeError::Empty)));

    // = がない
    assert!(matches!(
        Range::parse("bytes0-499"),
        Err(RangeError::InvalidFormat)
    ));

    // 単位が空
    assert!(matches!(
        Range::parse("=0-499"),
        Err(RangeError::InvalidUnit)
    ));

    // 範囲指定が空
    assert!(matches!(Range::parse("bytes="), Err(RangeError::Empty)));

    // 不正な範囲
    assert!(matches!(
        Range::parse("bytes=-"),
        Err(RangeError::InvalidRange)
    ));

    // start > end
    assert!(matches!(
        Range::parse("bytes=500-100"),
        Err(RangeError::InvalidBounds)
    ));

    // 数値でない
    assert!(matches!(
        Range::parse("bytes=abc-def"),
        Err(RangeError::InvalidRange)
    ));
}

#[test]
fn test_content_range_parse_errors() {
    // 空
    assert!(matches!(ContentRange::parse(""), Err(RangeError::Empty)));

    // スペースがない
    assert!(matches!(
        ContentRange::parse("bytes0-499/1000"),
        Err(RangeError::InvalidFormat)
    ));

    // / がない
    assert!(matches!(
        ContentRange::parse("bytes 0-499"),
        Err(RangeError::InvalidFormat)
    ));

    // start > end
    assert!(matches!(
        ContentRange::parse("bytes 500-100/1000"),
        Err(RangeError::InvalidBounds)
    ));

    // 不正な数値
    assert!(matches!(
        ContentRange::parse("bytes abc-def/1000"),
        Err(RangeError::InvalidFormat)
    ));

    // 不正な長さ
    assert!(matches!(
        ContentRange::parse("bytes 0-499/abc"),
        Err(RangeError::InvalidFormat)
    ));

    // complete_length <= end (RFC 9110 Section 14.4)
    assert!(matches!(
        ContentRange::parse("bytes 0-100/50"),
        Err(RangeError::InvalidBounds)
    ));
    assert!(matches!(
        ContentRange::parse("bytes 0-0/0"),
        Err(RangeError::InvalidBounds)
    ));
}

#[test]
fn test_accept_ranges_parse_errors() {
    // 空
    assert!(matches!(AcceptRanges::parse(""), Err(RangeError::Empty)));

    // 空白のみ
    assert!(matches!(AcceptRanges::parse("   "), Err(RangeError::Empty)));

    // カンマのみ
    assert!(matches!(AcceptRanges::parse(",,,"), Err(RangeError::Empty)));
}

// ========================================
// Accept-Ranges の none 混在拒否 (RFC 9110 Section 14.3)
// ========================================

#[test]
fn test_accept_ranges_none_mixed_with_other_units_error() {
    // none は他の単位と混在できない
    assert!(matches!(
        AcceptRanges::parse("bytes, none"),
        Err(RangeError::InvalidUnit)
    ));
    assert!(matches!(
        AcceptRanges::parse("none, bytes"),
        Err(RangeError::InvalidUnit)
    ));
}

#[test]
fn test_accept_ranges_none_alone_ok() {
    // none 単独は正常
    let ar = AcceptRanges::parse("none").unwrap();
    assert!(ar.is_none());
}

#[test]
fn test_accept_ranges_multiple_units_without_none_ok() {
    // none を含まない複数単位は正常
    let ar = AcceptRanges::parse("bytes, items").unwrap();
    assert!(!ar.is_none());
    assert!(ar.accepts_bytes());
}

// ========================================
// ContentRange::length() 境界値テスト
// ========================================

#[test]
fn test_content_range_length_overflow_returns_none() {
    // (s=0, e=u64::MAX) → None (唯一の overflow ケース)
    let cr = ContentRange::new_bytes(0, u64::MAX, None);
    assert_eq!(cr.length(), None);
}

#[test]
fn test_content_range_length_max_some() {
    // (s=1, e=u64::MAX) → Some(u64::MAX)
    let cr = ContentRange::new_bytes(1, u64::MAX, None);
    assert_eq!(cr.length(), Some(u64::MAX));
}

#[test]
fn test_content_range_length_max_result() {
    // (s=0, e=u64::MAX-1) → Some(u64::MAX)
    let cr = ContentRange::new_bytes(0, u64::MAX - 1, None);
    assert_eq!(cr.length(), Some(u64::MAX));
}

#[test]
fn test_content_range_length_single_byte() {
    // (s=0, e=0) → Some(1)
    let cr = ContentRange::new_bytes(0, 0, Some(1));
    assert_eq!(cr.length(), Some(1));
}

#[test]
fn test_content_range_length_max_single_byte() {
    // (s=u64::MAX, e=u64::MAX) → Some(1)
    let cr = ContentRange::new_bytes(u64::MAX, u64::MAX, None);
    assert_eq!(cr.length(), Some(1));
}

#[test]
fn test_content_range_length_max_two_bytes() {
    // (s=u64::MAX-1, e=u64::MAX) → Some(2)
    let cr = ContentRange::new_bytes(u64::MAX - 1, u64::MAX, None);
    assert_eq!(cr.length(), Some(2));
}

// ========================================
// ContentRange::new_bytes() バリデーションテスト
// ========================================

#[test]
#[should_panic(expected = "ContentRange: start must be <= end")]
fn test_new_bytes_start_greater_than_end() {
    ContentRange::new_bytes(10, 5, None);
}

#[test]
#[should_panic(expected = "ContentRange: complete_length must be > last-pos")]
fn test_new_bytes_complete_length_less_than_end() {
    ContentRange::new_bytes(0, 100, Some(50));
}

#[test]
#[should_panic(expected = "ContentRange: complete_length must be > last-pos")]
fn test_new_bytes_complete_length_equal_to_end() {
    ContentRange::new_bytes(u64::MAX, u64::MAX, Some(u64::MAX));
}

#[test]
#[should_panic(expected = "ContentRange: complete_length must be > last-pos")]
fn test_new_bytes_complete_length_zero_with_max_end() {
    ContentRange::new_bytes(0, u64::MAX, Some(0));
}

// ========================================
// ContentRange::new_bytes() 正常系テスト
// ========================================

#[test]
fn test_new_bytes_max_range_none_length() {
    // new_bytes(0, u64::MAX, None) は正常終了する
    let cr = ContentRange::new_bytes(0, u64::MAX, None);
    assert_eq!(cr.length(), None);
    assert_eq!(cr.to_string(), "bytes 0-18446744073709551615/*");
}
