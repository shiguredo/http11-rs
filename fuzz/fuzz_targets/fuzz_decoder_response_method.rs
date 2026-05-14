//! `ResponseDecoder::set_request_method` の任意 method × 任意 status 組合せの
//! パニック安全性を検証する
//!
//! 既存 `fuzz_decoder_response` は `set_request_method` を `"HEAD"` /
//! `"CONNECT"` の 2 値固定でしか叩いていないため、以下の組合せが未到達:
//! - 空文字 / 制御文字 / 巨大 method の `set_request_method` 経路
//! - 任意 method × 1xx/204/304 等のボディ禁止 status の `decode_headers` 分岐
//! - 任意 method × `Content-Length` / `Transfer-Encoding: chunked` 同居経路
//!
//! 本 target は arbitrary で method を生成し、任意 status との組合せで
//! `decode_headers` → ボディ消費の全パスがパニックしないことを確認する。

#![no_main]

use arbitrary::Arbitrary;
use libfuzzer_sys::fuzz_target;
use shiguredo_http11::{BodyKind, BodyProgress, ResponseDecoder};

#[derive(Arbitrary, Debug)]
struct FuzzInput {
    /// 任意 method (空文字・制御文字・巨大文字列を含む)
    method: String,
    /// レスポンス本体 (ステータス行 + ヘッダー + ボディ想定の任意バイト列)
    data: Vec<u8>,
    /// データ分割サイズ (1..=64 で正規化)
    split_hint: u8,
}

fn drain(decoder: &mut ResponseDecoder, body_kind: BodyKind) {
    match body_kind {
        BodyKind::ContentLength(_) | BodyKind::Chunked | BodyKind::CloseDelimited => loop {
            if let Some(data) = decoder.peek_body() {
                let len = data.len();
                match decoder.consume_body(len) {
                    Ok(BodyProgress::Complete { .. }) => return,
                    Ok(BodyProgress::Advanced | BodyProgress::NeedData) => continue,
                    Err(_) => return,
                }
            }
            match decoder.progress() {
                Ok(BodyProgress::Complete { .. }) => return,
                Ok(BodyProgress::Advanced) => continue,
                Ok(BodyProgress::NeedData) | Err(_) => return,
            }
        },
        BodyKind::None | BodyKind::Tunnel => {}
        _ => {}
    }
}

fuzz_target!(|input: FuzzInput| {
    let FuzzInput {
        method,
        data,
        split_hint,
    } = input;
    let split_size = ((split_hint as usize) % 64).max(1);

    // パターン 1: 一括 feed
    let mut decoder = ResponseDecoder::new();
    decoder.set_request_method(&method);
    if decoder.feed(&data).is_ok()
        && let Ok(Some((_, body_kind))) = decoder.decode_headers()
    {
        let _ = decoder.is_tunnel();
        let _ = decoder.is_close_delimited();
        drain(&mut decoder, body_kind);
        decoder.mark_eof();
        let _ = decoder.take_remaining();
    }

    // パターン 2: 分割 feed
    decoder.reset();
    decoder.set_request_method(&method);
    for chunk in data.chunks(split_size) {
        if decoder.feed(chunk).is_err() {
            break;
        }
        if let Ok(Some((_, body_kind))) = decoder.decode_headers() {
            let _ = decoder.is_tunnel();
            drain(&mut decoder, body_kind);
            break;
        }
    }

    // パターン 3: reset 後に method を変えて再投入 (Keep-Alive シナリオ)
    decoder.reset();
    decoder.set_request_method("GET");
    let _ = decoder.feed(&data);
    let _ = decoder.decode_headers();
    decoder.reset();
    decoder.set_request_method(&method);
    let _ = decoder.feed(&data);
    if let Ok(Some((_, body_kind))) = decoder.decode_headers() {
        drain(&mut decoder, body_kind);
    }
});
