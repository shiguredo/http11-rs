//! HTTP-date パース (RFC 9110 Section 5.6.7)
//!
//! ## 概要
//!
//! RFC 9110 に基づいた HTTP-date のパースと生成を提供します。
//!
//! ## 使い方
//!
//! ```rust
//! use shiguredo_http11::date::HttpDate;
//!
//! // IMF-fixdate 形式のパース
//! let date = HttpDate::parse("Sun, 06 Nov 1994 08:49:37 GMT").unwrap();
//! assert_eq!(date.year(), 1994);
//! assert_eq!(date.month(), 11);
//! assert_eq!(date.day(), 6);
//!
//! // HTTP-date 形式で出力
//! assert_eq!(date.to_string(), "Sun, 06 Nov 1994 08:49:37 GMT");
//! ```

use core::fmt;

/// HTTP-date パースエラー
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DateError {
    /// 空の日付
    Empty,
    /// 不正な形式
    InvalidFormat,
    /// 不正な曜日
    InvalidDayName,
    /// 不正な日
    InvalidDay,
    /// 不正な月
    InvalidMonth,
    /// 不正な年
    InvalidYear,
    /// 不正な時
    InvalidHour,
    /// 不正な分
    InvalidMinute,
    /// 不正な秒
    InvalidSecond,
    /// GMT ではない
    NotGmt,
}

impl fmt::Display for DateError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DateError::Empty => write!(f, "empty date"),
            DateError::InvalidFormat => write!(f, "invalid date format"),
            DateError::InvalidDayName => write!(f, "invalid day name"),
            DateError::InvalidDay => write!(f, "invalid day"),
            DateError::InvalidMonth => write!(f, "invalid month"),
            DateError::InvalidYear => write!(f, "invalid year"),
            DateError::InvalidHour => write!(f, "invalid hour"),
            DateError::InvalidMinute => write!(f, "invalid minute"),
            DateError::InvalidSecond => write!(f, "invalid second"),
            DateError::NotGmt => write!(f, "timezone is not GMT"),
        }
    }
}

impl std::error::Error for DateError {}

/// 曜日
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DayOfWeek {
    Sunday,
    Monday,
    Tuesday,
    Wednesday,
    Thursday,
    Friday,
    Saturday,
}

impl DayOfWeek {
    /// 短い形式の曜日名を取得
    pub fn short_name(&self) -> &'static str {
        match self {
            DayOfWeek::Sunday => "Sun",
            DayOfWeek::Monday => "Mon",
            DayOfWeek::Tuesday => "Tue",
            DayOfWeek::Wednesday => "Wed",
            DayOfWeek::Thursday => "Thu",
            DayOfWeek::Friday => "Fri",
            DayOfWeek::Saturday => "Sat",
        }
    }

    /// 曜日名からパース
    fn from_name(s: &str) -> Option<Self> {
        match s {
            "Sun" | "Sunday" => Some(DayOfWeek::Sunday),
            "Mon" | "Monday" => Some(DayOfWeek::Monday),
            "Tue" | "Tuesday" => Some(DayOfWeek::Tuesday),
            "Wed" | "Wednesday" => Some(DayOfWeek::Wednesday),
            "Thu" | "Thursday" => Some(DayOfWeek::Thursday),
            "Fri" | "Friday" => Some(DayOfWeek::Friday),
            "Sat" | "Saturday" => Some(DayOfWeek::Saturday),
            _ => None,
        }
    }
}

/// パース済み HTTP-date
///
/// RFC 9110 Section 5.6.7 に基づいた日時構造。
/// 3つの形式をパースできます:
/// - IMF-fixdate: Sun, 06 Nov 1994 08:49:37 GMT (推奨)
/// - RFC 850: Sunday, 06-Nov-94 08:49:37 GMT (廃止)
/// - ANSI C asctime: Sun Nov  6 08:49:37 1994 (廃止)
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HttpDate {
    /// 曜日
    day_of_week: DayOfWeek,
    /// 日 (1-31)
    day: u8,
    /// 月 (1-12)
    month: u8,
    /// 年 (4桁)
    year: u16,
    /// 時 (0-23)
    hour: u8,
    /// 分 (0-59)
    minute: u8,
    /// 秒 (0-60, 60はうるう秒)
    second: u8,
}

