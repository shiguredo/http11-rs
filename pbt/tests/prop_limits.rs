//! DecoderLimits 構造体のプロパティテスト (limits.rs)

use proptest::prelude::*;
use shiguredo_http11::DecoderLimits;

// ========================================
// DecoderLimits 構造体のテスト
// ========================================

// Default のプロパティ: 各フィールドが期待値を持つ
#[test]
fn decoder_limits_default_values() {
    let limits = DecoderLimits::default();

    assert_eq!(limits.max_buffer_size, 64 * 1024); // 64KB
    assert_eq!(limits.max_headers_count, 100);
    assert_eq!(limits.max_header_line_size, 8 * 1024); // 8KB
    assert_eq!(limits.max_body_size, 10 * 1024 * 1024); // 10MB
    assert_eq!(limits.max_chunk_line_size, 64); // 64 bytes
}

// unlimited のプロパティ: 各フィールドが usize::MAX
#[test]
fn decoder_limits_unlimited_values() {
    let limits = DecoderLimits::unlimited();

    assert_eq!(limits.max_buffer_size, usize::MAX);
    assert_eq!(limits.max_headers_count, usize::MAX);
    assert_eq!(limits.max_header_line_size, usize::MAX);
    assert_eq!(limits.max_body_size, usize::MAX);
    assert_eq!(limits.max_chunk_line_size, usize::MAX);
}

// Clone のプロパティ: クローンが元と等しい
proptest! {
    #[test]
    fn decoder_limits_clone_eq(
        max_buffer_size in 1usize..1_000_000,
        max_headers_count in 1usize..1000,
        max_header_line_size in 1usize..100_000,
        max_body_size in 1usize..100_000_000,
        max_chunk_line_size in 1usize..1000
    ) {
        let limits = DecoderLimits {
            max_buffer_size,
            max_headers_count,
            max_header_line_size,
            max_body_size,
            max_chunk_line_size,
        };

        let cloned = limits.clone();
        prop_assert_eq!(limits, cloned);
    }
}

// PartialEq のプロパティ
proptest! {
    #[test]
    fn decoder_limits_partial_eq(
        max_buffer_size in 1usize..1_000_000,
        max_headers_count in 1usize..1000
    ) {
        let limits1 = DecoderLimits {
            max_buffer_size,
            max_headers_count,
            ..DecoderLimits::default()
        };

        let limits2 = DecoderLimits {
            max_buffer_size,
            max_headers_count,
            ..DecoderLimits::default()
        };

        let limits3 = DecoderLimits {
            max_buffer_size: max_buffer_size + 1,
            max_headers_count,
            ..DecoderLimits::default()
        };

        prop_assert_eq!(&limits1, &limits2);
        prop_assert_ne!(&limits1, &limits3);
    }
}

// Debug trait のテスト
#[test]
fn decoder_limits_debug() {
    let limits = DecoderLimits::default();
    let debug_str = format!("{:?}", limits);

    assert!(debug_str.contains("DecoderLimits"));
    assert!(debug_str.contains("max_buffer_size"));
    assert!(debug_str.contains("max_headers_count"));
    assert!(debug_str.contains("max_header_line_size"));
    assert!(debug_str.contains("max_body_size"));
    assert!(debug_str.contains("max_chunk_line_size"));
}
