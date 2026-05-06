//! mut_buf / advance_buf に任意の usize を渡してもパニックしないことを検証する
//!
//! 通常の `fuzz_decoder_request` / `fuzz_decoder_response` は `chunk.len()`
//! 経由で実用範囲のサイズしか `mut_buf` に渡さないため、
//! `buf.len() + len` の整数オーバーフロー (`checked_add` 防御) のような
//! 極端なエッジケースを踏まない。本 fuzz target は `usize` 全域を `len` に
//! 流し込み、API としての堅牢性を検証する。
//!
//! 検証対象:
//! - `mut_buf(usize::MAX)` 等の極端な len で panic しない (BufferOverflow を返す)
//! - 既存データが入った状態 (buf.len() > 0) で大きな len を渡し、
//!   `buf.len() + len` の usize オーバーフロー経路を踏んでも panic しない
//! - 連続した `mut_buf` / `advance_buf` 呼び出しで内部状態が壊れない

#![no_main]

use arbitrary::Arbitrary;
use libfuzzer_sys::fuzz_target;
use shiguredo_http11::{DecoderLimits, RequestDecoder, ResponseDecoder};

#[derive(Arbitrary, Debug)]
struct FuzzOp {
    /// `mut_buf` に渡す `len` (`usize` 全域)
    len: usize,
    /// `mut_buf` 成功時に書き込むバイト列の先頭部分
    bytes: Vec<u8>,
    /// `advance_buf` に渡す値 (`pending = len` で clamp する)
    advance: usize,
}

#[derive(Arbitrary, Debug)]
struct FuzzInput {
    /// `max_buffer_size` の元値 (OOM 回避のため 1MB で clamp する)
    max_buffer_size: u32,
    operations: Vec<FuzzOp>,
}

fuzz_target!(|input: FuzzInput| {
    // 任意の `mut_buf(len)` が成功した場合の `Vec::resize` による OOM を避けるため、
    // 上限を 1MB に固定する。`checked_add` 経路 (`buf.len() + len` の usize
    // オーバーフロー) は max_buffer_size の値と独立に踏めるので、これで
    // テスト目的を損なわない。
    let max_buffer_size = (input.max_buffer_size as usize).min(1 << 20);
    let limits = DecoderLimits {
        max_buffer_size,
        ..Default::default()
    };

    let mut request = RequestDecoder::with_limits(limits.clone());
    for op in &input.operations {
        if let Ok(dst) = request.mut_buf(op.len) {
            let n = op.bytes.len().min(dst.len());
            dst[..n].copy_from_slice(&op.bytes[..n]);
            // advance は pending (= op.len) で clamp して debug_assert! 違反を回避する
            request.advance_buf(op.advance.min(op.len));
        }
    }
    // pending == 0 を満たした状態で他の API も呼んで panic しないことを確認する
    let _ = request.decode_headers();

    let mut response = ResponseDecoder::with_limits(limits);
    for op in &input.operations {
        if let Ok(dst) = response.mut_buf(op.len) {
            let n = op.bytes.len().min(dst.len());
            dst[..n].copy_from_slice(&op.bytes[..n]);
            response.advance_buf(op.advance.min(op.len));
        }
    }
    let _ = response.decode_headers();
});