impl HttpDate {
    /// HTTP-date 文字列をパース
    ///
    /// 3つの形式をサポート:
    /// - IMF-fixdate: `Sun, 06 Nov 1994 08:49:37 GMT`
    /// - RFC 850: `Sunday, 06-Nov-94 08:49:37 GMT`
    /// - ANSI C: `Sun Nov  6 08:49:37 1994`
    pub fn parse(input: &str) -> Result<Self, DateError> {
        let input = input.trim();
        if input.is_empty() {
            return Err(DateError::Empty);
        }

        // カンマの位置で形式を判別
        if let Some(comma_pos) = input.find(',') {
            let day_name = &input[..comma_pos];
            let rest = input[comma_pos + 1..].trim_start();

            // IMF-fixdate: Sun, 06 Nov 1994 08:49:37 GMT
            // RFC 850: Sunday, 06-Nov-94 08:49:37 GMT
            if rest.contains('-') {
                parse_rfc850(day_name, rest)
            } else {
                parse_imf_fixdate(day_name, rest)
            }
        } else {
            // ANSI C asctime: Sun Nov  6 08:49:37 1994
            parse_asctime(input)
        }
    }

    /// 新しい HttpDate を作成
    pub fn new(
        day_of_week: DayOfWeek,
        day: u8,
        month: u8,
        year: u16,
        hour: u8,
        minute: u8,
        second: u8,
    ) -> Result<Self, DateError> {
        if !(1..=31).contains(&day) {
            return Err(DateError::InvalidDay);
        }
        if !(1..=12).contains(&month) {
            return Err(DateError::InvalidMonth);
        }
        if year < 1 {
            return Err(DateError::InvalidYear);
        }
        if hour > 23 {
            return Err(DateError::InvalidHour);
        }
        if minute > 59 {
            return Err(DateError::InvalidMinute);
        }
        if second > 60 {
            return Err(DateError::InvalidSecond);
        }

        Ok(HttpDate {
            day_of_week,
            day,
            month,
            year,
            hour,
            minute,
            second,
        })
    }

    /// 曜日を取得
    pub fn day_of_week(&self) -> DayOfWeek {
        self.day_of_week
    }

    /// 日を取得 (1-31)
    pub fn day(&self) -> u8 {
        self.day
    }

    /// 月を取得 (1-12)
    pub fn month(&self) -> u8 {
        self.month
    }

    /// 年を取得
    pub fn year(&self) -> u16 {
        self.year
    }

    /// 時を取得 (0-23)
    pub fn hour(&self) -> u8 {
        self.hour
    }

    /// 分を取得 (0-59)
    pub fn minute(&self) -> u8 {
        self.minute
    }

    /// 秒を取得 (0-60)
    pub fn second(&self) -> u8 {
        self.second
    }
}

impl fmt::Display for HttpDate {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // IMF-fixdate 形式で出力
        write!(
            f,
            "{}, {:02} {} {:04} {:02}:{:02}:{:02} GMT",
            self.day_of_week.short_name(),
            self.day,
            month_name(self.month),
            self.year,
            self.hour,
            self.minute,
            self.second
        )
    }
}

/// IMF-fixdate 形式をパース
/// 例: 06 Nov 1994 08:49:37 GMT
fn parse_imf_fixdate(day_name: &str, rest: &str) -> Result<HttpDate, DateError> {
    let day_of_week = DayOfWeek::from_name(day_name).ok_or(DateError::InvalidDayName)?;

    // "06 Nov 1994 08:49:37 GMT" をパース
    let parts: Vec<&str> = rest.split_whitespace().collect();
    if parts.len() != 5 {
        return Err(DateError::InvalidFormat);
    }

    let day = parts[0].parse::<u8>().map_err(|_| DateError::InvalidDay)?;
    let month = parse_month(parts[1])?;
    let year = parts[2]
        .parse::<u16>()
        .map_err(|_| DateError::InvalidYear)?;
    let (hour, minute, second) = parse_time(parts[3])?;

    if parts[4] != "GMT" {
        return Err(DateError::NotGmt);
    }

    HttpDate::new(day_of_week, day, month, year, hour, minute, second)
}

