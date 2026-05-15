//! パーセントエンコーディングのパニック安全性を検証する
//!
//! - 任意入力に対する percent_encode / percent_decode / percent_encode_path /
//!   percent_encode_query / percent_decode_bytes のパニック安全性を確認する

#![no_main]

use libfuzzer_sys::fuzz_target;
use shiguredo_http11::uri::{
    percent_decode, percent_decode_bytes, percent_encode, percent_encode_path, percent_encode_query,
};

fuzz_target!(|data: &[u8]| {
    if let Ok(s) = std::str::from_utf8(data) {
        let _ = percent_encode(s);
        let _ = percent_encode_path(s);
        let _ = percent_encode_query(s);
        let _ = percent_decode(s);
        let _ = percent_decode_bytes(s);
    }
});
