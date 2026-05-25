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
// HttpDate::new の月別日数検証
// ========================================

#[test]
fn test_date_month_day_validation() {
    // 6 月 31 日は存在しない
    assert!(matches!(
        HttpDate::new(DayOfWeek::Sunday, 31, 6, 1994, 8, 49, 37),
        Err(DateError::InvalidDay)
    ));

    // 2 月 30 日は存在しない
    assert!(matches!(
        HttpDate::new(DayOfWeek::Monday, 30, 2, 2000, 0, 0, 0),
        Err(DateError::InvalidDay)
    ));

    // 4 月 31 日は存在しない
    assert!(matches!(
        HttpDate::new(DayOfWeek::Tuesday, 31, 4, 2000, 0, 0, 0),
        Err(DateError::InvalidDay)
    ));
}

#[test]
fn test_date_february_leap_year() {
    // うるう年の 2 月 29 日は有効
    let date = HttpDate::new(DayOfWeek::Tuesday, 29, 2, 2000, 0, 0, 0).unwrap();
    assert_eq!(date.day(), 29);
    assert_eq!(date.month(), 2);
    assert_eq!(date.year(), 2000);

    // うるう年の 2 月 28 日も有効
    let date = HttpDate::new(DayOfWeek::Monday, 28, 2, 2000, 0, 0, 0).unwrap();
    assert_eq!(date.day(), 28);

    // 平年の 2 月 29 日は拒否
    assert!(matches!(
        HttpDate::new(DayOfWeek::Thursday, 29, 2, 2001, 0, 0, 0),
        Err(DateError::InvalidDay)
    ));

    // 平年の 2 月 28 日は有効
    let date = HttpDate::new(DayOfWeek::Wednesday, 28, 2, 2001, 0, 0, 0).unwrap();
    assert_eq!(date.day(), 28);
}

#[test]
fn test_date_30_day_months() {
    // 4/6/9/11 月の 30 日は有効、31 日は拒否
    for month in [4, 6, 9, 11] {
        let date = HttpDate::new(DayOfWeek::Monday, 30, month, 2000, 0, 0, 0).unwrap();
        assert_eq!(date.day(), 30);

        assert!(matches!(
            HttpDate::new(DayOfWeek::Monday, 31, month, 2000, 0, 0, 0),
            Err(DateError::InvalidDay)
        ));
    }
}

#[test]
fn test_date_31_day_months() {
    // 1/3/5/7/8/10/12 月の 31 日は有効
    for month in [1, 3, 5, 7, 8, 10, 12] {
        let date = HttpDate::new(DayOfWeek::Monday, 31, month, 2000, 0, 0, 0).unwrap();
        assert_eq!(date.day(), 31);
    }
}

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

// ========================================
// NBSP は OWS ではないことの検証 (RFC 9110 Section 5.6.3)
// ========================================

#[test]
fn test_date_nbsp_not_stripped_as_ows() {
    // NBSP は OWS ではないため除去されず、日付パースに失敗する
    let result = HttpDate::parse("\u{00A0}Sun, 06 Nov 1994 08:49:37 GMT");
    assert!(result.is_err());
}

#[test]
fn test_date_sp_htab_stripped_as_ows() {
    // SP と HTAB は OWS として正しく除去される
    let date = HttpDate::parse(" \tSun, 06 Nov 1994 08:49:37 GMT\t ").unwrap();
    assert_eq!(date.year(), 1994);
}

#[test]
fn test_date_rfc850_nbsp_after_comma_not_stripped() {
    // カンマ直後の NBSP は trim_ows_start で除去されない。
    // ここでは先頭の NBSP が trim_ows で除去されず、
    // 曜日名のパースに失敗することを確認する
    let result = HttpDate::parse_rfc850("\u{00A0}Sunday, 06-Nov-94 08:49:37 GMT", 2026);
    assert!(result.is_err());
}

// ========================================
// src/date.rs のインラインテストを移動
// ========================================

#[test]
fn test_parse_imf_fixdate() {
    let date = HttpDate::parse("Sun, 06 Nov 1994 08:49:37 GMT").unwrap();
    assert_eq!(date.day_of_week(), DayOfWeek::Sunday);
    assert_eq!(date.day(), 6);
    assert_eq!(date.month(), 11);
    assert_eq!(date.year(), 1994);
    assert_eq!(date.hour(), 8);
    assert_eq!(date.minute(), 49);
    assert_eq!(date.second(), 37);
}

#[test]
fn test_parse_rejects_rfc850() {
    assert!(matches!(
        HttpDate::parse("Sunday, 06-Nov-94 08:49:37 GMT"),
        Err(DateError::Rfc850Date)
    ));
}

#[test]
fn test_parse_rfc850() {
    let date = HttpDate::parse_rfc850("Sunday, 06-Nov-94 08:49:37 GMT", 2026).unwrap();
    assert_eq!(date.day_of_week(), DayOfWeek::Sunday);
    assert_eq!(date.day(), 6);
    assert_eq!(date.month(), 11);
    assert_eq!(date.year(), 1994);
    assert_eq!(date.hour(), 8);
    assert_eq!(date.minute(), 49);
    assert_eq!(date.second(), 37);
}

#[test]
fn test_parse_rfc850_4digit_year_accepted() {
    let date = HttpDate::parse_rfc850("Sunday, 06-Nov-1994 08:49:37 GMT", 2026).unwrap();
    assert_eq!(date.year(), 1994);
}

