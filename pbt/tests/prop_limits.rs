//! DecoderLimits 構造体のプロパティテスト (limits.rs)

use proptest::prelude::*;
use shiguredo_http11::DecoderLimits;

// ========================================
// DecoderLimits 構造体のテスト
// ========================================

// Clone のプロパティ: クローンが元と等しい
proptest! {
    #[test]
    fn prop_decoder_limits_clone_eq(
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
    fn prop_decoder_limits_partial_eq(
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
