#![no_main]

use libfuzzer_sys::fuzz_target;
use shiguredo_http11::vary::Vary;

fuzz_target!(|data: &[u8]| {
    if let Ok(s) = std::str::from_utf8(data)
        && let Ok(vary) = Vary::parse(s)
    {
        let _ = vary.is_any();
        let _ = vary.fields();

        let displayed = vary.to_string();
        if let Ok(reparsed) = Vary::parse(&displayed) {
            assert_eq!(vary.is_any(), reparsed.is_any());
            assert_eq!(vary.fields(), reparsed.fields());
        }
    }
});
