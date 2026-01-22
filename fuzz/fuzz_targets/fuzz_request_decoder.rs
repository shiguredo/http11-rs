#![no_main]

use libfuzzer_sys::fuzz_target;
use shiguredo_http11::{BodyKind, BodyProgress, RequestDecoder};

fuzz_target!(|data: &[u8]| {
    let mut decoder = RequestDecoder::new();

    // データを一度に feed
    if decoder.feed(data).is_ok() {
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
                BodyKind::None => {}
            }
        }
    }

    // データを分割して feed (ストリーミングシナリオ)
    decoder.reset();
    for chunk in data.chunks(17) {
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
                BodyKind::None => {}
            }
            break;
        }
    }
});
