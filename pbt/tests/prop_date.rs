//! HTTP-date のプロパティテスト

use proptest::prelude::*;
use shiguredo_http11::date::{DateError, DayOfWeek, HttpDate};

// ========================================
// Strategy 定義
// ========================================

// 曜日
fn day_of_week() -> impl Strategy<Value = DayOfWeek> {
    prop_oneof![
        Just(DayOfWeek::Sunday),
        Just(DayOfWeek::Monday),
        Just(DayOfWeek::Tuesday),
        Just(DayOfWeek::Wednesday),
        Just(DayOfWeek::Thursday),
        Just(DayOfWeek::Friday),
        Just(DayOfWeek::Saturday),
    ]
}

// 曜日の短い名前
fn day_name_short() -> impl Strategy<Value = &'static str> {
    prop_oneof![
        Just("Sun"),
        Just("Mon"),
        Just("Tue"),
        Just("Wed"),
        Just("Thu"),
        Just("Fri"),
        Just("Sat"),
    ]
}

// 曜日の長い名前
fn day_name_long() -> impl Strategy<Value = &'static str> {
    prop_oneof![
        Just("Sunday"),
        Just("Monday"),
        Just("Tuesday"),
        Just("Wednesday"),
        Just("Thursday"),
        Just("Friday"),
        Just("Saturday"),
    ]
}

// 月名
fn month_name() -> impl Strategy<Value = &'static str> {
    prop_oneof![
        Just("Jan"),
        Just("Feb"),
        Just("Mar"),
        Just("Apr"),
        Just("May"),
        Just("Jun"),
        Just("Jul"),
        Just("Aug"),
        Just("Sep"),
        Just("Oct"),
        Just("Nov"),
        Just("Dec"),
    ]
}

// 月番号
fn valid_month() -> impl Strategy<Value = u8> {
    1u8..=12u8
}

// 日 (1-31)
fn valid_day() -> impl Strategy<Value = u8> {
    1u8..=31u8
}

// 年 (1-9999)
fn valid_year() -> impl Strategy<Value = u16> {
    1u16..=9999u16
}

// RFC 850 の 2 桁年 (00-99)
fn rfc850_year() -> impl Strategy<Value = u8> {
    0u8..=99u8
}

// 時 (0-23)
fn valid_hour() -> impl Strategy<Value = u8> {
    0u8..=23u8
}

// 分 (0-59)
fn valid_minute() -> impl Strategy<Value = u8> {
    0u8..=59u8
}

// 秒 (0-60, うるう秒を含む)
fn valid_second() -> impl Strategy<Value = u8> {
    0u8..=60u8
}

// 通常の秒 (0-59)
fn normal_second() -> impl Strategy<Value = u8> {
    0u8..=59u8
}

proptest! {
    #[test]
    fn prop_day_of_week_clone_eq(dow in day_of_week()) {
        let cloned = dow;
        prop_assert_eq!(dow, cloned);
    }
}

// ========================================
// IMF-fixdate 形式のテスト
// ========================================

// IMF-fixdate のラウンドトリップ
proptest! {
    #[test]
    fn prop_http_date_imf_fixdate_roundtrip(
        dow in day_of_week(),
        day in valid_day(),
        month in valid_month(),
        year in 1900u16..=2100u16,
        hour in valid_hour(),
        minute in valid_minute(),
        second in normal_second()
    ) {
        let date = HttpDate::new(dow, day, month, year, hour, minute, second).unwrap();
        let displayed = date.to_string();
        let reparsed = HttpDate::parse(&displayed).unwrap();

        prop_assert_eq!(date.day(), reparsed.day());
        prop_assert_eq!(date.month(), reparsed.month());
        prop_assert_eq!(date.year(), reparsed.year());
        prop_assert_eq!(date.hour(), reparsed.hour());
        prop_assert_eq!(date.minute(), reparsed.minute());
        prop_assert_eq!(date.second(), reparsed.second());
        prop_assert_eq!(date.day_of_week(), reparsed.day_of_week());
    }
}

