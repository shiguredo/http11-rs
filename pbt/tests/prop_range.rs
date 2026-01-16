//! Range 関連のプロパティテスト

use proptest::prelude::*;
use shiguredo_http11::range::{AcceptRanges, ContentRange, Range, RangeError, RangeSpec};

// ========================================
// RangeError のテスト
// ========================================

#[test]
fn range_error_display() {
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

#[test]
fn range_error_is_error_trait() {
    let error: Box<dyn std::error::Error> = Box::new(RangeError::Empty);
    assert_eq!(error.to_string(), "empty range header");
}

#[test]
fn range_error_clone_eq() {
    let error = RangeError::InvalidFormat;
    let cloned = error.clone();
    assert_eq!(error, cloned);
}

// ========================================
// RangeSpec のテスト
// ========================================

// RangeSpec::Range の Display
proptest! {
    #[test]
    fn range_spec_range_display(start in 0u64..10000, end in 0u64..10000) {
        let (start, end) = if start <= end { (start, end) } else { (end, start) };
        let spec = RangeSpec::Range { start, end };
        let display = spec.to_string();

        prop_assert!(display.contains('-'));
        let expected = format!("{}-{}", start, end);
        prop_assert_eq!(display, expected);
    }
}

// RangeSpec::FromStart の Display
proptest! {
    #[test]
    fn range_spec_from_start_display(start in 0u64..10000) {
        let spec = RangeSpec::FromStart { start };
        let display = spec.to_string();

        prop_assert!(display.ends_with('-'));
        let expected = format!("{}-", start);
        prop_assert_eq!(display, expected);
    }
}

// RangeSpec::Suffix の Display
proptest! {
    #[test]
    fn range_spec_suffix_display(length in 1u64..10000) {
        let spec = RangeSpec::Suffix { length };
        let display = spec.to_string();

        prop_assert!(display.starts_with('-'));
        let expected = format!("-{}", length);
        prop_assert_eq!(display, expected);
    }
}

// RangeSpec::Range の to_bounds
proptest! {
    #[test]
    fn range_spec_range_to_bounds(start in 0u64..1000, end in 0u64..1000, total in 1u64..2000) {
        let (start, end) = if start <= end { (start, end) } else { (end, start) };
        let spec = RangeSpec::Range { start, end };

        if let Some((s, e)) = spec.to_bounds(total) {
            prop_assert!(s <= e);
            prop_assert!(e < total);
            prop_assert_eq!(s, start);
        }
    }
}

// RangeSpec::FromStart の to_bounds
proptest! {
    #[test]
    fn range_spec_from_start_to_bounds(start in 0u64..1000, total in 1u64..2000) {
        let spec = RangeSpec::FromStart { start };

        if start < total {
            let bounds = spec.to_bounds(total);
            prop_assert!(bounds.is_some());
            let (s, e) = bounds.unwrap();
            prop_assert_eq!(s, start);
            prop_assert_eq!(e, total - 1);
        } else {
            prop_assert!(spec.to_bounds(total).is_none());
        }
    }
}

// RangeSpec::Suffix の to_bounds
proptest! {
    #[test]
    fn range_spec_suffix_to_bounds(length in 1u64..1000, total in 1u64..2000) {
        let spec = RangeSpec::Suffix { length };
        let bounds = spec.to_bounds(total);

        prop_assert!(bounds.is_some());
        let (s, e) = bounds.unwrap();
        prop_assert!(s <= e);
        prop_assert_eq!(e, total - 1);
        // 長さがトータルを超える場合は 0 から開始
        if length >= total {
            prop_assert_eq!(s, 0);
        } else {
            prop_assert_eq!(s, total - length);
        }
    }
}

// RangeSpec::Suffix length=0 のケース
#[test]
fn range_spec_suffix_zero_length() {
    let spec = RangeSpec::Suffix { length: 0 };
    assert!(spec.to_bounds(1000).is_none());
}

// total_length=0 のケース
proptest! {
    #[test]
    fn range_spec_to_bounds_zero_total(start in 0u64..1000, end in 0u64..1000) {
        let (start, end) = if start <= end { (start, end) } else { (end, start) };

        let spec1 = RangeSpec::Range { start, end };
        let spec2 = RangeSpec::FromStart { start };
        let spec3 = RangeSpec::Suffix { length: 100 };

        prop_assert!(spec1.to_bounds(0).is_none());
        prop_assert!(spec2.to_bounds(0).is_none());
        prop_assert!(spec3.to_bounds(0).is_none());
    }
}

// RangeSpec Clone/Copy/PartialEq
proptest! {
    #[test]
    fn range_spec_clone_eq(start in 0u64..1000, end in 0u64..1000) {
        let (start, end) = if start <= end { (start, end) } else { (end, start) };
        let spec = RangeSpec::Range { start, end };
        let cloned = spec;  // Copy

        prop_assert_eq!(spec, cloned);
    }
}

// ========================================
// Range のテスト
// ========================================

// Range ヘッダーラウンドトリップ
proptest! {
    #[test]
    fn range_roundtrip(start in 0u64..10000, end in 0u64..10000) {
        // start <= end のみ有効
        let (start, end) = if start <= end {
            (start, end)
        } else {
            (end, start)
        };

        let input = format!("bytes={}-{}", start, end);
        let range = Range::parse(&input).unwrap();
        let displayed = range.to_string();
        let reparsed = Range::parse(&displayed).unwrap();

        prop_assert_eq!(range.unit(), reparsed.unit());
        prop_assert_eq!(range.ranges().len(), reparsed.ranges().len());
    }
}

// Range suffix ラウンドトリップ
proptest! {
    #[test]
    fn range_suffix_roundtrip(length in 1u64..10000) {
        let input = format!("bytes=-{}", length);
        let range = Range::parse(&input).unwrap();

        match range.first().unwrap() {
            RangeSpec::Suffix { length: l } => prop_assert_eq!(*l, length),
            _ => prop_assert!(false, "expected Suffix"),
        }
    }
}

// Range from-start ラウンドトリップ
proptest! {
    #[test]
    fn range_from_start_roundtrip(start in 0u64..10000) {
        let input = format!("bytes={}-", start);
        let range = Range::parse(&input).unwrap();

        match range.first().unwrap() {
            RangeSpec::FromStart { start: s } => prop_assert_eq!(*s, start),
            _ => prop_assert!(false, "expected FromStart"),
        }
    }
}

// RangeSpec to_bounds の正確性
proptest! {
    #[test]
    fn range_spec_to_bounds(start in 0u64..1000, end in 0u64..1000, total in 1u64..2000) {
        let (start, end) = if start <= end {
            (start, end)
        } else {
            (end, start)
        };
        let spec = RangeSpec::Range { start, end };

        if let Some((s, e)) = spec.to_bounds(total) {
            prop_assert!(s <= e);
            prop_assert!(e < total);
            prop_assert_eq!(s, start);
        }
    }
}

// Content-Range ラウンドトリップ
proptest! {
    #[test]
    fn content_range_roundtrip(start in 0u64..10000, end in 0u64..10000, total in 1u64..20000) {
        let (start, end) = if start <= end {
            (start, end)
        } else {
            (end, start)
        };
        let total = total.max(end + 1); // total は end より大きい必要がある

        let cr = ContentRange::new_bytes(start, end, Some(total));
        let displayed = cr.to_string();
        let reparsed = ContentRange::parse(&displayed).unwrap();

        prop_assert_eq!(cr.start(), reparsed.start());
        prop_assert_eq!(cr.end(), reparsed.end());
        prop_assert_eq!(cr.complete_length(), reparsed.complete_length());
    }
}

// Range::is_bytes のテスト
proptest! {
    #[test]
    fn range_is_bytes(start in 0u64..1000, end in 0u64..1000) {
        let (start, end) = if start <= end { (start, end) } else { (end, start) };

        // bytes の場合
        let input = format!("bytes={}-{}", start, end);
        let range = Range::parse(&input).unwrap();
        prop_assert!(range.is_bytes());

        // BYTES (大文字) の場合も true
        let input2 = format!("BYTES={}-{}", start, end);
        let range2 = Range::parse(&input2).unwrap();
        prop_assert!(range2.is_bytes());

        // 他の単位の場合は false
        let input3 = format!("custom={}-{}", start, end);
        let range3 = Range::parse(&input3).unwrap();
        prop_assert!(!range3.is_bytes());
    }
}

// Range::first のテスト
proptest! {
    #[test]
    fn range_first(start in 0u64..1000, end in 0u64..1000) {
        let (start, end) = if start <= end { (start, end) } else { (end, start) };

        let input = format!("bytes={}-{}", start, end);
        let range = Range::parse(&input).unwrap();

        prop_assert!(range.first().is_some());
        match range.first().unwrap() {
            RangeSpec::Range { start: s, end: e } => {
                prop_assert_eq!(*s, start);
                prop_assert_eq!(*e, end);
            }
            _ => prop_assert!(false, "expected Range"),
        }
    }
}

// 複数範囲のテスト
proptest! {
    #[test]
    fn range_multiple_ranges(
        start1 in 0u64..1000,
        end1 in 0u64..1000,
        start2 in 0u64..1000,
        end2 in 0u64..1000
    ) {
        let (start1, end1) = if start1 <= end1 { (start1, end1) } else { (end1, start1) };
        let (start2, end2) = if start2 <= end2 { (start2, end2) } else { (end2, start2) };

        let input = format!("bytes={}-{}, {}-{}", start1, end1, start2, end2);
        let range = Range::parse(&input).unwrap();

        prop_assert_eq!(range.ranges().len(), 2);
    }
}

// Range Clone/PartialEq
proptest! {
    #[test]
    fn range_clone_eq(start in 0u64..1000, end in 0u64..1000) {
        let (start, end) = if start <= end { (start, end) } else { (end, start) };

        let input = format!("bytes={}-{}", start, end);
        let range = Range::parse(&input).unwrap();
        let cloned = range.clone();

        prop_assert_eq!(range, cloned);
    }
}

// ========================================
// ContentRange のテスト
// ========================================

// ContentRange::length のテスト
proptest! {
    #[test]
    fn content_range_length(start in 0u64..10000, end in 0u64..10000) {
        let (start, end) = if start <= end { (start, end) } else { (end, start) };

        let cr = ContentRange::new_bytes(start, end, Some(end + 100));
        let length = cr.length();

        prop_assert!(length.is_some());
        prop_assert_eq!(length.unwrap(), end - start + 1);
    }
}

// ContentRange::is_unsatisfied のテスト
proptest! {
    #[test]
    fn content_range_is_unsatisfied(total in 100u64..10000) {
        // 満たせる場合
        let cr = ContentRange::new_bytes(0, 99, Some(total));
        prop_assert!(!cr.is_unsatisfied());

        // 満たせない場合
        let cr_unsatisfied = ContentRange::unsatisfied("bytes", total);
        prop_assert!(cr_unsatisfied.is_unsatisfied());
    }
}

// ContentRange::unsatisfied のテスト
proptest! {
    #[test]
    fn content_range_unsatisfied(total in 100u64..10000) {
        let cr = ContentRange::unsatisfied("bytes", total);

        prop_assert_eq!(cr.unit(), "bytes");
        prop_assert!(cr.start().is_none());
        prop_assert!(cr.end().is_none());
        prop_assert_eq!(cr.complete_length(), Some(total));
        prop_assert!(cr.is_unsatisfied());
        prop_assert!(cr.length().is_none());
    }
}

// ContentRange Display ラウンドトリップ (unsatisfied)
proptest! {
    #[test]
    fn content_range_unsatisfied_display_roundtrip(total in 100u64..10000) {
        let cr = ContentRange::unsatisfied("bytes", total);
        let displayed = cr.to_string();
        let reparsed = ContentRange::parse(&displayed).unwrap();

        prop_assert!(reparsed.is_unsatisfied());
        prop_assert_eq!(reparsed.complete_length(), Some(total));
    }
}

// ContentRange パース (不明な長さ)
proptest! {
    #[test]
    fn content_range_unknown_length(start in 0u64..10000, end in 0u64..10000) {
        let (start, end) = if start <= end { (start, end) } else { (end, start) };

        let input = format!("bytes {}-{}/*", start, end);
        let cr = ContentRange::parse(&input).unwrap();

        prop_assert_eq!(cr.start(), Some(start));
        prop_assert_eq!(cr.end(), Some(end));
        prop_assert!(cr.complete_length().is_none());
    }
}

// ContentRange Clone/PartialEq
proptest! {
    #[test]
    fn content_range_clone_eq(start in 0u64..1000, end in 0u64..1000, total in 0u64..2000) {
        let (start, end) = if start <= end { (start, end) } else { (end, start) };
        let total = total.max(end + 1);

        let cr = ContentRange::new_bytes(start, end, Some(total));
        let cloned = cr.clone();

        prop_assert_eq!(cr, cloned);
    }
}

// ========================================
// AcceptRanges のテスト
// ========================================

// AcceptRanges::bytes のテスト
#[test]
fn accept_ranges_bytes() {
    let ar = AcceptRanges::bytes();
    assert!(ar.accepts_bytes());
    assert!(!ar.is_none());
    assert_eq!(ar.units().len(), 1);
    assert_eq!(ar.units()[0], "bytes");
}

// AcceptRanges::none のテスト
#[test]
fn accept_ranges_none() {
    let ar = AcceptRanges::none();
    assert!(!ar.accepts_bytes());
    assert!(ar.is_none());
    assert_eq!(ar.units().len(), 1);
    assert_eq!(ar.units()[0], "none");
}

// AcceptRanges ラウンドトリップ
#[test]
fn accept_ranges_bytes_roundtrip() {
    let ar = AcceptRanges::bytes();
    let displayed = ar.to_string();
    let reparsed = AcceptRanges::parse(&displayed).unwrap();
    assert!(reparsed.accepts_bytes());
}

// AcceptRanges Display
#[test]
fn accept_ranges_display() {
    let ar = AcceptRanges::bytes();
    assert_eq!(ar.to_string(), "bytes");

    let ar2 = AcceptRanges::none();
    assert_eq!(ar2.to_string(), "none");
}

// AcceptRanges 複数単位
#[test]
fn accept_ranges_multiple_units() {
    let ar = AcceptRanges::parse("bytes, custom").unwrap();
    assert_eq!(ar.units().len(), 2);
    assert!(ar.accepts_bytes());
}

// AcceptRanges Clone/PartialEq
#[test]
fn accept_ranges_clone_eq() {
    let ar = AcceptRanges::bytes();
    let cloned = ar.clone();
    assert_eq!(ar, cloned);
}

// ========================================
// エラーケースのテスト
// ========================================

#[test]
fn range_parse_errors() {
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
fn content_range_parse_errors() {
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
fn accept_ranges_parse_errors() {
    // 空
    assert!(matches!(AcceptRanges::parse(""), Err(RangeError::Empty)));

    // 空白のみ
    assert!(matches!(AcceptRanges::parse("   "), Err(RangeError::Empty)));

    // カンマのみ
    assert!(matches!(AcceptRanges::parse(",,,"), Err(RangeError::Empty)));
}

// ========================================
// no_panic テスト
// ========================================

proptest! {
    #[test]
    fn range_parse_no_panic(s in "[ -~]{0,64}") {
        let _ = Range::parse(&s);
        let _ = ContentRange::parse(&s);
        let _ = AcceptRanges::parse(&s);
    }
}

// RangeSpec to_bounds は全ての入力でパニックしない
proptest! {
    #[test]
    fn range_spec_to_bounds_no_panic(
        start in 0u64..u64::MAX / 2,
        end in 0u64..u64::MAX / 2,
        length in 0u64..u64::MAX / 2,
        total in 0u64..u64::MAX / 2
    ) {
        let spec1 = RangeSpec::Range { start, end };
        let spec2 = RangeSpec::FromStart { start };
        let spec3 = RangeSpec::Suffix { length };

        let _ = spec1.to_bounds(total);
        let _ = spec2.to_bounds(total);
        let _ = spec3.to_bounds(total);
    }
}
