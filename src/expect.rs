//! Expect ヘッダーパース (RFC 9110 Section 10.1.1)
//!
//! ## 概要
//!
//! RFC 9110 に基づいた Expect ヘッダーのパースを提供します。
//!
//! ## ABNF
//!
//! ```text
//! Expect      = #expectation
//! expectation = token [ "=" ( token / quoted-string ) parameters ]
//! ```
//!
//! ## 制限事項
//!
//! RFC 9110 の ABNF では expectation に parameters (セミコロン区切りの name=value ペア) が
//! 許容されていますが、本実装では parameters をサポートしていません。
//! RFC 9110 で定義されている唯一の expectation は "100-continue" であり、
//! パラメータは定義されていないためです。
//!
//! ## 使い方
//!
//! ```rust
//! use shiguredo_http11::expect::Expect;
//!
//! let expect = Expect::parse("100-continue").unwrap();
//! assert!(expect.has_100_continue());
//! ```

use alloc::string::{String, ToString};
use alloc::vec::Vec;
use core::fmt;

use crate::validate::{
    QuotedStringError, escape_quotes, is_token_char, is_valid_token, parse_quoted_string,
};

/// Expect パースエラー
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum ExpectError {
    /// 空の入力
    Empty,
    /// 不正な形式
    InvalidFormat,
    /// 不正なトークン
    InvalidToken,
    /// 不正な値 (qdtext / quoted-pair の文字種違反を含む)
    InvalidValue,
    /// quoted-string の閉じ DQUOTE が見つからない (RFC 9110 Section 5.6.4)
    UnterminatedQuote,
}

impl fmt::Display for ExpectError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ExpectError::Empty => write!(f, "empty Expect header"),
            ExpectError::InvalidFormat => write!(f, "invalid Expect header format"),
            ExpectError::InvalidToken => write!(f, "invalid Expect token"),
            ExpectError::InvalidValue => write!(f, "invalid Expect value"),
            ExpectError::UnterminatedQuote => write!(f, "unterminated quoted-string"),
        }
    }
}

impl core::error::Error for ExpectError {}

impl From<QuotedStringError> for ExpectError {
    fn from(e: QuotedStringError) -> Self {
        match e {
            QuotedStringError::InvalidQdtext | QuotedStringError::InvalidQuotedPair => {
                ExpectError::InvalidValue
            }
            QuotedStringError::Unterminated => ExpectError::UnterminatedQuote,
        }
    }
}

/// Expect ヘッダー
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Expect {
    items: Vec<Expectation>,
}

impl Expect {
    /// Expect ヘッダーをパース
    ///
    /// RFC 9110 Section 5.6.1.2: 空フィールド値・空要素は受理する
    pub fn parse(input: &str) -> Result<Self, ExpectError> {
        let input = input.trim();

        let mut items = Vec::new();
        for part in split_with_quotes(input, ',') {
            let part = part.trim();
            // RFC 9110 Section 5.6.1.2: 空要素は無視する
            if part.is_empty() {
                continue;
            }

            let (token, value) = if let Some((token, value)) = part.split_once('=') {
                let token = token.trim();
                if token.is_empty() {
                    return Err(ExpectError::InvalidFormat);
                }
                let value = parse_value(value)?;
                (token, Some(value))
            } else {
                (part, None)
            };

            if !is_valid_token(token) {
                return Err(ExpectError::InvalidToken);
            }

            items.push(Expectation {
                token: token.to_ascii_lowercase(),
                value,
            });
        }

        Ok(Expect { items })
    }

    /// Expectation 一覧
    pub fn items(&self) -> &[Expectation] {
        &self.items
    }

    /// 100-continue が含まれるかどうか
    pub fn has_100_continue(&self) -> bool {
        self.items
            .iter()
            .any(|item| item.token.eq_ignore_ascii_case("100-continue"))
    }
}

impl fmt::Display for Expect {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let values: Vec<String> = self.items.iter().map(|item| item.to_string()).collect();
        write!(f, "{}", values.join(", "))
    }
}

/// Expectation
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Expectation {
    token: String,
    value: Option<String>,
}

impl Expectation {
    /// トークン
    pub fn token(&self) -> &str {
        &self.token
    }

