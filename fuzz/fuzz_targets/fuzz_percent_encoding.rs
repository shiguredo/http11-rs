#![no_main]

use libfuzzer_sys::fuzz_target;
use shiguredo_http11::uri::{
    percent_decode, percent_decode_bytes, percent_encode, percent_encode_path, percent_encode_query,
};

fuzz_target!(|data: &[u8]| {
    if let Ok(s) = std::str::from_utf8(data) {
        let encoded = percent_encode(s);
        if let Ok(decoded) = percent_decode(&encoded) {
            assert_eq!(decoded, s);
        }

        let encoded_path = percent_encode_path(s);
        if let Ok(decoded) = percent_decode(&encoded_path) {
            assert_eq!(decoded, s);
        }

        let encoded_query = percent_encode_query(s);
        if let Ok(decoded) = percent_decode(&encoded_query) {
            assert_eq!(decoded, s);
        }

        let _ = percent_decode(s);
        let _ = percent_decode_bytes(s);
    }
});
