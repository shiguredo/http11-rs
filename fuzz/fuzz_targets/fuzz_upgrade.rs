//! Upgrade ヘッダーのパニック安全性と Display ラウンドトリップを検証する

#![no_main]

use libfuzzer_sys::fuzz_target;
use shiguredo_http11::upgrade::Upgrade;

fuzz_target!(|data: &[u8]| {
    if let Ok(s) = std::str::from_utf8(data) {
        if let Ok(value) = Upgrade::parse(s) {
            let _ = value.has_protocol("websocket");
            for protocol in value.protocols() {
                let _ = protocol.name();
                let _ = protocol.version();
            }
            let displayed = value.to_string();
            let _ = Upgrade::parse(&displayed);
        }
    }
});
