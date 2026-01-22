#![no_main]

use libfuzzer_sys::fuzz_target;
use shiguredo_http11::conditional::{
    IfMatch, IfModifiedSince, IfNoneMatch, IfRange, IfUnmodifiedSince,
};
use shiguredo_http11::etag::EntityTag;

fuzz_target!(|data: &[u8]| {
    // UTF-8 文字列として解釈できる場合のみテスト
    if let Ok(s) = std::str::from_utf8(data) {
        // If-Match パース
        if let Ok(im) = IfMatch::parse(s) {
            let _ = im.is_any();
            let displayed = im.to_string();
            if let Ok(reparsed) = IfMatch::parse(&displayed) {
                assert_eq!(im.is_any(), reparsed.is_any());
            }
            // matches テスト
            if let Ok(etag) = EntityTag::strong("test") {
                let _ = im.matches(&etag);
            }
        }

        // If-None-Match パース
        if let Ok(inm) = IfNoneMatch::parse(s) {
            let _ = inm.is_any();
            let displayed = inm.to_string();
            if let Ok(reparsed) = IfNoneMatch::parse(&displayed) {
                assert_eq!(inm.is_any(), reparsed.is_any());
            }
        }

        // If-Modified-Since パース
        if let Ok(ims) = IfModifiedSince::parse(s) {
            let date = ims.date();
            let _ = date.day();
            let _ = date.month();
            let _ = date.year();
            let displayed = ims.to_string();
            if let Ok(reparsed) = IfModifiedSince::parse(&displayed) {
                assert_eq!(ims.date().day(), reparsed.date().day());
            }
        }

        // If-Unmodified-Since パース
        if let Ok(ius) = IfUnmodifiedSince::parse(s) {
            let date = ius.date();
            let _ = date.day();
        }

        // If-Range パース
        if let Ok(ir) = IfRange::parse(s) {
            let _ = ir.is_etag();
            let _ = ir.is_date();
            let _ = ir.etag();
            let _ = ir.date();
            let displayed = ir.to_string();
            if let Ok(reparsed) = IfRange::parse(&displayed) {
                assert_eq!(ir.is_etag(), reparsed.is_etag());
            }
        }
    }
});
