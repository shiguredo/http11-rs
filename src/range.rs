//! Range リクエストヘッダー (RFC 9110)
//!
//! ## 概要
//!
//! RFC 9110 Section 14 に基づいた Range リクエストヘッダーのパースを提供します。
//!
//! ## 使い方
//!
//! ```rust
//! use shiguredo_http11::range::{Range, ContentRange, AcceptRanges};
//!
//! // Range ヘッダーパース
//! let range = Range::parse("bytes=0-499").unwrap();
//! assert_eq!(range.unit(), "bytes");
//! let specs = range.ranges();
//! assert_eq!(specs.len(), 1);
//!
//! // Content-Range ヘッダー生成
//! let cr = ContentRange::new_bytes(0, 499, Some(1000));
//! assert_eq!(cr.to_string(), "bytes 0-499/1000");
//!
//! // Accept-Ranges ヘッダーパース
//! let ar = AcceptRanges::parse("bytes").unwrap();
//! assert!(ar.accepts_bytes());
//! ```

use core::fmt;

/// Range パースエラー
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RangeError {
    /// 空の入力
    Empty,
    /// 不正な形式
    InvalidFormat,
    /// 不正な単位
    InvalidUnit,
    /// 不正な範囲
    InvalidRange,
    /// 範囲が不正 (開始 > 終了)
    InvalidBounds,
}

impl fmt::Display for RangeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RangeError::Empty => write!(f, "empty range header"),
            RangeError::InvalidFormat => write!(f, "invalid range header format"),
            RangeError::InvalidUnit => write!(f, "invalid range unit"),
            RangeError::InvalidRange => write!(f, "invalid range specification"),
            RangeError::InvalidBounds => write!(f, "invalid range bounds (start > end)"),
        }
    }
}

impl std::error::Error for RangeError {}

/// 範囲指定
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RangeSpec {
    /// 開始位置から終了位置まで (両端含む)
    /// bytes=0-499 → Range { start: 0, end: 499 }
    Range { start: u64, end: u64 },
    /// 開始位置から末尾まで
    /// bytes=500- → FromStart { start: 500 }
    FromStart { start: u64 },
    /// 末尾から n バイト
    /// bytes=-500 → Suffix { length: 500 }
    Suffix { length: u64 },
}

impl RangeSpec {
    /// 実際のバイト範囲を計算
    ///
    /// total_length はリソースの総バイト数
    /// 戻り値は (start, end) で両端含む
    pub fn to_bounds(&self, total_length: u64) -> Option<(u64, u64)> {
        if total_length == 0 {
            return None;
        }
        match *self {
            RangeSpec::Range { start, end } => {
                if start > end || start >= total_length {
                    return None;
                }
                let end = end.min(total_length - 1);
                Some((start, end))
            }
            RangeSpec::FromStart { start } => {
                if start >= total_length {
                    return None;
                }
                Some((start, total_length - 1))
            }
            RangeSpec::Suffix { length } => {
                if length == 0 {
                    return None;
                }
                let start = total_length.saturating_sub(length);
                Some((start, total_length - 1))
            }
        }
    }
}

impl fmt::Display for RangeSpec {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RangeSpec::Range { start, end } => write!(f, "{}-{}", start, end),
            RangeSpec::FromStart { start } => write!(f, "{}-", start),
            RangeSpec::Suffix { length } => write!(f, "-{}", length),
        }
    }
}

/// Range ヘッダー (RFC 9110 Section 14.2)
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Range {
    /// 範囲単位 (通常は "bytes")
    unit: String,
    /// 範囲指定のリスト
    ranges: Vec<RangeSpec>,
}

