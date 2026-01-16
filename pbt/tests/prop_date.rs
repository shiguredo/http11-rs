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

// ========================================
// DateError のテスト
// ========================================

#[test]
fn date_error_display() {
    let errors = [
        (DateError::Empty, "empty date"),
        (DateError::InvalidFormat, "invalid date format"),
        (DateError::InvalidDayName, "invalid day name"),
        (DateError::InvalidDay, "invalid day"),
        (DateError::InvalidMonth, "invalid month"),
        (DateError::InvalidYear, "invalid year"),
        (DateError::InvalidHour, "invalid hour"),
        (DateError::InvalidMinute, "invalid minute"),
        (DateError::InvalidSecond, "invalid second"),
        (DateError::NotGmt, "timezone is not GMT"),
    ];

    for (error, expected) in errors {
        assert_eq!(error.to_string(), expected);
    }
}

#[test]
fn date_error_is_error_trait() {
    let error: Box<dyn std::error::Error> = Box::new(DateError::Empty);
    assert_eq!(error.to_string(), "empty date");
}

#[test]
fn date_error_clone_eq() {
    let error = DateError::InvalidFormat;
    let cloned = error.clone();
    assert_eq!(error, cloned);
}

// ========================================
// DayOfWeek のテスト
// ========================================

#[test]
fn day_of_week_short_name() {
    let days = [
        (DayOfWeek::Sunday, "Sun"),
        (DayOfWeek::Monday, "Mon"),
        (DayOfWeek::Tuesday, "Tue"),
        (DayOfWeek::Wednesday, "Wed"),
        (DayOfWeek::Thursday, "Thu"),
        (DayOfWeek::Friday, "Fri"),
        (DayOfWeek::Saturday, "Sat"),
    ];

    for (day, expected) in days {
        assert_eq!(day.short_name(), expected);
    }
}

proptest! {
    #[test]
    fn day_of_week_clone_eq(dow in day_of_week()) {
        let cloned = dow.clone();
        prop_assert_eq!(dow, cloned);
    }
}

// ========================================
// IMF-fixdate 形式のテスト
// ========================================

