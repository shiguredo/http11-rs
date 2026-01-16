//! Expect ヘッダーパース (RFC 9110 Section 10.1.1)
//!
//! ## 概要
//!
//! RFC 9110 に基づいた Expect ヘッダーのパースを提供します。
//!
//! ## 使い方
//!
//! ```rust
//! use shiguredo_http11::expect::Expect;
//!
//! let expect = Expect::parse("100-continue").unwrap();
//! assert!(expect.has_100_continue());
//! ```

use core::fmt;

/// Expect パースエラー
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExpectError {
    /// 空の入力
    Empty,
    /// 不正な形式
    InvalidFormat,
    /// 不正なトークン
    InvalidToken,
    /// 不正な値
    InvalidValue,
}

impl fmt::Display for ExpectError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ExpectError::Empty => write!(f, "empty Expect header"),
            ExpectError::InvalidFormat => write!(f, "invalid Expect header format"),
            ExpectError::InvalidToken => write!(f, "invalid Expect token"),
            ExpectError::InvalidValue => write!(f, "invalid Expect value"),
        }
    }
}

impl std::error::Error for ExpectError {}

/// Expect ヘッダー
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Expect {
    items: Vec<Expectation>,
}

impl Expect {
    /// Expect ヘッダーをパース
    pub fn parse(input: &str) -> Result<Self, ExpectError> {
        let input = input.trim();
        if input.is_empty() {
            return Err(ExpectError::Empty);
        }

        let mut items = Vec::new();
        for part in split_with_quotes(input, ',') {
            let part = part.trim();
            if part.is_empty() {
                return Err(ExpectError::InvalidFormat);
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

        if items.is_empty() {
            return Err(ExpectError::Empty);
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
    } else {
        if !is_valid_token(input) {
            return Err(ExpectError::InvalidValue);
        }
        Ok(input.to_string())
    }
}

fn parse_quoted_string(input: &str) -> Result<(String, &str), ExpectError> {
    let mut result = String::new();
    let mut escaped = false;

    for (i, c) in input.char_indices() {
        if escaped {
            result.push(c);
            escaped = false;
        } else if c == '\\' {
            escaped = true;
        } else if c == '"' {
            return Ok((result, &input[i + 1..]));
        } else {
            result.push(c);
        }
    }

    Err(ExpectError::InvalidValue)
}

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

fn is_valid_token(s: &str) -> bool {
    !s.is_empty() && s.bytes().all(is_token_char)
}

fn is_token_char(b: u8) -> bool {
    matches!(
        b,
        b'!' | b'#' | b'$' | b'%' | b'&' | b'\'' | b'*' | b'+' | b'-' | b'.' |
        b'0'..=b'9' | b'A'..=b'Z' | b'^' | b'_' | b'`' | b'a'..=b'z' | b'|' | b'~'
    )
}

fn needs_quoting(s: &str) -> bool {
    // 空文字列は引用符が必要
    s.is_empty() || s.bytes().any(|b| !is_token_char(b))
}

fn escape_quotes(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"")
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
        assert!(Expect::parse("").is_err());
        assert!(Expect::parse("bad value").is_err());
        assert!(Expect::parse("token=").is_err());
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