impl Range {
    /// Range ヘッダーをパース
    ///
    /// # 例
    ///
    /// ```rust
    /// use shiguredo_http11::range::Range;
    ///
    /// // 単一範囲
    /// let range = Range::parse("bytes=0-499").unwrap();
    /// assert_eq!(range.unit(), "bytes");
    ///
    /// // 複数範囲
    /// let range = Range::parse("bytes=0-499, 1000-1499").unwrap();
    /// assert_eq!(range.ranges().len(), 2);
    ///
    /// // 末尾から
    /// let range = Range::parse("bytes=-500").unwrap();
    /// ```
    pub fn parse(input: &str) -> Result<Self, RangeError> {
        let input = input.trim();
        if input.is_empty() {
            return Err(RangeError::Empty);
        }

        // unit=ranges の形式
        let eq_pos = input.find('=').ok_or(RangeError::InvalidFormat)?;
        let unit = input[..eq_pos].trim();
        let ranges_str = input[eq_pos + 1..].trim();

        if unit.is_empty() {
            return Err(RangeError::InvalidUnit);
        }

        let mut ranges = Vec::new();
        for part in ranges_str.split(',') {
            let part = part.trim();
            if part.is_empty() {
                continue;
            }
            ranges.push(parse_range_spec(part)?);
        }

        if ranges.is_empty() {
            return Err(RangeError::Empty);
        }

        Ok(Range {
            unit: unit.to_string(),
            ranges,
        })
    }

    /// 単位を取得
    pub fn unit(&self) -> &str {
        &self.unit
    }

    /// バイト範囲かどうか
    pub fn is_bytes(&self) -> bool {
        self.unit.eq_ignore_ascii_case("bytes")
    }

    /// 範囲指定のリストを取得
    pub fn ranges(&self) -> &[RangeSpec] {
        &self.ranges
    }

    /// 最初の範囲を取得
    pub fn first(&self) -> Option<&RangeSpec> {
        self.ranges.first()
    }
}

impl fmt::Display for Range {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}=", self.unit)?;
        let specs: Vec<String> = self.ranges.iter().map(|r| r.to_string()).collect();
        write!(f, "{}", specs.join(", "))
    }
}

/// 範囲指定をパース
fn parse_range_spec(s: &str) -> Result<RangeSpec, RangeError> {
    let dash_pos = s.find('-').ok_or(RangeError::InvalidRange)?;

    let start_str = s[..dash_pos].trim();
    let end_str = s[dash_pos + 1..].trim();

    if start_str.is_empty() && end_str.is_empty() {
        return Err(RangeError::InvalidRange);
    }

    if start_str.is_empty() {
        // Suffix: -500
        let length = end_str
            .parse::<u64>()
            .map_err(|_| RangeError::InvalidRange)?;
        return Ok(RangeSpec::Suffix { length });
    }

    let start = start_str
        .parse::<u64>()
        .map_err(|_| RangeError::InvalidRange)?;

    if end_str.is_empty() {
        // FromStart: 500-
        return Ok(RangeSpec::FromStart { start });
    }

    // Range: 0-499
    let end = end_str
        .parse::<u64>()
        .map_err(|_| RangeError::InvalidRange)?;

    if start > end {
        return Err(RangeError::InvalidBounds);
    }

    Ok(RangeSpec::Range { start, end })
}

/// Content-Range ヘッダー (RFC 9110 Section 14.4)
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContentRange {
    /// 範囲単位
    unit: String,
    /// 開始位置
    start: Option<u64>,
    /// 終了位置
    end: Option<u64>,
    /// 完全な長さ (不明な場合は None)
    complete_length: Option<u64>,
}