/// RFC 850 形式をパース
/// 例: 06-Nov-94 08:49:37 GMT
fn parse_rfc850(day_name: &str, rest: &str) -> Result<HttpDate, DateError> {
    let day_of_week = DayOfWeek::from_name(day_name).ok_or(DateError::InvalidDayName)?;

    // "06-Nov-94 08:49:37 GMT" をパース
    let parts: Vec<&str> = rest.split_whitespace().collect();
    if parts.len() != 3 {
        return Err(DateError::InvalidFormat);
    }

    // 日-月-年 をパース
    let date_parts: Vec<&str> = parts[0].split('-').collect();
    if date_parts.len() != 3 {
        return Err(DateError::InvalidFormat);
    }

    let day = date_parts[0]
        .parse::<u8>()
        .map_err(|_| DateError::InvalidDay)?;
    let month = parse_month(date_parts[1])?;
    let raw_year = date_parts[2]
        .parse::<u16>()
        .map_err(|_| DateError::InvalidYear)?;

    // 2 桁年の補正 (RFC 9110 Section 5.6.7)
    // 50 年以上未来に見える場合は 100 年引く
    let year = if raw_year < 100 {
        interpret_two_digit_year(raw_year)
    } else {
        raw_year
    };

    let (hour, minute, second) = parse_time(parts[1])?;

    if parts[2] != "GMT" {
        return Err(DateError::NotGmt);
    }

    HttpDate::new(day_of_week, day, month, year, hour, minute, second)
}

/// ANSI C asctime 形式をパース
/// 例: Sun Nov  6 08:49:37 1994
fn parse_asctime(input: &str) -> Result<HttpDate, DateError> {
    let parts: Vec<&str> = input.split_whitespace().collect();
    if parts.len() != 5 {
        return Err(DateError::InvalidFormat);
    }

    let day_of_week = DayOfWeek::from_name(parts[0]).ok_or(DateError::InvalidDayName)?;
    let month = parse_month(parts[1])?;
    let day = parts[2].parse::<u8>().map_err(|_| DateError::InvalidDay)?;
    let (hour, minute, second) = parse_time(parts[3])?;
    let year = parts[4]
        .parse::<u16>()
        .map_err(|_| DateError::InvalidYear)?;

    HttpDate::new(day_of_week, day, month, year, hour, minute, second)
}

/// 月名をパース
fn parse_month(s: &str) -> Result<u8, DateError> {
    match s {
        "Jan" => Ok(1),
        "Feb" => Ok(2),
        "Mar" => Ok(3),
        "Apr" => Ok(4),
        "May" => Ok(5),
        "Jun" => Ok(6),
        "Jul" => Ok(7),
        "Aug" => Ok(8),
        "Sep" => Ok(9),
        "Oct" => Ok(10),
        "Nov" => Ok(11),
        "Dec" => Ok(12),
        _ => Err(DateError::InvalidMonth),
    }
}

/// 月番号から月名を取得
fn month_name(month: u8) -> &'static str {
    match month {
        1 => "Jan",
        2 => "Feb",
        3 => "Mar",
        4 => "Apr",
        5 => "May",
        6 => "Jun",
        7 => "Jul",
        8 => "Aug",
        9 => "Sep",
        10 => "Oct",
        11 => "Nov",
        12 => "Dec",
        _ => "???",
    }
}

/// 現在の年を取得
#[cfg(not(test))]
fn current_year() -> u16 {
    use std::time::{SystemTime, UNIX_EPOCH};
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    // 1 年 ≈ 31,557,600 秒 (365.25 日)
    let years_since_1970 = now.as_secs() / 31_557_600;
    (1970 + years_since_1970) as u16
}

#[cfg(test)]
thread_local! {
    static CURRENT_YEAR_FOR_TEST: std::cell::Cell<u16> = const { std::cell::Cell::new(2026) };
}

#[cfg(test)]
fn current_year() -> u16 {
    CURRENT_YEAR_FOR_TEST.with(|y| y.get())
}

#[cfg(test)]
fn set_current_year_for_test(year: u16) {
    CURRENT_YEAR_FOR_TEST.with(|y| y.set(year));
}

