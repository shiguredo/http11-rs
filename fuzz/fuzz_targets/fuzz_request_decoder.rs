#![no_main]

use libfuzzer_sys::fuzz_target;
use shiguredo_http11::RequestDecoder;

fuzz_target!(|data: &[u8]| {
    let mut decoder = RequestDecoder::new();

    // データを一度に feed
    if decoder.feed(data).is_ok() {
        let _ = decoder.decode();
    }

    // データを分割して feed (ストリーミングシナリオ)
    decoder.reset();
    for chunk in data.chunks(17) {
        if decoder.feed(chunk).is_err() {
            return;
        }
        let _ = decoder.decode();
    }
});
