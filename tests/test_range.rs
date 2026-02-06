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
        (
            RangeError::InvalidBounds,
            "invalid range bounds (start > end)",
        ),
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

// AcceptRanges Clone/PartialEq
#[test]
fn test_accept_ranges_clone_eq() {
    let ar = AcceptRanges::bytes();
    let cloned = ar.clone();
    assert_eq!(ar, cloned);
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