impl ContentRange {
    /// Content-Range ヘッダーをパース
    pub fn parse(input: &str) -> Result<Self, RangeError> {
        let input = input.trim();
        if input.is_empty() {
            return Err(RangeError::Empty);
        }

        // unit range/length の形式
        let space_pos = input.find(' ').ok_or(RangeError::InvalidFormat)?;
        let unit = input[..space_pos].trim();
        let rest = input[space_pos + 1..].trim();

        if unit.is_empty() {
            return Err(RangeError::InvalidUnit);
        }

        // range/length
        let slash_pos = rest.find('/').ok_or(RangeError::InvalidFormat)?;
        let range_str = rest[..slash_pos].trim();
        let length_str = rest[slash_pos + 1..].trim();

        let complete_length = if length_str == "*" {
            None
        } else {
            Some(
                length_str
                    .parse::<u64>()
                    .map_err(|_| RangeError::InvalidFormat)?,
            )
        };

        if range_str == "*" {
            // unsatisfied range: bytes */1000
            return Ok(ContentRange {
                unit: unit.to_string(),
                start: None,
                end: None,
                complete_length,
            });
        }

        // start-end
        let dash_pos = range_str.find('-').ok_or(RangeError::InvalidFormat)?;
        let start = range_str[..dash_pos]
            .parse::<u64>()
            .map_err(|_| RangeError::InvalidFormat)?;
        let end = range_str[dash_pos + 1..]
            .parse::<u64>()
            .map_err(|_| RangeError::InvalidFormat)?;

        if start > end {
            return Err(RangeError::InvalidBounds);
        }

        Ok(ContentRange {
            unit: unit.to_string(),
            start: Some(start),
            end: Some(end),
            complete_length,
        })
    }

    /// 新しい Content-Range を作成 (bytes)
    pub fn new_bytes(start: u64, end: u64, complete_length: Option<u64>) -> Self {
        ContentRange {
            unit: "bytes".to_string(),
            start: Some(start),
            end: Some(end),
            complete_length,
        }
    }

    /// 範囲が満たせない場合の Content-Range (bytes */total)
    pub fn unsatisfied(unit: &str, complete_length: u64) -> Self {
        ContentRange {
            unit: unit.to_string(),
            start: None,
            end: None,
            complete_length: Some(complete_length),
        }
    }

    /// 単位を取得
    pub fn unit(&self) -> &str {
        &self.unit
    }

    /// 開始位置を取得
    pub fn start(&self) -> Option<u64> {
        self.start
    }

    /// 終了位置を取得
    pub fn end(&self) -> Option<u64> {
        self.end
    }

    /// 完全な長さを取得
    pub fn complete_length(&self) -> Option<u64> {
        self.complete_length
    }

    /// 範囲の長さを取得
    pub fn length(&self) -> Option<u64> {
        match (self.start, self.end) {
            (Some(s), Some(e)) => Some(e - s + 1),
            _ => None,
        }
    }

    /// 範囲が満たせないかどうか
    pub fn is_unsatisfied(&self) -> bool {
        self.start.is_none() && self.end.is_none()
    }
}

impl fmt::Display for ContentRange {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match (self.start, self.end) {
            (Some(s), Some(e)) => {
                write!(f, "{} {}-{}/", self.unit, s, e)?;
            }
            _ => {
                write!(f, "{} */", self.unit)?;
            }
        }
        match self.complete_length {
            Some(len) => write!(f, "{}", len),
            None => write!(f, "*"),
        }
    }
}

/// Accept-Ranges ヘッダー (RFC 9110 Section 14.3)
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AcceptRanges {
    /// 受け入れる範囲単位のリスト
    units: Vec<String>,
}

impl AcceptRanges {
    /// Accept-Ranges ヘッダーをパース
    pub fn parse(input: &str) -> Result<Self, RangeError> {
        let input = input.trim();
        if input.is_empty() {
            return Err(RangeError::Empty);
        }

        let units: Vec<String> = input
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();

        if units.is_empty() {
            return Err(RangeError::Empty);
        }

        Ok(AcceptRanges { units })
    }

    /// bytes を作成
    pub fn bytes() -> Self {
        AcceptRanges {
            units: vec!["bytes".to_string()],
        }
    }

    /// none を作成
    pub fn none() -> Self {
        AcceptRanges {
            units: vec!["none".to_string()],
        }
    }

    /// 単位のリストを取得
    pub fn units(&self) -> &[String] {
        &self.units
    }

    /// bytes を受け入れるかどうか
    pub fn accepts_bytes(&self) -> bool {
        self.units.iter().any(|u| u.eq_ignore_ascii_case("bytes"))
    }

    /// 何も受け入れないかどうか
    pub fn is_none(&self) -> bool {
        self.units.len() == 1 && self.units[0].eq_ignore_ascii_case("none")
    }
}

