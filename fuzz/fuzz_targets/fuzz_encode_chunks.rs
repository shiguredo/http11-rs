//! `encode_chunk` / `encode_chunks` の直接 panic 安全性を検証する
//!
//! 検証対象:
//! - 任意の `Vec<Vec<u8>>` を `encode_chunks` / `encode_chunk` 反復に流して
//!   panic / abort しないこと

#![no_main]

use arbitrary::Arbitrary;
use libfuzzer_sys::fuzz_target;
use shiguredo_http11::{encode_chunk, encode_chunks};

#[derive(Arbitrary, Debug)]
struct FuzzInput {
    chunks: Vec<Vec<u8>>,
}

/// chunks の合計サイズと個数を制限する (OOM 回避)
fn normalize(mut chunks: Vec<Vec<u8>>) -> Vec<Vec<u8>> {
    if chunks.len() > 256 {
        chunks.truncate(256);
    }
    let mut total: usize = 0;
    chunks.retain_mut(|chunk| {
        if chunk.len() > 64 * 1024 {
            chunk.truncate(64 * 1024);
        }
        let next = match total.checked_add(chunk.len()) {
            Some(v) => v,
            None => return false,
        };
        if next > 4 * 1024 * 1024 {
            return false;
        }
        total = next;
        true
    });
    chunks
}

fuzz_target!(|input: FuzzInput| {
    let chunks = normalize(input.chunks);

    let refs: Vec<&[u8]> = chunks.iter().map(|c| c.as_slice()).collect();
    let _ = encode_chunks(&refs);
    for chunk in &chunks {
        let _ = encode_chunk(chunk);
    }
    let _ = encode_chunk(&[]);
});
