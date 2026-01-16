//! Upgrade ヘッダーパース (RFC 9110 Section 7.8)
//!
//! ## 概要
//!
//! RFC 9110 に基づいた Upgrade ヘッダーのパースを提供します。
//!
//! ## 使い方
//!
//! ```rust
//! use shiguredo_http11::upgrade::Upgrade;
//!
//! let upgrade = Upgrade::parse("websocket, h2c/1.0").unwrap();
//! assert!(upgrade.has_protocol("websocket"));
//! ```

use core::fmt;

/// Upgrade パースエラー
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UpgradeError {
    /// 空の入力
    Empty,
    /// 不正な形式
    InvalidFormat,
    /// 不正なプロトコル
    InvalidProtocol,
    /// 不正なバージョン
    InvalidVersion,
}

impl fmt::Display for UpgradeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            UpgradeError::Empty => write!(f, "empty Upgrade header"),
            UpgradeError::InvalidFormat => write!(f, "invalid Upgrade header format"),
            UpgradeError::InvalidProtocol => write!(f, "invalid Upgrade protocol"),
            UpgradeError::InvalidVersion => write!(f, "invalid Upgrade protocol version"),
        }
    }
}

impl std::error::Error for UpgradeError {}

/// Upgrade ヘッダー
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Upgrade {
    protocols: Vec<Protocol>,
}

impl Upgrade {
    /// Upgrade ヘッダーをパース
    pub fn parse(input: &str) -> Result<Self, UpgradeError> {
        let input = input.trim();
        if input.is_empty() {
            return Err(UpgradeError::Empty);
        }

        let mut protocols = Vec::new();
        for part in input.split(',') {
            let part = part.trim();
            if part.is_empty() {
                return Err(UpgradeError::InvalidFormat);
            }

            let (name, version) = if let Some((name, version)) = part.split_once('/') {
                if version.contains('/') {
                    return Err(UpgradeError::InvalidFormat);
                }
                let name = name.trim();
                let version = version.trim();
                if name.is_empty() {
                    return Err(UpgradeError::InvalidProtocol);
                }
                if version.is_empty() {
                    return Err(UpgradeError::InvalidVersion);
                }
                if !is_valid_token(name) {
                    return Err(UpgradeError::InvalidProtocol);
                }
                if !is_valid_token(version) {
                    return Err(UpgradeError::InvalidVersion);
                }
                (
                    name.to_ascii_lowercase(),
                    Some(version.to_ascii_lowercase()),
                )
            } else {
                if !is_valid_token(part) {
                    return Err(UpgradeError::InvalidProtocol);
                }
                (part.to_ascii_lowercase(), None)
            };

            protocols.push(Protocol { name, version });
        }

        if protocols.is_empty() {
            return Err(UpgradeError::Empty);
        }

        Ok(Upgrade { protocols })
    }

    /// プロトコル一覧
    pub fn protocols(&self) -> &[Protocol] {
        &self.protocols
    }

    /// 指定したプロトコルが含まれるかどうか
    pub fn has_protocol(&self, protocol: &str) -> bool {
        self.protocols
            .iter()
            .any(|p| p.name.eq_ignore_ascii_case(protocol))
    }
}

impl fmt::Display for Upgrade {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let values: Vec<String> = self.protocols.iter().map(|item| item.to_string()).collect();
        write!(f, "{}", values.join(", "))
    }
}

/// Upgrade プロトコル
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Protocol {
    name: String,
    version: Option<String>,
}

impl Protocol {
    /// プロトコル名
    pub fn name(&self) -> &str {
        &self.name
    }

    /// バージョン (任意)
    pub fn version(&self) -> Option<&str> {
        self.version.as_deref()
    }
}

impl fmt::Display for Protocol {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.version {
            Some(version) => write!(f, "{}/{}", self.name, version),
            None => write!(f, "{}", self.name),
        }
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_simple() {
        let upgrade = Upgrade::parse("websocket").unwrap();
        assert!(upgrade.has_protocol("websocket"));
        assert_eq!(upgrade.protocols().len(), 1);
    }

    #[test]
    fn parse_with_version() {
        let upgrade = Upgrade::parse("h2c/1.0, websocket").unwrap();
        assert_eq!(upgrade.protocols()[0].name(), "h2c");
        assert_eq!(upgrade.protocols()[0].version(), Some("1.0"));
    }

    #[test]
    fn parse_invalid() {
        assert!(Upgrade::parse("").is_err());
        assert!(Upgrade::parse("bad value").is_err());
        assert!(Upgrade::parse("websocket/").is_err());
        assert!(Upgrade::parse("websocket/1/2").is_err());
    }

    #[test]
    fn display() {
        let upgrade = Upgrade::parse("websocket, h2c/1.0").unwrap();
        assert_eq!(upgrade.to_string(), "websocket, h2c/1.0");
    }
}