// IMF-fixdate パース
proptest! {
    #[test]
    fn prop_http_date_parse_imf_fixdate(
        dow_name in day_name_short(),
        day in valid_day(),
        month_str in month_name(),
        year in 1900u16..=2100u16,
        hour in valid_hour(),
        minute in valid_minute(),
        second in normal_second()
    ) {
        let date_str = format!(
            "{}, {:02} {} {:04} {:02}:{:02}:{:02} GMT",
            dow_name, day, month_str, year, hour, minute, second
        );
        let result = HttpDate::parse(&date_str);
        prop_assert!(result.is_ok());

        let date = result.unwrap();
        prop_assert_eq!(date.day(), day);
        prop_assert_eq!(date.year(), year);
        prop_assert_eq!(date.hour(), hour);
        prop_assert_eq!(date.minute(), minute);
        prop_assert_eq!(date.second(), second);
    }
}

// ========================================
// RFC 850 形式のテスト
// ========================================

// RFC 850 パース
proptest! {
    #[test]
    fn prop_http_date_parse_rfc850(
        dow_name in day_name_long(),
        day in valid_day(),
        month_str in month_name(),
        year in rfc850_year(),
        hour in valid_hour(),
        minute in valid_minute(),
        second in normal_second()
    ) {
        let date_str = format!(
            "{}, {:02}-{}-{:02} {:02}:{:02}:{:02} GMT",
            dow_name, day, month_str, year, hour, minute, second
        );
        let result = HttpDate::parse(&date_str);
        prop_assert!(result.is_ok());

        let date = result.unwrap();
        prop_assert_eq!(date.day(), day);
        prop_assert_eq!(date.hour(), hour);
        prop_assert_eq!(date.minute(), minute);
        prop_assert_eq!(date.second(), second);
    }
}

// RFC 850 2桁年の変換 (RFC 9110 Section 5.6.7)
// 注意: この関数は現在の年に依存するため、テストは現在の年に基づいて
// 期待値を計算する必要がある。
// RFC 9110: 50年以上未来に見える場合は100年引く
proptest! {
    #[test]
    fn prop_http_date_rfc850_year_conversion(
        year in rfc850_year()
    ) {
        let date_str = format!(
            "Sunday, 06-Nov-{:02} 08:49:37 GMT",
            year
        );
        let date = HttpDate::parse(&date_str).unwrap();

        // RFC 9110 Section 5.6.7:
        // 50年以上未来に見える場合は100年引く
        // 現在の年は実行時に決まるため、期待値も動的に計算する
        // この PBT では、パースが成功することと年が妥当な範囲にあることのみ確認
        // (具体的な期待値の検証は date.rs のユニットテストで行う)
        let parsed_year = date.year();
        // 年は 1900-2100 の範囲内であるべき
        prop_assert!((1900..=2100).contains(&parsed_year));
    }
}

// ========================================
// ANSI C asctime 形式のテスト
// ========================================

// asctime パース
proptest! {
    #[test]
    fn prop_http_date_parse_asctime(
        dow_name in day_name_short(),
        month_str in month_name(),
        day in valid_day(),
        hour in valid_hour(),
        minute in valid_minute(),
        second in normal_second(),
        year in 1900u16..=2100u16
    ) {
        // asctime: Sun Nov  6 08:49:37 1994
        let date_str = format!(
            "{} {} {:2} {:02}:{:02}:{:02} {}",
            dow_name, month_str, day, hour, minute, second, year
        );
        let result = HttpDate::parse(&date_str);
        prop_assert!(result.is_ok());

        let date = result.unwrap();
        prop_assert_eq!(date.day(), day);
        prop_assert_eq!(date.year(), year);
        prop_assert_eq!(date.hour(), hour);
        prop_assert_eq!(date.minute(), minute);
        prop_assert_eq!(date.second(), second);
    }
}

// ========================================
// HttpDate::new() のテスト
// ========================================

proptest! {
    #[test]
    fn prop_http_date_new_valid(
        dow in day_of_week(),
        day in valid_day(),
        month in valid_month(),
        year in valid_year(),
        hour in valid_hour(),
        minute in valid_minute(),
        second in valid_second()
    ) {
        let result = HttpDate::new(dow, day, month, year, hour, minute, second);
        prop_assert!(result.is_ok());

        let date = result.unwrap();
        prop_assert_eq!(date.day_of_week(), dow);
        prop_assert_eq!(date.day(), day);
        prop_assert_eq!(date.month(), month);
        prop_assert_eq!(date.year(), year);
        prop_assert_eq!(date.hour(), hour);
        prop_assert_eq!(date.minute(), minute);
        prop_assert_eq!(date.second(), second);
    }
}

