//! ResponseDecoder の任意バイト列に対するパニック安全性を検証する
//!
//! - 通常デコード: 任意のバイト列を ResponseDecoder に投入し、
//!   ヘッダーデコード → ボディ消費の全パスでパニックしないことを確認する
//! - HEAD レスポンス: set_expect_no_body(true) でのデコードパスを検証する
//! - ストリーミング feed: 同じデータを 23 バイト単位に分割して段階的にデコードする

#![no_main]

use libfuzzer_sys::fuzz_target;
use shiguredo_http11::{BodyKind, BodyProgress, ResponseDecoder};

fuzz_target!(|data: &[u8]| {
    // 通常のレスポンスデコード
    let mut decoder = ResponseDecoder::new();
    if decoder.feed(data).is_ok()
        && let Ok(Some((_, body_kind))) = decoder.decode_headers()
    {
        match body_kind {
            BodyKind::ContentLength(_) | BodyKind::Chunked | BodyKind::CloseDelimited => {
                while let Some(body_data) = decoder.peek_body() {
                    let len = body_data.len();
                    match decoder.consume_body(len) {
                        Ok(BodyProgress::Complete { .. }) => break,
                        Ok(BodyProgress::Continue) => {}
                        Err(_) => break,
                    }
                }
            }
            BodyKind::None | BodyKind::Tunnel => {}
        }
    }

    // HEAD リクエストへのレスポンスとしてデコード
    decoder.reset();
    decoder.set_expect_no_body(true);
    if decoder.feed(data).is_ok() {
        let _ = decoder.decode_headers();
    }

    // データを分割して feed (ストリーミングシナリオ)
    decoder.reset();
    for chunk in data.chunks(23) {
        if decoder.feed(chunk).is_err() {
            return;
        }
        if let Ok(Some((_, body_kind))) = decoder.decode_headers() {
            match body_kind {
                BodyKind::ContentLength(_) | BodyKind::Chunked | BodyKind::CloseDelimited => {
                    while let Some(body_data) = decoder.peek_body() {
                        let len = body_data.len();
                        match decoder.consume_body(len) {
                            Ok(BodyProgress::Complete { .. }) => break,
                            Ok(BodyProgress::Continue) => {}
                            Err(_) => break,
                        }
                    }
                }
                BodyKind::None | BodyKind::Tunnel => {}
            }
            break;
        }
    }
});
