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

use alloc::string::{String, ToString};
use alloc::vec::Vec;
use core::fmt;

use crate::validate::is_valid_token;

/// Upgrade パースエラー
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
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

impl core::error::Error for UpgradeError {}

/// Upgrade ヘッダー
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Upgrade {
    protocols: Vec<Protocol>,
}

impl Upgrade {
    /// Upgrade ヘッダーをパース
    ///
    /// RFC 9110 Section 5.6.1.2: 空フィールド値・空要素は受理する
    pub fn parse(input: &str) -> Result<Self, UpgradeError> {
        let input = input.trim();

        let mut protocols = Vec::new();
        for part in input.split(',') {
            let part = part.trim();
            // RFC 9110 Section 5.6.1.2: 空要素は無視する
            if part.is_empty() {
                continue;
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
