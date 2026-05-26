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

use crate::validate::{trim_ows, trim_ows_start};
use alloc::vec::Vec;
use core::fmt;

/// HTTP-date パースエラー
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
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
    /// rfc850-date (obs-date) 形式である
    ///
    /// `HttpDate::parse` は IMF-fixdate と asctime のみ受理する。
    /// rfc850-date は 2 桁年の解決に基準年が必要なため、
    /// [`HttpDate::parse_rfc850`] でフォールバックすること。
    Rfc850Date,
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
            DateError::Rfc850Date => write!(f, "rfc850-date format requires reference year"),
            DateError::InvalidHour => write!(f, "invalid hour"),
            DateError::InvalidMinute => write!(f, "invalid minute"),
            DateError::InvalidSecond => write!(f, "invalid second"),
            DateError::NotGmt => write!(f, "timezone is not GMT"),
        }
    }
}

impl core::error::Error for DateError {}

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
/// - RFC 850: Sunday, 06-Nov-94 08:49:37 GMT (obs-date, 廃止)
/// - ANSI C asctime: Sun Nov  6 08:49:37 1994 (obs-date, 廃止)
///
/// 注: RFC 850 / asctime 形式 (obs-date) は一般的に使われていないが、
/// RFC 9110 Section 5.6.7 の受信者要件 (MUST accept) に準拠するために実装している。
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

impl PartialOrd for HttpDate {
    fn partial_cmp(&self, other: &Self) -> Option<core::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for HttpDate {
    fn cmp(&self, other: &Self) -> core::cmp::Ordering {
        (
            self.year,
            self.month,
            self.day,
            self.hour,
            self.minute,
            self.second,
        )
            .cmp(&(
                other.year,
                other.month,
                other.day,
                other.hour,
                other.minute,
                other.second,
            ))
    }
}

/// 月と年からその月の最大日数を返す
fn max_day_in_month(month: u8, year: u16) -> u8 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 => {
            let leap =
                year.is_multiple_of(4) && (!year.is_multiple_of(100) || year.is_multiple_of(400));
            if leap { 29 } else { 28 }
        }
        _ => 0,
    }
}

impl HttpDate {
    /// HTTP-date 文字列をパース (IMF-fixdate / asctime)
    ///
    /// 4 桁年を持つ 2 形式のみを受理する:
    /// - IMF-fixdate: `Sun, 06 Nov 1994 08:49:37 GMT`
    /// - ANSI C asctime: `Sun Nov  6 08:49:37 1994`
    ///
    /// rfc850-date (`Sunday, 06-Nov-94 ...`) を検出した場合は
    /// `Err(DateError::Rfc850Date)` を返す。RFC 9110 §5.6.7 は受信者に
    /// 3 形式すべての受理を MUST しているため、完全適合が必要な箇所は
    /// このエラーを検知して [`HttpDate::parse_rfc850`] にフォールバック
    /// すること。
    ///
    /// rfc850-date の 2 桁年解決には基準年が必要だが、no_std ではシステム
    /// 時刻を取得できないため、ライブラリ側で基準年を持たず呼び出し側に
    /// 委ねる設計としている。
    pub fn parse(input: &str) -> Result<Self, DateError> {
        let input = trim_ows(input);
        if input.is_empty() {
            return Err(DateError::Empty);
        }

        // カンマの位置で形式を判別
        if let Some(comma_pos) = input.find(',') {
            let day_name = &input[..comma_pos];
            let rest = trim_ows_start(&input[comma_pos + 1..]);

            // IMF-fixdate: Sun, 06 Nov 1994 08:49:37 GMT
            // rfc850-date: Sunday, 06-Nov-94 08:49:37 GMT
            if rest.contains('-') {
                Err(DateError::Rfc850Date)
            } else {
                parse_imf_fixdate(day_name, rest)
            }
        } else {
            // ANSI C asctime: Sun Nov  6 08:49:37 1994
            parse_asctime(input)
        }
    }

    /// rfc850-date 形式 (obs-date) をパース (RFC 9110 §5.6.7)
    ///
    /// 例: `Sunday, 06-Nov-94 08:49:37 GMT`
    ///
    /// 2 桁年は RFC 9110 §5.6.7 の規則で 4 桁年に解決する:
    /// 「`reference_year + 50` より未来に見える 2 桁年は最も近い過去の
    /// 同末尾年と解釈する」。`reference_year` には呼び出し側が把握して
    /// いる現在年 (RTC / NTP / ビルド時の埋め込み年など) を渡すこと。
    ///
    /// IMF-fixdate / asctime はこの関数では受理しない。3 形式すべてを
    /// 受理したい場合は [`HttpDate::parse`] を先に試し、`Rfc850Date`
    /// エラーで本関数にフォールバックする。
    pub fn parse_rfc850(input: &str, reference_year: u16) -> Result<Self, DateError> {
        let input = trim_ows(input);
        if input.is_empty() {
            return Err(DateError::Empty);
        }

        let comma_pos = input.find(',').ok_or(DateError::InvalidFormat)?;
        let day_name = &input[..comma_pos];
        let rest = trim_ows_start(&input[comma_pos + 1..]);

        if !rest.contains('-') {
            return Err(DateError::InvalidFormat);
        }

        parse_rfc850_inner(day_name, rest, reference_year)
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
        if !(1..=12).contains(&month) {
            return Err(DateError::InvalidMonth);
        }
        let max_day = max_day_in_month(month, year);
        if day < 1 || day > max_day {
            return Err(DateError::InvalidDay);
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

/// rfc850-date 形式をパース (内部実装)
/// 例: 06-Nov-94 08:49:37 GMT
fn parse_rfc850_inner(
    day_name: &str,
    rest: &str,
    reference_year: u16,
) -> Result<HttpDate, DateError> {
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
    // RFC 9110 §5.6.7 ABNF では date2 = day "-" month "-" 2DIGIT で
    // 年は 2 桁固定だが、Postel 原則に従い 4 桁年も受理する。
    // 4 桁年は曖昧さがないのでそのまま使い、reference_year は無視する。
    let raw_year_str = date_parts[2];
    let raw_year = raw_year_str
        .parse::<u16>()
        .map_err(|_| DateError::InvalidYear)?;
    let year = if raw_year_str.len() == 2 {
        interpret_two_digit_year(raw_year, reference_year)
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

/// 2 桁年を RFC 9110 準拠で解釈する
///
/// RFC 9110 Section 5.6.7:
/// 「Recipients of a timestamp value in rfc850-date format, which uses a
/// two-digit year, MUST interpret a timestamp that appears to be more than
/// 50 years in the future as representing the most recent year in the past
/// that had the same last two digits.」
fn interpret_two_digit_year(two_digit: u16, reference_year: u16) -> u16 {
    let current_century = (reference_year / 100) * 100;
    let candidate = current_century + two_digit;

    // 50 年以上未来なら 100 年引く
    if candidate > reference_year + 50 {
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
