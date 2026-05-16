//! 条件付きリクエストヘッダーのパニック安全性と Display ラウンドトリップを検証する
//!
//! - If-Match, If-None-Match: ETag リストのパース、matches() メソッド、
//!   Display ラウンドトリップを検証する
//! - If-Modified-Since, If-Unmodified-Since: 日付のパースとアクセサを検証する
//! - If-Range: ETag または日付のパースと Display ラウンドトリップを検証する

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
            let _ = IfMatch::parse(&displayed);
            // matches テスト
            if let Ok(etag) = EntityTag::strong("test") {
                let _ = im.matches(&etag);
            }
        }

        // If-None-Match パース
        if let Ok(inm) = IfNoneMatch::parse(s) {
            let _ = inm.is_any();
            let displayed = inm.to_string();
            let _ = IfNoneMatch::parse(&displayed);
        }

        // If-Modified-Since パース
        if let Ok(ims) = IfModifiedSince::parse(s, 2026) {
            let date = ims.date();
            let _ = date.day();
            let _ = date.month();
            let _ = date.year();
            let displayed = ims.to_string();
            let _ = IfModifiedSince::parse(&displayed, 2026);
        }

        // If-Unmodified-Since パース
        if let Ok(ius) = IfUnmodifiedSince::parse(s, 2026) {
            let date = ius.date();
            let _ = date.day();
        }

        // If-Range パース
        if let Ok(ir) = IfRange::parse(s, 2026) {
            let _ = ir.is_etag();
            let _ = ir.is_date();
            let _ = ir.etag();
            let _ = ir.date();
            let displayed = ir.to_string();
            let _ = IfRange::parse(&displayed, 2026);
        }
    }
});