#[test]
fn test_parse_asctime() {
    let date = HttpDate::parse("Sun Nov  6 08:49:37 1994").unwrap();
    assert_eq!(date.day_of_week(), DayOfWeek::Sunday);
    assert_eq!(date.day(), 6);
    assert_eq!(date.month(), 11);
    assert_eq!(date.year(), 1994);
    assert_eq!(date.hour(), 8);
    assert_eq!(date.minute(), 49);
    assert_eq!(date.second(), 37);
}

#[test]
fn test_display() {
    let date = HttpDate::parse("Sun, 06 Nov 1994 08:49:37 GMT").unwrap();
    assert_eq!(date.to_string(), "Sun, 06 Nov 1994 08:49:37 GMT");
}

#[test]
fn test_parse_rfc850_2digit_year() {
    let date = HttpDate::parse_rfc850("Sunday, 06-Nov-20 08:49:37 GMT", 2026).unwrap();
    assert_eq!(date.year(), 2020);

    let date = HttpDate::parse_rfc850("Sunday, 06-Nov-76 08:49:37 GMT", 2026).unwrap();
    assert_eq!(date.year(), 2076);

    let date = HttpDate::parse_rfc850("Sunday, 06-Nov-77 08:49:37 GMT", 2026).unwrap();
    assert_eq!(date.year(), 1977);

    let date = HttpDate::parse_rfc850("Sunday, 06-Nov-99 08:49:37 GMT", 2026).unwrap();
    assert_eq!(date.year(), 1999);
}

#[test]
fn test_parse_rfc850_2digit_year_boundary() {
    let date = HttpDate::parse_rfc850("Sunday, 06-Nov-00 08:49:37 GMT", 2050).unwrap();
    assert_eq!(date.year(), 2000);

    let date = HttpDate::parse_rfc850("Sunday, 06-Nov-01 08:49:37 GMT", 2050).unwrap();
    assert_eq!(date.year(), 2001);

    let date = HttpDate::parse_rfc850("Sunday, 06-Nov-50 08:49:37 GMT", 2050).unwrap();
    assert_eq!(date.year(), 2050);
}

#[test]
fn test_parse_empty() {
    assert!(HttpDate::parse("").is_err());
}

#[test]
fn test_parse_invalid_format() {
    assert!(HttpDate::parse("not a date").is_err());
    assert!(HttpDate::parse("Sun, 06 Nov").is_err());
}

#[test]
fn test_parse_invalid_day() {
    assert!(HttpDate::parse("Sun, 32 Nov 1994 08:49:37 GMT").is_err());
    assert!(HttpDate::parse("Sun, 00 Nov 1994 08:49:37 GMT").is_err());
}

#[test]
fn test_parse_invalid_month() {
    assert!(HttpDate::parse("Sun, 06 Xyz 1994 08:49:37 GMT").is_err());
}

#[test]
fn test_parse_invalid_time() {
    assert!(HttpDate::parse("Sun, 06 Nov 1994 25:49:37 GMT").is_err());
    assert!(HttpDate::parse("Sun, 06 Nov 1994 08:60:37 GMT").is_err());
    assert!(HttpDate::parse("Sun, 06 Nov 1994 08:49:61 GMT").is_err());
}

#[test]
fn test_parse_not_gmt() {
    assert!(HttpDate::parse("Sun, 06 Nov 1994 08:49:37 UTC").is_err());
    assert!(HttpDate::parse("Sun, 06 Nov 1994 08:49:37 PST").is_err());
}

#[test]
fn test_leap_second() {
    let date = HttpDate::parse("Sun, 06 Nov 1994 23:59:60 GMT").unwrap();
    assert_eq!(date.second(), 60);
}

#[test]
fn test_all_months() {
    for (month, name) in [
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
    ] {
        let date_str = format!("Sun, 06 {} 1994 08:49:37 GMT", name);
        let date = HttpDate::parse(&date_str).unwrap();
        assert_eq!(date.month(), month);
    }
}

#[test]
fn test_all_days_of_week() {
    for (dow, name) in [
        (DayOfWeek::Sunday, "Sun"),
        (DayOfWeek::Monday, "Mon"),
        (DayOfWeek::Tuesday, "Tue"),
        (DayOfWeek::Wednesday, "Wed"),
        (DayOfWeek::Thursday, "Thu"),
        (DayOfWeek::Friday, "Fri"),
        (DayOfWeek::Saturday, "Sat"),
    ] {
        let date_str = format!("{}, 06 Nov 1994 08:49:37 GMT", name);
        let date = HttpDate::parse(&date_str).unwrap();
        assert_eq!(date.day_of_week(), dow);
    }
}

#[test]
fn test_ord() {
    let d1 = HttpDate::parse("Sun, 06 Nov 1994 08:49:37 GMT").unwrap();
    let d2 = HttpDate::parse("Mon, 07 Nov 1994 08:49:37 GMT").unwrap();
    let d3 = HttpDate::parse("Sun, 06 Nov 1994 08:49:37 GMT").unwrap();

    assert!(d1 < d2);
    assert!(d2 > d1);
    assert_eq!(d1, d3);
    assert!(d1 <= d3);
    assert!(d1 >= d3);
}
