#![no_main]

use libfuzzer_sys::fuzz_target;
use shiguredo_http11::expect::Expect;

fuzz_target!(|data: &[u8]| {
    if let Ok(s) = std::str::from_utf8(data) {
        if let Ok(value) = Expect::parse(s) {
            let _ = value.has_100_continue();
            for item in value.items() {
                let _ = item.token();
                let _ = item.value();
                let _ = item.is_100_continue();
            }
            let displayed = value.to_string();
            let _ = Expect::parse(&displayed);
        }
    }
});
