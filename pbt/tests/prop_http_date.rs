//! HTTP-date のプロパティテスト

use proptest::prelude::*;
use shiguredo_http11::date::HttpDate;

// ========================================
// HTTP-date パースのテスト
// ========================================

// IMF-fixdate のラウンドトリップ
proptest! {
    #[test]
    fn prop_http_date_roundtrip(day in 1u8..=28, month in 1u8..=12, year in 1970u16..=2100, hour in 0u8..=23, minute in 0u8..=59, second in 0u8..=59) {
        let dow_names = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];
        let month_names = [
            "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
        ];
        let dow_idx = ((day as usize) + (month as usize) + (year as usize)) % 7;
        let dow = dow_names[dow_idx];
        let mon = month_names[(month - 1) as usize];

        let date_str = format!(
            "{}, {:02} {} {:04} {:02}:{:02}:{:02} GMT",
            dow, day, mon, year, hour, minute, second
        );

        let date = HttpDate::parse(&date_str).unwrap();
        prop_assert_eq!(date.day(), day);
        prop_assert_eq!(date.month(), month);
        prop_assert_eq!(date.year(), year);
        prop_assert_eq!(date.hour(), hour);
        prop_assert_eq!(date.minute(), minute);
        prop_assert_eq!(date.second(), second);

        // Display を再パース
        let displayed = date.to_string();
        let reparsed = HttpDate::parse(&displayed).unwrap();
        prop_assert_eq!(date.day(), reparsed.day());
        prop_assert_eq!(date.month(), reparsed.month());
        prop_assert_eq!(date.year(), reparsed.year());
    }
}

// 任意の文字列で HTTP-date パースがパニックしない
proptest! {
    #[test]
    fn prop_http_date_parse_no_panic(s in "[ -~]{0,64}") {
        let _ = HttpDate::parse(&s);
    }
}

// 有効な時刻の境界値
proptest! {
    #[test]
    fn prop_http_date_time_boundaries(hour in prop_oneof![Just(0u8), Just(12), Just(23)], minute in prop_oneof![Just(0u8), Just(30), Just(59)], second in prop_oneof![Just(0u8), Just(30), Just(59)]) {
        let date_str = format!(
            "Sun, 06 Nov 1994 {:02}:{:02}:{:02} GMT",
            hour, minute, second
        );
        let date = HttpDate::parse(&date_str).unwrap();
        prop_assert_eq!(date.hour(), hour);
        prop_assert_eq!(date.minute(), minute);
        prop_assert_eq!(date.second(), second);
    }
}
