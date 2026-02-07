//! Digest Fields ヘッダーのパニック安全性と Display ラウンドトリップを検証する
//!
//! - Content-Digest: ダイジェスト値のパースとアクセサ、Display ラウンドトリップを検証する
//! - Repr-Digest: ダイジェスト値のパースとアクセサ、Display ラウンドトリップを検証する
//! - Want-Content-Digest: 要求ダイジェストのパースとアクセサ、Display ラウンドトリップを検証する
//! - Want-Repr-Digest: 要求ダイジェストのパースとアクセサ、Display ラウンドトリップを検証する

#![no_main]

use libfuzzer_sys::fuzz_target;
use shiguredo_http11::digest_fields::{
    ContentDigest, ReprDigest, WantContentDigest, WantReprDigest,
};

fuzz_target!(|data: &[u8]| {
    if let Ok(s) = std::str::from_utf8(data) {
        if let Ok(value) = ContentDigest::parse(s) {
            for item in value.items() {
                let _ = item.algorithm();
                let _ = item.value();
                let _ = item.value().bytes();
                let _ = value.get(item.algorithm());
            }
            let displayed = value.to_string();
            let _ = ContentDigest::parse(&displayed);
        }

        if let Ok(value) = ReprDigest::parse(s) {
            for item in value.items() {
                let _ = item.algorithm();
                let _ = item.value();
                let _ = item.value().bytes();
                let _ = value.get(item.algorithm());
            }
            let displayed = value.to_string();
            let _ = ReprDigest::parse(&displayed);
        }

        if let Ok(value) = WantContentDigest::parse(s) {
            for item in value.items() {
                let _ = item.algorithm();
                let _ = item.weight();
                let _ = value.get(item.algorithm());
            }
            let displayed = value.to_string();
            let _ = WantContentDigest::parse(&displayed);
        }

        if let Ok(value) = WantReprDigest::parse(s) {
            for item in value.items() {
                let _ = item.algorithm();
                let _ = item.weight();
                let _ = value.get(item.algorithm());
            }
            let displayed = value.to_string();
            let _ = WantReprDigest::parse(&displayed);
        }
    }
});
