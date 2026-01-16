#![no_main]

use libfuzzer_sys::fuzz_target;
use shiguredo_http11::ResponseDecoder;

fuzz_target!(|data: &[u8]| {
    // 通常のレスポンスデコード
    let mut decoder = ResponseDecoder::new();
    if decoder.feed(data).is_ok() {
        let _ = decoder.decode();
    }

    // HEAD リクエストへのレスポンスとしてデコード
    decoder.reset();
    decoder.set_expect_no_body(true);
    if decoder.feed(data).is_ok() {
        let _ = decoder.decode();
    }

    // データを分割して feed (ストリーミングシナリオ)
    decoder.reset();
    for chunk in data.chunks(23) {
        if decoder.feed(chunk).is_err() {
            return;
        }
        let _ = decoder.decode();
    }
});
