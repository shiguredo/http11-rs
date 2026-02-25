//! DecoderLimits のユニットテスト

use shiguredo_http11::DecoderLimits;

// ========================================
// DecoderLimits 構造体のテスト
// ========================================

// Default のプロパティ: 各フィールドが期待値を持つ
#[test]
fn test_decoder_limits_default_values() {
    let limits = DecoderLimits::default();

    assert_eq!(limits.max_buffer_size, 64 * 1024); // 64KB
    assert_eq!(limits.max_headers_count, 100);
    assert_eq!(limits.max_header_line_size, 8 * 1024); // 8KB
    assert_eq!(limits.max_body_size, 10 * 1024 * 1024); // 10MB
    assert_eq!(limits.max_chunk_line_size, 64); // 64 bytes
}

// unlimited のプロパティ: 各フィールドが usize::MAX
#[test]
fn test_decoder_limits_unlimited_values() {
    let limits = DecoderLimits::unlimited();

    assert_eq!(limits.max_buffer_size, usize::MAX);
    assert_eq!(limits.max_headers_count, usize::MAX);
    assert_eq!(limits.max_header_line_size, usize::MAX);
    assert_eq!(limits.max_body_size, usize::MAX);
    assert_eq!(limits.max_chunk_line_size, usize::MAX);
}
