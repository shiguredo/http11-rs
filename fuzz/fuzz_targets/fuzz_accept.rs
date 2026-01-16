#![no_main]

use libfuzzer_sys::fuzz_target;
use shiguredo_http11::accept::{Accept, AcceptCharset, AcceptEncoding, AcceptLanguage};

fuzz_target!(|data: &[u8]| {
    if let Ok(s) = std::str::from_utf8(data) {
        if let Ok(value) = Accept::parse(s) {
            for item in value.items() {
                let _ = item.media_type();
                let _ = item.subtype();
                let _ = item.parameters();
                let qvalue = item.qvalue();
                let _ = qvalue.value();
                let _ = qvalue.as_f32();
            }
            let displayed = value.to_string();
            let _ = Accept::parse(&displayed);
        }

        if let Ok(value) = AcceptCharset::parse(s) {
            for item in value.items() {
                let _ = item.charset();
                let qvalue = item.qvalue();
                let _ = qvalue.value();
                let _ = qvalue.as_f32();
            }
            let displayed = value.to_string();
            let _ = AcceptCharset::parse(&displayed);
        }

        if let Ok(value) = AcceptEncoding::parse(s) {
            for item in value.items() {
                let _ = item.coding();
                let qvalue = item.qvalue();
                let _ = qvalue.value();
                let _ = qvalue.as_f32();
            }
            let displayed = value.to_string();
            let _ = AcceptEncoding::parse(&displayed);
        }

        if let Ok(value) = AcceptLanguage::parse(s) {
            for item in value.items() {
                let _ = item.language();
                let qvalue = item.qvalue();
                let _ = qvalue.value();
                let _ = qvalue.as_f32();
            }
            let displayed = value.to_string();
            let _ = AcceptLanguage::parse(&displayed);
        }
    }
});