// 無効な日
proptest! {
    #[test]
    fn prop_http_date_new_invalid_day(
        dow in day_of_week(),
        month in valid_month(),
        year in valid_year(),
        invalid_day in prop_oneof![Just(0u8), 32u8..=255u8]
    ) {
        let result = HttpDate::new(dow, invalid_day, month, year, 0, 0, 0);
        prop_assert!(matches!(result, Err(DateError::InvalidDay)));
    }
}

// 無効な月
proptest! {
    #[test]
    fn prop_http_date_new_invalid_month(
        dow in day_of_week(),
        day in valid_day(),
        year in valid_year(),
        invalid_month in prop_oneof![Just(0u8), 13u8..=255u8]
    ) {
        let result = HttpDate::new(dow, day, invalid_month, year, 0, 0, 0);
        prop_assert!(matches!(result, Err(DateError::InvalidMonth)));
    }
}

// 無効な時
proptest! {
    #[test]
    fn prop_http_date_new_invalid_hour(
        dow in day_of_week(),
        day in valid_day(),
        month in valid_month(),
        year in valid_year(),
        invalid_hour in 24u8..=255u8
    ) {
        let result = HttpDate::new(dow, day, month, year, invalid_hour, 0, 0);
        prop_assert!(matches!(result, Err(DateError::InvalidHour)));
    }
}

// 無効な分
proptest! {
    #[test]
    fn prop_http_date_new_invalid_minute(
        dow in day_of_week(),
        day in valid_day(),
        month in valid_month(),
        year in valid_year(),
        invalid_minute in 60u8..=255u8
    ) {
        let result = HttpDate::new(dow, day, month, year, 0, invalid_minute, 0);
        prop_assert!(matches!(result, Err(DateError::InvalidMinute)));
    }
}

// 無効な秒
proptest! {
    #[test]
    fn prop_http_date_new_invalid_second(
        dow in day_of_week(),
        day in valid_day(),
        month in valid_month(),
        year in valid_year(),
        invalid_second in 61u8..=255u8
    ) {
        let result = HttpDate::new(dow, day, month, year, 0, 0, invalid_second);
        prop_assert!(matches!(result, Err(DateError::InvalidSecond)));
    }
}

// ========================================
// Display のテスト
// ========================================

proptest! {
    #[test]
    fn prop_http_date_display_format(
        dow in day_of_week(),
        day in valid_day(),
        month in valid_month(),
        year in 1900u16..=2100u16,
        hour in valid_hour(),
        minute in valid_minute(),
        second in normal_second()
    ) {
        let date = HttpDate::new(dow, day, month, year, hour, minute, second).unwrap();
        let displayed = date.to_string();

        // IMF-fixdate 形式であることを確認
        prop_assert!(displayed.contains(", "));
        prop_assert!(displayed.ends_with(" GMT"));
        prop_assert!(displayed.contains(dow.short_name()));
    }
}

// ========================================
// Clone と PartialEq のテスト
// ========================================

proptest! {
    #[test]
    fn prop_http_date_clone_eq(
        dow in day_of_week(),
        day in valid_day(),
        month in valid_month(),
        year in valid_year(),
        hour in valid_hour(),
        minute in valid_minute(),
        second in valid_second()
    ) {
        let date = HttpDate::new(dow, day, month, year, hour, minute, second).unwrap();
        let cloned = date.clone();
        prop_assert_eq!(date, cloned);
    }
}

// ========================================
// 空白処理のテスト
// ========================================

proptest! {
    #[test]
    fn prop_http_date_trim_whitespace(
        dow_name in day_name_short(),
        day in valid_day(),
        month_str in month_name(),
        year in 1900u16..=2100u16,
        hour in valid_hour(),
        minute in valid_minute(),
        second in normal_second()
    ) {
        let date_str = format!(
            "  {}, {:02} {} {:04} {:02}:{:02}:{:02} GMT  ",
            dow_name, day, month_str, year, hour, minute, second
        );
        let result = HttpDate::parse(&date_str);
        prop_assert!(result.is_ok());
    }
}

// ========================================
// no_panic テスト
// ========================================

proptest! {
    #[test]
    fn prop_http_date_parse_no_panic(s in "[ -~]{0,64}") {
        let _ = HttpDate::parse(&s);
    }
}

proptest! {
    #[test]
    fn prop_http_date_parse_no_panic_extended(s in ".{0,128}") {
        let _ = HttpDate::parse(&s);
    }
}
