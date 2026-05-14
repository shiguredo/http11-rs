//! `RequestDecoder::peek_body_decompressed` / `ResponseDecoder::peek_body_decompressed`
//! の任意操作列に対する panic 安全性を検証する
//!
//! `fuzz_streaming_encoder` は **encoder 側** の `Compressor` (NoCompression) を
//! fuzz するが、decoder 側の `peek_body_decompressed` 経路 (`Decompressor` 注入 +
//! 任意 output サイズ + Continue/OutputFull/Complete 状態機械) は未到達だった。
//!
//! 本 target は `NoCompression` を `Decompressor` として注入し、以下を検証する:
//! - 任意バイト列に対する `decode_headers` → `peek_body_decompressed` →
//!   `consume_body(status.consumed())` のループが panic / abort しないこと
//! - 任意 output buffer サイズ (0 含む) でも API 契約が壊れないこと
//! - `BodyKind` (ContentLength / Chunked / CloseDelimited / None / Tunnel) の
//!   全分岐で安全に動作すること
//! - reset 後の再利用でも panic しないこと

#![no_main]

use arbitrary::Arbitrary;
use libfuzzer_sys::fuzz_target;
use shiguredo_http11::compression::{CompressionStatus, NoCompression};
use shiguredo_http11::{BodyKind, BodyProgress, RequestDecoder, ResponseDecoder};

#[derive(Arbitrary, Debug)]
struct FuzzInput {
    /// 投入するバイト列
    data: Vec<u8>,
    /// `peek_body_decompressed` に渡す output buffer サイズ (0..=4096 で正規化)
    output_size_hint: u16,
    /// CONNECT トンネル経路を踏むかどうか (ResponseDecoder 専用)
    use_connect: bool,
    /// 反復回数の上限 (1..=32 で正規化)
    iter_hint: u8,
}

fn drive_request(decoder: &mut RequestDecoder<NoCompression>, output_size: usize, iter_max: usize) {
    let body_kind = match decoder.decode_headers() {
        Ok(Some((_, kind))) => kind,
        _ => return,
    };
    if matches!(body_kind, BodyKind::None | BodyKind::Tunnel) {
        return;
    }
    let mut output = vec![0u8; output_size];
    for _ in 0..iter_max {
        match decoder.peek_body_decompressed(&mut output) {
            Ok(Some(status)) => {
                let consumed = status.consumed();
                let produced = status.produced();
                assert!(
                    produced <= output.len(),
                    "produced {} must not exceed output {}",
                    produced,
                    output.len()
                );
                if consumed > 0 {
                    if decoder.consume_body(consumed).is_err() {
                        return;
                    }
                } else if matches!(status, CompressionStatus::OutputFull { .. }) {
                    // output が小さすぎて何も進まない: ループを抜ける
                    return;
                } else {
                    // 進展なし: progress() で状態遷移を試みる
                    match decoder.progress() {
                        Ok(BodyProgress::Complete { .. }) => return,
                        Ok(BodyProgress::Advanced) => continue,
                        Ok(BodyProgress::NeedData) | Err(_) => return,
                    }
                }
            }
            Ok(None) => return,
            Err(_) => return,
        }
    }
}

fn drive_response(
    decoder: &mut ResponseDecoder<NoCompression>,
    output_size: usize,
    iter_max: usize,
) {
    let body_kind = match decoder.decode_headers() {
        Ok(Some((_, kind))) => kind,
        _ => return,
    };
    if matches!(body_kind, BodyKind::None | BodyKind::Tunnel) {
        return;
    }
    let mut output = vec![0u8; output_size];
    for _ in 0..iter_max {
        match decoder.peek_body_decompressed(&mut output) {
            Ok(Some(status)) => {
                let consumed = status.consumed();
                let produced = status.produced();
                assert!(
                    produced <= output.len(),
                    "produced {} must not exceed output {}",
                    produced,
                    output.len()
                );
                if consumed > 0 {
                    if decoder.consume_body(consumed).is_err() {
                        return;
                    }
                } else if matches!(status, CompressionStatus::OutputFull { .. }) {
                    return;
                } else {
                    match decoder.progress() {
                        Ok(BodyProgress::Complete { .. }) => return,
                        Ok(BodyProgress::Advanced) => continue,
                        Ok(BodyProgress::NeedData) | Err(_) => return,
                    }
                }
            }
            Ok(None) => return,
            Err(_) => return,
        }
    }
}

fuzz_target!(|input: FuzzInput| {
    let FuzzInput {
        data,
        output_size_hint,
        use_connect,
        iter_hint,
    } = input;
    let output_size = (output_size_hint as usize) % 4097;
    let iter_max = ((iter_hint as usize) % 32).max(1);

    // RequestDecoder 経路
    let mut request_decoder = RequestDecoder::with_decompressor(NoCompression::new());
    if request_decoder.feed(&data).is_ok() {
        drive_request(&mut request_decoder, output_size, iter_max);
    }
    request_decoder.reset();
    // reset 後の再利用が壊れないこと
    if request_decoder.feed(&data).is_ok() {
        drive_request(&mut request_decoder, output_size, iter_max);
    }

    // ResponseDecoder 経路
    let mut response_decoder = ResponseDecoder::with_decompressor(NoCompression::new());
    if use_connect {
        response_decoder.set_request_method("CONNECT");
    }
    if response_decoder.feed(&data).is_ok() {
        drive_response(&mut response_decoder, output_size, iter_max);
    }
    response_decoder.reset();
    if response_decoder.feed(&data).is_ok() {
        drive_response(&mut response_decoder, output_size, iter_max);
    }
});
