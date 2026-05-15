//! Range 関連ヘッダーのパニック安全性と Display ラウンドトリップを検証する
//!
//! - Range: パースと unit, ranges, first アクセサ、to_bounds(1000) / to_bounds(0)
//!   による境界計算、Display ラウンドトリップを検証する
//! - Content-Range: パースと start, end, complete_length, is_unsatisfied
//!   アクセサ、Display ラウンドトリップを検証する
//! - Accept-Ranges: パースと accepts_bytes, is_none アクセサを検証する
//! - ContentRange::new_bytes(): バイナリデータから直接構築してアクセサと Display を検証する

#![no_main]

use libfuzzer_sys::fuzz_target;
use shiguredo_http11::range::{AcceptRanges, ContentRange, Range};

fuzz_target!(|data: &[u8]| {
    // ContentRange::new_bytes() の直接構築経路
    if data.len() >= 16 {
        let start = u64::from_le_bytes(data[0..8].try_into().unwrap());
        let end = u64::from_le_bytes(data[8..16].try_into().unwrap());
        let complete_length = if data.len() >= 24 {
            Some(u64::from_le_bytes(data[16..24].try_into().unwrap()))
        } else {
            None
        };

        // assert を回避する事前検証 (assert 経路は単体テストの #[should_panic] でカバー)
        if start <= end && complete_length.map_or(true, |cl| cl > end) {
            let cr = ContentRange::new_bytes(start, end, complete_length);
            let _ = cr.unit();
            let _ = cr.start();
            let _ = cr.end();
            let _ = cr.complete_length();
            let _ = cr.length();
            let _ = cr.is_unsatisfied();

            let displayed = cr.to_string();
            let _ = ContentRange::parse(&displayed);
        }
    }

    // UTF-8 文字列として解釈できる場合のみテスト
    if let Ok(s) = std::str::from_utf8(data) {
        // Range パース
        if let Ok(range) = Range::parse(s) {
            let _ = range.unit();
            let _ = range.is_bytes();
            let _ = range.ranges();
            let _ = range.first();

            // Display ラウンドトリップ
            let displayed = range.to_string();
            let _ = Range::parse(&displayed);

            // to_bounds テスト
            for spec in range.ranges() {
                let _ = spec.to_bounds(1000);
                let _ = spec.to_bounds(0);
            }
        }

        // Content-Range パース
        if let Ok(cr) = ContentRange::parse(s) {
            let _ = cr.unit();
            let _ = cr.start();
            let _ = cr.end();
            let _ = cr.complete_length();
            let _ = cr.length();
            let _ = cr.is_unsatisfied();

            // Display ラウンドトリップ
            let displayed = cr.to_string();
            let _ = ContentRange::parse(&displayed);
        }

        // Accept-Ranges パース
        if let Ok(ar) = AcceptRanges::parse(s) {
            let _ = ar.units();
            let _ = ar.accepts_bytes();
            let _ = ar.is_none();
        }
    }
});