// IMF-fixdate のラウンドトリップ
proptest! {
    #[test]
    fn http_date_imf_fixdate_roundtrip(
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
    fn http_date_parse_imf_fixdate(
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
    fn http_date_parse_rfc850(
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

// RFC 850 2桁年の変換
proptest! {
    #[test]
    fn http_date_rfc850_year_conversion(
        year in rfc850_year()
    ) {
        let date_str = format!(
            "Sunday, 06-Nov-{:02} 08:49:37 GMT",
            year
        );
        let date = HttpDate::parse(&date_str).unwrap();

        // 00-49 は 2000-2049, 50-99 は 1950-1999
        let expected_year = if year < 50 { 2000 + year as u16 } else { 1900 + year as u16 };
        prop_assert_eq!(date.year(), expected_year);
    }
}

// ========================================
// ANSI C asctime 形式のテスト
// ========================================

// asctime パース
proptest! {
    #[test]
    fn http_date_parse_asctime(
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
    fn http_date_new_valid(
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
    fn http_date_new_invalid_day(
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
    fn http_date_new_invalid_month(
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
    fn http_date_new_invalid_hour(
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
    fn http_date_new_invalid_minute(
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
    fn http_date_new_invalid_second(
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
// うるう秒のテスト
// ========================================

#[test]
fn http_date_leap_second() {
    // 60秒 (うるう秒) は許可
    let date = HttpDate::parse("Sun, 06 Nov 1994 23:59:60 GMT").unwrap();
    assert_eq!(date.second(), 60);

    // new() でも許可
    let date = HttpDate::new(DayOfWeek::Sunday, 6, 11, 1994, 23, 59, 60).unwrap();
    assert_eq!(date.second(), 60);
}

// ========================================
// Display のテスト
// ========================================

proptest! {
    #[test]
    fn http_date_display_format(
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
// エラーケースのテスト
// ========================================

#[test]
fn http_date_parse_errors() {
    // 空
    assert!(matches!(HttpDate::parse(""), Err(DateError::Empty)));
    assert!(matches!(HttpDate::parse("   "), Err(DateError::Empty)));

    // 不正な形式
    assert!(matches!(
        HttpDate::parse("not a date"),
        Err(DateError::InvalidFormat) | Err(DateError::InvalidDayName)
    ));

    // 不正な曜日
    assert!(matches!(
        HttpDate::parse("Xyz, 06 Nov 1994 08:49:37 GMT"),
        Err(DateError::InvalidDayName)
    ));

    // 不正な日
    assert!(matches!(
        HttpDate::parse("Sun, xx Nov 1994 08:49:37 GMT"),
        Err(DateError::InvalidDay)
    ));
    assert!(matches!(
        HttpDate::parse("Sun, 00 Nov 1994 08:49:37 GMT"),
        Err(DateError::InvalidDay)
    ));
    assert!(matches!(
        HttpDate::parse("Sun, 32 Nov 1994 08:49:37 GMT"),
        Err(DateError::InvalidDay)
    ));

    // 不正な月
    assert!(matches!(
        HttpDate::parse("Sun, 06 Xyz 1994 08:49:37 GMT"),
        Err(DateError::InvalidMonth)
    ));

    // 不正な年
    assert!(matches!(
        HttpDate::parse("Sun, 06 Nov xxxx 08:49:37 GMT"),
        Err(DateError::InvalidYear)
    ));

    // 不正な時
    assert!(matches!(
        HttpDate::parse("Sun, 06 Nov 1994 25:49:37 GMT"),
        Err(DateError::InvalidHour)
    ));

    // 不正な分
    assert!(matches!(
        HttpDate::parse("Sun, 06 Nov 1994 08:60:37 GMT"),
        Err(DateError::InvalidMinute)
    ));

    // 不正な秒
    assert!(matches!(
        HttpDate::parse("Sun, 06 Nov 1994 08:49:61 GMT"),
        Err(DateError::InvalidSecond)
    ));

    // GMT ではない
    assert!(matches!(
        HttpDate::parse("Sun, 06 Nov 1994 08:49:37 UTC"),
        Err(DateError::NotGmt)
    ));
    assert!(matches!(
        HttpDate::parse("Sun, 06 Nov 1994 08:49:37 PST"),
        Err(DateError::NotGmt)
    ));
}

// 不正な時刻形式
#[test]
fn http_date_invalid_time_format() {
    // コロンがない
    assert!(HttpDate::parse("Sun, 06 Nov 1994 084937 GMT").is_err());

    // 部分的
    assert!(HttpDate::parse("Sun, 06 Nov 1994 08:49 GMT").is_err());
}

// ========================================
// Clone と PartialEq のテスト
// ========================================

proptest! {
    #[test]
    fn http_date_clone_eq(
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
// 全月のパーステスト
// ========================================

#[test]
fn http_date_all_months() {
    let months = [
        (1, "Jan"),
        (2, "Feb"),
        (3, "Mar"),
        (4, "Apr"),
        (5, "May"),
        (6, "Jun"),
        (7, "Jul"),
        (8, "Aug"),
        (9, "Sep"),
        (10, "Oct"),
        (11, "Nov"),
        (12, "Dec"),
    ];

    for (expected_month, month_name) in months {
        let date_str = format!("Sun, 06 {} 1994 08:49:37 GMT", month_name);
        let date = HttpDate::parse(&date_str).unwrap();
        assert_eq!(date.month(), expected_month);
    }
}

// ========================================
// 全曜日のパーステスト
// ========================================

#[test]
fn http_date_all_days_of_week() {
    let days = [
        (DayOfWeek::Sunday, "Sun"),
        (DayOfWeek::Monday, "Mon"),
        (DayOfWeek::Tuesday, "Tue"),
        (DayOfWeek::Wednesday, "Wed"),
        (DayOfWeek::Thursday, "Thu"),
        (DayOfWeek::Friday, "Fri"),
        (DayOfWeek::Saturday, "Sat"),
    ];

    for (expected_dow, dow_name) in days {
        let date_str = format!("{}, 06 Nov 1994 08:49:37 GMT", dow_name);
        let date = HttpDate::parse(&date_str).unwrap();
        assert_eq!(date.day_of_week(), expected_dow);
    }

    // 長い名前も
    let long_days = [
        (DayOfWeek::Sunday, "Sunday"),
        (DayOfWeek::Monday, "Monday"),
        (DayOfWeek::Tuesday, "Tuesday"),
        (DayOfWeek::Wednesday, "Wednesday"),
        (DayOfWeek::Thursday, "Thursday"),
        (DayOfWeek::Friday, "Friday"),
        (DayOfWeek::Saturday, "Saturday"),
    ];

    for (expected_dow, dow_name) in long_days {
        // RFC 850 形式
        let date_str = format!("{}, 06-Nov-94 08:49:37 GMT", dow_name);
        let date = HttpDate::parse(&date_str).unwrap();
        assert_eq!(date.day_of_week(), expected_dow);
    }
}

// ========================================
// 空白処理のテスト
// ========================================

proptest! {
    #[test]
    fn http_date_trim_whitespace(
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
    fn http_date_parse_no_panic(s in "[ -~]{0,64}") {
        let _ = HttpDate::parse(&s);
    }
}

proptest! {
    #[test]
    fn http_date_parse_no_panic_extended(s in ".{0,128}") {
        let _ = HttpDate::parse(&s);
    }
}

// ========================================
// 境界値テスト
// ========================================

#[test]
fn http_date_boundary_values() {
    // 最小値
    let date = HttpDate::new(DayOfWeek::Sunday, 1, 1, 1, 0, 0, 0).unwrap();
    assert_eq!(date.day(), 1);
    assert_eq!(date.month(), 1);
    assert_eq!(date.year(), 1);
    assert_eq!(date.hour(), 0);
    assert_eq!(date.minute(), 0);
    assert_eq!(date.second(), 0);

    // 最大値
    let date = HttpDate::new(DayOfWeek::Saturday, 31, 12, 9999, 23, 59, 60).unwrap();
    assert_eq!(date.day(), 31);
    assert_eq!(date.month(), 12);
    assert_eq!(date.year(), 9999);
    assert_eq!(date.hour(), 23);
    assert_eq!(date.minute(), 59);
    assert_eq!(date.second(), 60);
}

// ========================================
// RFC 850 形式の追加テスト
// ========================================

#[test]
fn http_date_rfc850_format_errors() {
    // 不正な日-月-年 形式
    assert!(HttpDate::parse("Sunday, 06-Nov 08:49:37 GMT").is_err());
    assert!(HttpDate::parse("Sunday, 06-Nov-94-extra 08:49:37 GMT").is_err());
}

#[test]
fn http_date_rfc850_with_4digit_year() {
    // RFC 850 形式でも 4 桁年は許可される (そのまま使用)
    let date = HttpDate::parse("Sunday, 06-Nov-1994 08:49:37 GMT").unwrap();
    assert_eq!(date.year(), 1994);
}
