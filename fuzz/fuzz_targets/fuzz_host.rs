//! Host ヘッダーのパニック安全性と Display ラウンドトリップを検証する

#![no_main]

use libfuzzer_sys::fuzz_target;
use shiguredo_http11::host::Host;

fuzz_target!(|data: &[u8]| {
    if let Ok(s) = std::str::from_utf8(data) {
        if let Ok(value) = Host::parse(s) {
            let _ = value.host();
            let _ = value.port();
            let _ = value.is_ipv6();
            let displayed = value.to_string();
            let _ = Host::parse(&displayed);
        }
    }
});
