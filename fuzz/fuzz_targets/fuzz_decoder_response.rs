//! ResponseDecoder の任意バイト列に対するパニック安全性を検証する
//!
//! - 通常デコード: 任意のバイト列を ResponseDecoder に投入し、
//!   ヘッダーデコード → ボディ消費の全パスでパニックしないことを確認する
//! - HEAD レスポンス: set_request_method("HEAD") でのデコードパスを検証する
//! - CONNECT トンネル: set_request_method("CONNECT") で 2xx 応答時の
//!   Tunnel モード遷移パスを検証する
//! - ストリーミング feed: 同じデータを 23 バイト単位に分割して段階的にデコードする
//! - 直接書き込み API: mut_buf / advance_buf 経由でも同じ全パスがパニックしないことを確認する
//! - feed_unchecked 経路: max_buffer_size チェックをスキップする経路でもパニックしないことを確認する
//! - mark_eof / is_close_delimited のパス網羅

#![no_main]

use libfuzzer_sys::fuzz_target;
use shiguredo_http11::{BodyKind, BodyProgress, ResponseDecoder};

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

fuzz_target!(|data: &[u8]| {
    // 通常のレスポンスデコード
    let mut decoder = ResponseDecoder::new();
    if decoder.feed(data).is_ok()
        && let Ok(Some((_, body_kind))) = decoder.decode_headers()
    {
        let _ = decoder.is_tunnel();
        let _ = decoder.is_close_delimited();
        let _ = decoder.available_buf();
        let _ = decoder.remaining();
        drain(&mut decoder, body_kind);
        // close-delimited なら mark_eof で Complete に遷移させる
        decoder.mark_eof();
        let _ = decoder.is_close_delimited();
        let _ = decoder.take_remaining();
    }

    // HEAD リクエストへのレスポンスとしてデコード
    decoder.reset();
    decoder.set_request_method("HEAD");
    if decoder.feed(data).is_ok()
        && let Ok(Some((_, body_kind))) = decoder.decode_headers()
    {
        drain(&mut decoder, body_kind);
    }

    // CONNECT リクエストへのレスポンスとしてデコード (Tunnel モード経路)
    decoder.reset();
    decoder.set_request_method("CONNECT");
    if decoder.feed(data).is_ok()
        && let Ok(Some((_, body_kind))) = decoder.decode_headers()
    {
        let _ = decoder.is_tunnel();
        drain(&mut decoder, body_kind);
    }

    // データを分割して feed (ストリーミングシナリオ)
    decoder.reset();
    for chunk in data.chunks(23) {
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
    for chunk in data.chunks(29) {
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