impl fmt::Display for AcceptRanges {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.units.join(", "))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_range_single() {
        let range = Range::parse("bytes=0-499").unwrap();
        assert_eq!(range.unit(), "bytes");
        assert!(range.is_bytes());
        let specs = range.ranges();
        assert_eq!(specs.len(), 1);
        match specs[0] {
            RangeSpec::Range { start, end } => {
                assert_eq!(start, 0);
                assert_eq!(end, 499);
            }
            _ => panic!("expected Range"),
        }
    }

    #[test]
    fn test_parse_range_multiple() {
        let range = Range::parse("bytes=0-499, 1000-1499").unwrap();
        assert_eq!(range.ranges().len(), 2);
    }

    #[test]
    fn test_parse_range_suffix() {
        let range = Range::parse("bytes=-500").unwrap();
        match range.first().unwrap() {
            RangeSpec::Suffix { length } => assert_eq!(*length, 500),
            _ => panic!("expected Suffix"),
        }
    }

    #[test]
    fn test_parse_range_from_start() {
        let range = Range::parse("bytes=500-").unwrap();
        match range.first().unwrap() {
            RangeSpec::FromStart { start } => assert_eq!(*start, 500),
            _ => panic!("expected FromStart"),
        }
    }

    #[test]
    fn test_parse_range_invalid_bounds() {
        assert!(Range::parse("bytes=500-100").is_err());
    }

    #[test]
    fn test_range_spec_to_bounds() {
        let total = 1000;

        // 0-499
        let spec = RangeSpec::Range { start: 0, end: 499 };
        assert_eq!(spec.to_bounds(total), Some((0, 499)));

        // 500-
        let spec = RangeSpec::FromStart { start: 500 };
        assert_eq!(spec.to_bounds(total), Some((500, 999)));

        // -200
        let spec = RangeSpec::Suffix { length: 200 };
        assert_eq!(spec.to_bounds(total), Some((800, 999)));

        // 範囲外
        let spec = RangeSpec::Range {
            start: 1000,
            end: 1500,
        };
        assert_eq!(spec.to_bounds(total), None);
    }

    #[test]
    fn test_range_display() {
        let range = Range::parse("bytes=0-499, 1000-1499").unwrap();
        assert_eq!(range.to_string(), "bytes=0-499, 1000-1499");
    }

    #[test]
    fn test_content_range_parse() {
        let cr = ContentRange::parse("bytes 0-499/1000").unwrap();
        assert_eq!(cr.unit(), "bytes");
        assert_eq!(cr.start(), Some(0));
        assert_eq!(cr.end(), Some(499));
        assert_eq!(cr.complete_length(), Some(1000));
        assert_eq!(cr.length(), Some(500));
    }

    #[test]
    fn test_content_range_unknown_length() {
        let cr = ContentRange::parse("bytes 0-499/*").unwrap();
        assert_eq!(cr.complete_length(), None);
    }

    #[test]
    fn test_content_range_unsatisfied() {
        let cr = ContentRange::parse("bytes */1000").unwrap();
        assert!(cr.is_unsatisfied());
        assert_eq!(cr.complete_length(), Some(1000));
    }

    #[test]
    fn test_content_range_display() {
        let cr = ContentRange::new_bytes(0, 499, Some(1000));
        assert_eq!(cr.to_string(), "bytes 0-499/1000");

        let cr = ContentRange::unsatisfied("bytes", 1000);
        assert_eq!(cr.to_string(), "bytes */1000");
    }

    #[test]
    fn test_accept_ranges_bytes() {
        let ar = AcceptRanges::parse("bytes").unwrap();
        assert!(ar.accepts_bytes());
        assert!(!ar.is_none());
    }

    #[test]
    fn test_accept_ranges_none() {
        let ar = AcceptRanges::parse("none").unwrap();
        assert!(ar.is_none());
        assert!(!ar.accepts_bytes());
    }

    #[test]
    fn test_accept_ranges_display() {
        let ar = AcceptRanges::bytes();
        assert_eq!(ar.to_string(), "bytes");
    }
}
