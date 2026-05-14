//! `encode_chunk` / `encode_chunks` の直接 panic / 容量見積もり安全性を検証する
//!
//! 既存 `fuzz_decoder_chunked` は decode 経路を主目的としており、
//! `encode_chunks` を経由するが `Vec<&[u8]>` 全長や個数を任意に流す target が無い。
//! `encode_chunks_capacity` (`src/encoder.rs:932`) は `checked_add` で wrap を
//! 防いでいるが、`Vec::with_capacity` や `extend_from_slice` 自体の OOM
//! / panic 経路は別途確認が要る。
//!
//! 検証対象:
//! - 任意の `Vec<Vec<u8>>` を `encode_chunks` / `encode_chunk` 反復に流して
//!   panic / abort しないこと
//! - `encode_chunks(refs)` の出力が
//!   「各 chunk への `encode_chunk` を順次連結 + 終端 `encode_chunk(&[])`」
//!   と完全に等価であること (バイト単位)
//! - 同じ入力を複数回 encode して結果が等しいこと (決定性)

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

    // `encode_chunks` 一括版
    let refs: Vec<&[u8]> = chunks.iter().map(|c| c.as_slice()).collect();
    let bulk = encode_chunks(&refs);

    // `encode_chunk` 反復版 (空チャンクは中間に書かないのが慣例だが、
    // `encode_chunks(&[])` は 5 バイトの終端のみを生成する仕様なので
    // ここでも空 chunks のときは終端のみを期待値とする)
    let stepwise = if chunks.is_empty() {
        encode_chunk(&[])
    } else {
        let mut buf = Vec::new();
        for chunk in &chunks {
            buf.extend_from_slice(&encode_chunk(chunk));
        }
        buf.extend_from_slice(&encode_chunk(&[]));
        buf
    };

    // バイト単位の等価性
    assert_eq!(bulk, stepwise, "encode_chunks must equal sequential encode_chunk + terminator");

    // 決定性 (同じ入力で 2 回呼んで結果が変わらない)
    let bulk_again = encode_chunks(&refs);
    assert_eq!(bulk, bulk_again, "encode_chunks must be deterministic");

    // 終端チャンクで必ず終わる
    assert!(
        bulk.ends_with(b"0\r\n\r\n"),
        "encode_chunks output must end with 0\\r\\n\\r\\n terminator"
    );
});
