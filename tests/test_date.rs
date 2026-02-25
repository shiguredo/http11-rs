//! HTTP-date のユニットテスト

use shiguredo_http11::date::{DateError, DayOfWeek, HttpDate};

// ========================================
// DateError のテスト
// ========================================

#[test]
fn test_date_error_display() {
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

// ========================================
// DayOfWeek のテスト
// ========================================

#[test]
fn test_day_of_week_short_name() {
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

// ========================================
// うるう秒のテスト
// ========================================

#[test]
fn test_date_leap_second() {
    // 60秒 (うるう秒) は許可
    let date = HttpDate::parse("Sun, 06 Nov 1994 23:59:60 GMT").unwrap();
    assert_eq!(date.second(), 60);

    // new() でも許可
    let date = HttpDate::new(DayOfWeek::Sunday, 6, 11, 1994, 23, 59, 60).unwrap();
    assert_eq!(date.second(), 60);
}

// ========================================
// エラーケースのテスト
// ========================================

#[test]
fn test_date_parse_errors() {
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
fn test_date_invalid_time_format() {
    // コロンがない
    assert!(HttpDate::parse("Sun, 06 Nov 1994 084937 GMT").is_err());

    // 部分的
    assert!(HttpDate::parse("Sun, 06 Nov 1994 08:49 GMT").is_err());
}

// ========================================
// 全月のパーステスト
// ========================================

#[test]
fn test_date_all_months() {
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
fn test_date_all_days_of_week() {
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
// 境界値テスト
// ========================================

#[test]
fn test_date_boundary_values() {
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
fn test_date_rfc850_format_errors() {
    // 不正な日-月-年 形式
    assert!(HttpDate::parse("Sunday, 06-Nov 08:49:37 GMT").is_err());
    assert!(HttpDate::parse("Sunday, 06-Nov-94-extra 08:49:37 GMT").is_err());
}

#[test]
fn test_date_rfc850_4digit_year() {
    // RFC 850 形式でも 4 桁年は許可される (そのまま使用)
    let date = HttpDate::parse("Sunday, 06-Nov-1994 08:49:37 GMT").unwrap();
    assert_eq!(date.year(), 1994);
}