/// 2 桁年を RFC 9110 準拠で解釈する
///
/// RFC 9110 Section 5.6.7:
/// 「Recipients of a timestamp value in rfc850-date format, which uses a
/// two-digit year, MUST interpret a timestamp that appears to be more than
/// 50 years in the future as representing the most recent year in the past
/// that had the same last two digits.」
fn interpret_two_digit_year(two_digit: u16) -> u16 {
    let current = current_year();
    let current_century = (current / 100) * 100;
    let candidate = current_century + two_digit;

    // 50 年以上未来なら 100 年引く
    if candidate > current + 50 {
        candidate - 100
    } else {
        candidate
    }
}

/// 時刻をパース (HH:MM:SS)
fn parse_time(s: &str) -> Result<(u8, u8, u8), DateError> {
    let parts: Vec<&str> = s.split(':').collect();
    if parts.len() != 3 {
        return Err(DateError::InvalidFormat);
    }

    let hour = parts[0].parse::<u8>().map_err(|_| DateError::InvalidHour)?;
    let minute = parts[1]
        .parse::<u8>()
        .map_err(|_| DateError::InvalidMinute)?;
    let second = parts[2]
        .parse::<u8>()
        .map_err(|_| DateError::InvalidSecond)?;

    if hour > 23 {
        return Err(DateError::InvalidHour);
    }
    if minute > 59 {
        return Err(DateError::InvalidMinute);
    }
    if second > 60 {
        return Err(DateError::InvalidSecond);
    }

    Ok((hour, minute, second))
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn test_parse_rfc850() {
        let date = HttpDate::parse("Sunday, 06-Nov-94 08:49:37 GMT").unwrap();
        assert_eq!(date.day_of_week(), DayOfWeek::Sunday);
        assert_eq!(date.day(), 6);
        assert_eq!(date.month(), 11);
        assert_eq!(date.year(), 1994);
        assert_eq!(date.hour(), 8);
        assert_eq!(date.minute(), 49);
        assert_eq!(date.second(), 37);
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
        // RFC 9110 Section 5.6.7:
        // 50 年以上未来に見える場合は 100 年引く

        // 現在年を 2026 に設定 (デフォルト)
        set_current_year_for_test(2026);

        // 20 → 2020 (2026 + 50 = 2076 > 2020 なので 2020)
        let date = HttpDate::parse("Sunday, 06-Nov-20 08:49:37 GMT").unwrap();
        assert_eq!(date.year(), 2020);

        // 76 → 2076 (2026 + 50 = 2076 >= 2076 なので 2076)
        let date = HttpDate::parse("Sunday, 06-Nov-76 08:49:37 GMT").unwrap();
        assert_eq!(date.year(), 2076);

        // 77 → 1977 (2026 + 50 = 2076 < 2077 なので 100 年引いて 1977)
        let date = HttpDate::parse("Sunday, 06-Nov-77 08:49:37 GMT").unwrap();
        assert_eq!(date.year(), 1977);

        // 99 → 1999 (2026 + 50 = 2076 < 2099 なので 100 年引いて 1999)
        let date = HttpDate::parse("Sunday, 06-Nov-99 08:49:37 GMT").unwrap();
        assert_eq!(date.year(), 1999);
    }

    #[test]
    fn test_parse_rfc850_2digit_year_boundary() {
        // 境界テスト: 異なる基準年での動作確認

        // 基準年 2050 の場合
        set_current_year_for_test(2050);

        // 00 → 2000 (candidate = 2000, 2000 > 2050 + 50 = 2100? No → 2000)
        let date = HttpDate::parse("Sunday, 06-Nov-00 08:49:37 GMT").unwrap();
        assert_eq!(date.year(), 2000);

        // 01 → 2001 (candidate = 2001, 2001 > 2100? No → 2001)
        let date = HttpDate::parse("Sunday, 06-Nov-01 08:49:37 GMT").unwrap();
        assert_eq!(date.year(), 2001);

        // 50 → 2050 (candidate = 2050, 2050 > 2100? No → 2050)
        let date = HttpDate::parse("Sunday, 06-Nov-50 08:49:37 GMT").unwrap();
        assert_eq!(date.year(), 2050);

        // テスト後にデフォルトに戻す
        set_current_year_for_test(2026);
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
        // うるう秒 (60秒) は許可
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
}
