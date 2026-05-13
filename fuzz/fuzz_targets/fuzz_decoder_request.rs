//! RequestDecoder の任意バイト列に対するパニック安全性を検証する
//!
//! - 一括 feed: 任意のバイト列をそのまま RequestDecoder に投入し、
//!   ヘッダーデコード → ボディ消費の全パスでパニックしないことを確認する
//! - ストリーミング feed: 同じデータを 17 バイト単位に分割して投入し、
//!   段階的なデコードでもパニックしないことを確認する
//! - 直接書き込み API: mut_buf / advance_buf 経由でも同じ全パスがパニックしないことを確認する
//! - feed_unchecked 経路: max_buffer_size チェックをスキップする経路でもパニックしないことを確認する
//! - progress / is_tunnel / take_remaining / available_buf / remaining のアクセサ網羅

#![no_main]

use libfuzzer_sys::fuzz_target;
use shiguredo_http11::{BodyKind, BodyProgress, RequestDecoder};

fn drain(decoder: &mut RequestDecoder, body_kind: BodyKind) {
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
            // peek_body が None でも progress() で状態遷移を試みる
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

fuzz_target!(|data: &[u8]| {
    let mut decoder = RequestDecoder::new();

    // データを一度に feed
    if decoder.feed(data).is_ok()
        && let Ok(Some((_, body_kind))) = decoder.decode_headers()
    {
        let _ = decoder.is_tunnel();
        let _ = decoder.available_buf();
        let _ = decoder.remaining();
        drain(&mut decoder, body_kind);
        // ボディ消費後の残データを取得 (パイプライン化想定)
        let _ = decoder.take_remaining();
    }

    // データを分割して feed (ストリーミングシナリオ)
    decoder.reset();
    for chunk in data.chunks(17) {
        if decoder.feed(chunk).is_err() {
            break;
        }
        if let Ok(Some((_, body_kind))) = decoder.decode_headers() {
            drain(&mut decoder, body_kind);
            break;
        }
    }

    // mut_buf / advance_buf 経由で投入するシナリオ
    decoder.reset();
    for chunk in data.chunks(19) {
        let dst = match decoder.mut_buf(chunk.len()) {
            Ok(dst) => dst,
            Err(_) => return,
        };
        dst.copy_from_slice(chunk);
        decoder.advance_buf(chunk.len());
        if let Ok(Some((_, body_kind))) = decoder.decode_headers() {
            drain(&mut decoder, body_kind);
            break;
        }
    }

    // feed_unchecked 経路 (max_buffer_size をスキップ)
    decoder.reset();
    decoder.feed_unchecked(data);
    if let Ok(Some((_, body_kind))) = decoder.decode_headers() {
        drain(&mut decoder, body_kind);
    }
});
