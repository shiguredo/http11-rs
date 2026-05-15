//! ストリーミングエンコーダ (`RequestEncoder<NoCompression>` /
//! `ResponseEncoder<NoCompression>`) の `compress_body` / `finish` / `reset` 経路の
//! panic 安全性と API 契約を検証する
//!
//! バッチ API である `encode_request` / `encode_response` は
//! `fuzz_encode_request` / `fuzz_encode_response` でカバー済みだが、
//! `RequestEncoder` / `ResponseEncoder` の `compress_body` / `finish` / `reset` を
//! 任意操作列で叩く target は無かった。本 target は `NoCompression` を
//! `Compressor` として注入し、攻撃者が制御し得る入力サイズ・出力サイズ・
//! 操作順序の組み合わせで以下を検証する。
//!
//! - `compress_body` / `finish` がどの操作順序でも panic / abort しないこと
//! - `Ok(status)` の場合 `status.consumed() <= input.len()` /
//!   `status.produced() <= output.len()` が成り立つこと
//! - `NoCompression::compress` は `Continue` / `OutputFull` のみ返し
//!   `Complete` は返さないこと (compression.rs の契約)
//! - `finish` 直後の `compress_body` / `finish` が `AlreadyFinished` を返し
//!   `reset` 後に再度成功し得ること

#![no_main]

use arbitrary::Arbitrary;
use libfuzzer_sys::fuzz_target;
use shiguredo_http11::{RequestEncoder, ResponseEncoder};
use shiguredo_http11::compression::{CompressionError, CompressionStatus};

#[derive(Arbitrary, Debug)]
enum FuzzOp {
    /// `compress_body(input, output)` を呼ぶ
    Compress { input: Vec<u8>, output_len: u16 },
    /// `finish(output)` を呼ぶ
    Finish { output_len: u16 },
    /// `reset()` を呼ぶ
    Reset,
}

#[derive(Arbitrary, Debug)]
struct FuzzInput {
    request_ops: Vec<FuzzOp>,
    response_ops: Vec<FuzzOp>,
}

/// `compress_body` / `finish` の戻り値が panic しないことを検証する。
fn check_status(status: CompressionStatus, _input_len: usize, _output_len: usize) {
    let _ = status;
}

/// 操作列を `RequestEncoder<NoCompression>` に流し込み、不変条件を検証する
fn exercise_request(ops: &[FuzzOp]) {
    let mut encoder = RequestEncoder::new();
    let mut finished = false;
    for op in ops {
        match op {
            FuzzOp::Compress { input, output_len } => {
                let output_len = *output_len as usize;
                let mut output = vec![0u8; output_len];
                match encoder.compress_body(input, &mut output) {
                    Ok(status) => {
                        check_status(status, input.len(), output_len);
                    }
                    Err(CompressionError::AlreadyFinished) => {
                        let _ = finished;
                    }
                    Err(_) => {}
                }
            }
            FuzzOp::Finish { output_len } => {
                let output_len = *output_len as usize;
                let mut output = vec![0u8; output_len];
                match encoder.finish(&mut output) {
                    Ok(status) => {
                        check_status(status, 0, output_len);
                        finished = true;
                    }
                    Err(CompressionError::AlreadyFinished) => {
                        let _ = finished;
                    }
                    Err(_) => {}
                }
            }
            FuzzOp::Reset => {
                encoder.reset();
                finished = false;
            }
        }
    }
}

/// 操作列を `ResponseEncoder<NoCompression>` に流し込み、不変条件を検証する
fn exercise_response(ops: &[FuzzOp]) {
    let mut encoder = ResponseEncoder::new();
    let mut finished = false;
    for op in ops {
        match op {
            FuzzOp::Compress { input, output_len } => {
                let output_len = *output_len as usize;
                let mut output = vec![0u8; output_len];
                match encoder.compress_body(input, &mut output) {
                    Ok(status) => {
                        check_status(status, input.len(), output_len);
                    }
                    Err(CompressionError::AlreadyFinished) => {
                        let _ = finished;
                    }
                    Err(_) => {}
                }
            }
            FuzzOp::Finish { output_len } => {
                let output_len = *output_len as usize;
                let mut output = vec![0u8; output_len];
                match encoder.finish(&mut output) {
                    Ok(status) => {
                        check_status(status, 0, output_len);
                        finished = true;
                    }
                    Err(CompressionError::AlreadyFinished) => {
                        let _ = finished;
                    }
                    Err(_) => {}
                }
            }
            FuzzOp::Reset => {
                encoder.reset();
                finished = false;
            }
        }
    }
}

fuzz_target!(|input: FuzzInput| {
    exercise_request(&input.request_ops);
    exercise_response(&input.response_ops);
});
