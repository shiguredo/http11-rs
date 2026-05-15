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
fn test_date_rfc850_all_days_of_week() {
    // rfc850-date 形式は長い曜日名 (Monday, Tuesday, ...) を使う (RFC 9110 §5.6.7 ABNF: day-name-l)
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
        let date_str = format!("{}, 06-Nov-94 08:49:37 GMT", dow_name);
        let date = HttpDate::parse_rfc850(&date_str, 2026).unwrap();
        assert_eq!(date.day_of_week(), expected_dow);
    }
}

// ========================================
// RFC 850 形式の追加テスト
// ========================================

#[test]
fn test_date_rfc850_format_errors() {
    // 不正な日-月-年 形式
    assert!(HttpDate::parse_rfc850("Sunday, 06-Nov 08:49:37 GMT", 2026).is_err());
    assert!(HttpDate::parse_rfc850("Sunday, 06-Nov-94-extra 08:49:37 GMT", 2026).is_err());
}

#[test]
fn test_date_rfc850_4digit_year() {
    // RFC 9110 §5.6.7 ABNF では 2DIGIT 固定だが、Postel 原則で 4 桁年も受理する。
    let date = HttpDate::parse_rfc850("Sunday, 06-Nov-1994 08:49:37 GMT", 2026).unwrap();
    assert_eq!(date.year(), 1994);
}
