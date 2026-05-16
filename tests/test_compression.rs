//! 圧縮/展開トレイトのユニットテスト

use shiguredo_http11::compression::{
    CompressionError, CompressionStatus, Compressor, Decompressor, NoCompression,
};

/// NoCompression::compress で Continue ステータスを取得する
fn make_continue() -> CompressionStatus {
    let mut comp = NoCompression::new();
    let mut output = vec![0u8; 32];
    // input <= output なので Continue が返る
    comp.compress(b"abcdefghij", &mut output).unwrap()
}

/// NoCompression::compress で OutputFull ステータスを取得する
fn make_output_full() -> CompressionStatus {
    let mut comp = NoCompression::new();
    let mut output = vec![0u8; 7];
    // input > output なので OutputFull が返る
    comp.compress(b"abcdefghij", &mut output).unwrap()
}

/// NoCompression::finish で Complete ステータスを取得する
fn make_complete() -> CompressionStatus {
    let mut comp = NoCompression::new();
    let mut output = vec![0u8; 32];
    comp.finish(&mut output).unwrap()
}

#[test]
fn test_compression_status_consumed() {
    let cont = make_continue();
    assert!(matches!(cont, CompressionStatus::Continue { .. }));
    assert_eq!(cont.consumed(), 10);

    let full = make_output_full();
    assert!(matches!(full, CompressionStatus::OutputFull { .. }));
    assert_eq!(full.consumed(), 7);

    let complete = make_complete();
    assert!(matches!(complete, CompressionStatus::Complete { .. }));
    assert_eq!(complete.consumed(), 0);
}

#[test]
fn test_compression_status_produced() {
    let cont = make_continue();
    assert_eq!(cont.produced(), 10);

    let full = make_output_full();
    assert_eq!(full.produced(), 7);

    let complete = make_complete();
    assert_eq!(complete.produced(), 0);
}

#[test]
fn test_compression_status_is_complete() {
    assert!(!make_continue().is_complete());
    assert!(make_complete().is_complete());
    assert!(!make_output_full().is_complete());
}

#[test]
fn test_compression_status_is_output_full() {
    assert!(!make_continue().is_output_full());
    assert!(!make_complete().is_output_full());
    assert!(make_output_full().is_output_full());
}

#[test]
fn test_no_compression_compress() {
    let mut comp = NoCompression::new();
    let input = b"Hello, World!";
    let mut output = vec![0u8; 32];

    let status = comp.compress(input, &mut output).unwrap();
    assert_eq!(status.consumed(), 13);
    assert_eq!(status.produced(), 13);
    assert_eq!(&output[..13], input);
}

#[test]
fn test_no_compression_compress_output_full() {
    let mut comp = NoCompression::new();
    let input = b"Hello, World!";
    let mut output = vec![0u8; 5];

    let status = comp.compress(input, &mut output).unwrap();
    assert!(status.is_output_full());
    assert_eq!(status.consumed(), 5);
    assert_eq!(status.produced(), 5);
    assert_eq!(&output[..5], b"Hello");
}

#[test]
fn test_no_compression_finish() {
    let mut comp = NoCompression::new();
    let mut output = vec![0u8; 32];

    let status = comp.finish(&mut output).unwrap();
    assert!(status.is_complete());
    assert_eq!(status.consumed(), 0);
    assert_eq!(status.produced(), 0);
}

#[test]
fn test_no_compression_already_finished() {
    let mut comp = NoCompression::new();
    let mut output = vec![0u8; 32];

    comp.finish(&mut output).unwrap();
    assert_eq!(
        comp.finish(&mut output).unwrap_err(),
        CompressionError::AlreadyFinished
    );
    assert_eq!(
        comp.compress(b"test", &mut output).unwrap_err(),
        CompressionError::AlreadyFinished
    );
}

#[test]
fn test_no_compression_reset_compressor() {
    let mut comp = NoCompression::new();
    let mut output = vec![0u8; 32];

    comp.finish(&mut output).unwrap();
    Compressor::reset(&mut comp);

    // リセット後は再度使用可能
    let status = comp.compress(b"test", &mut output).unwrap();
    assert_eq!(status.consumed(), 4);
}

#[test]
fn test_no_compression_reset_decompressor() {
    let mut decomp = NoCompression::new();

    Decompressor::reset(&mut decomp);

    // リセット後も使用可能
    let mut output = vec![0u8; 32];
    let status = decomp.decompress(b"test", &mut output).unwrap();
    assert_eq!(status.consumed(), 4);
}

#[test]
fn test_no_compression_decompress() {
    let mut decomp = NoCompression::new();
    let input = b"Hello, World!";
    let mut output = vec![0u8; 32];

    let status = decomp.decompress(input, &mut output).unwrap();
    assert_eq!(status.consumed(), 13);
    assert_eq!(status.produced(), 13);
    assert_eq!(&output[..13], input);
}

#[test]
fn test_no_compression_decompress_output_full() {
    let mut decomp = NoCompression::new();
    let input = b"Hello, World!";
    let mut output = vec![0u8; 5];

    let status = decomp.decompress(input, &mut output).unwrap();
    assert!(status.is_output_full());
    assert_eq!(status.consumed(), 5);
    assert_eq!(status.produced(), 5);
}

#[test]
fn test_no_compression_decompress_complete() {
    let mut decomp = NoCompression::new();
    let mut output = vec![0u8; 32];

    // 空入力で Complete を返す
    let status = decomp.decompress(&[], &mut output).unwrap();
    assert!(status.is_complete());
}

#[test]
fn test_compression_error_display() {
    assert_eq!(
        CompressionError::BufferTooSmall {
            required: 100,
            available: 50
        }
        .to_string(),
        "buffer too small: required 100 bytes, available 50 bytes"
    );
    assert_eq!(
        CompressionError::InvalidData("bad data".to_string()).to_string(),
        "invalid data: bad data"
    );
    assert_eq!(
        CompressionError::Internal("oops".to_string()).to_string(),
        "internal error: oops"
    );
    assert_eq!(
        CompressionError::UnexpectedEof.to_string(),
        "unexpected end of input"
    );
    assert_eq!(
        CompressionError::AlreadyFinished.to_string(),
        "compression already finished"
    );
}
