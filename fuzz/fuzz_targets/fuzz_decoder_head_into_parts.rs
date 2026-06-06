//! RequestHead / ResponseHead の into_parts() 経路のパニック安全性を検証する
//!
//! 任意入力 → デコード → into_parts() → with_version() の
//! ラウンドトリップでパニックしないことを確認する。

#![no_main]

use libfuzzer_sys::fuzz_target;
use shiguredo_http11::{Request, RequestDecoder, Response, ResponseDecoder};

fuzz_target!(|data: &[u8]| {
    // --- RequestHead 経路 ---
    let mut decoder = RequestDecoder::new();
    if decoder.feed(data).is_ok() && let Ok(Some((head, _))) = decoder.decode_headers() {
        let (method, uri, version, headers) = head.into_parts();
        let mut request = match Request::with_version(method, uri, version) {
            Ok(r) => r,
            Err(_) => return,
        };
        for (name, value) in headers {
            let _ = request.add_header(name, value);
        }
        let _ = request.encode_headers();
    }

    // --- ResponseHead 経路 ---
    let mut decoder = ResponseDecoder::new();
    if decoder.feed(data).is_ok() && let Ok(Some((head, _))) = decoder.decode_headers() {
        let (version, status_code, reason_phrase, headers) = head.into_parts();
        let mut response = match Response::with_version(version, status_code, reason_phrase) {
            Ok(r) => r,
            Err(_) => return,
        };
        for (name, value) in headers {
            let _ = response.add_header(name, value);
        }
        let _ = response.encode_headers();
    }
});