    /// 値
    pub fn value(&self) -> Option<&str> {
        self.value.as_deref()
    }

    /// 100-continue かどうか
    pub fn is_100_continue(&self) -> bool {
        self.token.eq_ignore_ascii_case("100-continue")
    }
}

impl fmt::Display for Expectation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.value {
            Some(value) => {
                if needs_quoting(value) {
                    write!(f, "{}=\"{}\"", self.token, escape_quotes(value))
                } else {
                    write!(f, "{}={}", self.token, value)
                }
            }
            None => write!(f, "{}", self.token),
        }
    }
}

fn parse_value(input: &str) -> Result<String, ExpectError> {
    let input = input.trim();
    if input.is_empty() {
        return Err(ExpectError::InvalidValue);
    }

    if let Some(rest) = input.strip_prefix('"') {
        let (value, remaining) = parse_quoted_string(rest)?;
        if !remaining.trim().is_empty() {
            return Err(ExpectError::InvalidValue);
        }
        Ok(value)
    } else if !is_valid_token(input) {
        Err(ExpectError::InvalidValue)
    } else {
        Ok(input.to_string())
    }
}

// 引用符付き文字列のパースは `validate::parse_quoted_string` に委譲する。
// `From<QuotedStringError> for ExpectError` で文字種違反は `InvalidValue`、
// 終端引用符なしは `UnterminatedQuote` にマップする。

fn split_with_quotes(input: &str, delimiter: char) -> Vec<String> {
    let mut parts = Vec::new();
    let mut start = 0;
    let mut in_quote = false;
    let mut escaped = false;

    for (i, c) in input.char_indices() {
        if escaped {
            escaped = false;
            continue;
        }
        if c == '\\' && in_quote {
            escaped = true;
            continue;
        }
        if c == '"' {
            in_quote = !in_quote;
            continue;
        }
        if c == delimiter && !in_quote {
            parts.push(input[start..i].to_string());
            start = i + c.len_utf8();
        }
    }
    parts.push(input[start..].to_string());
    parts
}

fn needs_quoting(s: &str) -> bool {
    // 空文字列は引用符が必要
    s.is_empty() || s.bytes().any(|b| !is_token_char(b))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_simple() {
        let expect = Expect::parse("100-continue").unwrap();
        assert!(expect.has_100_continue());
        assert_eq!(expect.items().len(), 1);
    }

    #[test]
    fn parse_extension() {
        let expect = Expect::parse("foo=bar, 100-continue").unwrap();
        assert_eq!(expect.items().len(), 2);
        assert_eq!(expect.items()[0].token(), "foo");
        assert_eq!(expect.items()[0].value(), Some("bar"));
    }

    #[test]
    fn parse_quoted_value() {
        let expect = Expect::parse("token=\"va\\\\lue\"").unwrap();
        assert_eq!(expect.items()[0].value(), Some("va\\lue"));
    }

    #[test]
    fn parse_invalid() {
        assert!(Expect::parse("bad value").is_err());
        assert!(Expect::parse("token=").is_err());
    }

    /// RFC 9110 Section 5.6.1.2: 空フィールド値・空要素は受理する
    #[test]
    fn parse_empty_elements() {
        let expect = Expect::parse("").unwrap();
        assert!(expect.items().is_empty());

        let expect = Expect::parse(",").unwrap();
        assert!(expect.items().is_empty());

        let expect = Expect::parse("100-continue,,foo=bar").unwrap();
        assert_eq!(expect.items().len(), 2);
    }

    #[test]
    fn display() {
        let expect = Expect::parse("foo=bar, 100-continue").unwrap();
        assert_eq!(expect.to_string(), "foo=bar, 100-continue");
    }

    #[test]
    fn empty_value_roundtrip() {
        // 空の値は引用符で囲む必要がある
        let expect = Expect::parse("token=\"\"").unwrap();
        assert_eq!(expect.items()[0].value(), Some(""));
        let displayed = expect.to_string();
        assert_eq!(displayed, "token=\"\"");
        let reparsed = Expect::parse(&displayed).unwrap();
        assert_eq!(expect, reparsed);
    }
}
