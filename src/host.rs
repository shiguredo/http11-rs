//! Host ヘッダーパース (RFC 9110 Section 7.2)
//!
//! ## 概要
//!
//! RFC 9110 に基づいた Host ヘッダーのパースを提供します。
//!
//! ## 使い方
//!
//! ```rust
//! use shiguredo_http11::host::Host;
//!
//! let host = Host::parse("example.com:8080").unwrap();
//! assert_eq!(host.host(), "example.com");
//! assert_eq!(host.port(), Some(8080));
//! ```

use core::fmt;
use std::net::{Ipv4Addr, Ipv6Addr};

/// Host パースエラー
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HostError {
    /// 空の入力
    Empty,
    /// 不正な形式
    InvalidFormat,
    /// 不正なホスト
    InvalidHost,
    /// 不正なポート
    InvalidPort,
}

impl fmt::Display for HostError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            HostError::Empty => write!(f, "empty Host header"),
            HostError::InvalidFormat => write!(f, "invalid Host header format"),
            HostError::InvalidHost => write!(f, "invalid Host header host"),
            HostError::InvalidPort => write!(f, "invalid Host header port"),
        }
    }
}

impl std::error::Error for HostError {}

/// Host ヘッダー
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Host {
    host: String,
    port: Option<u16>,
}

impl Host {
    /// Host ヘッダーをパース
    pub fn parse(input: &str) -> Result<Self, HostError> {
        let input = input.trim();
        if input.is_empty() {
            return Err(HostError::Empty);
        }

        if input.chars().any(|c| c.is_whitespace()) {
            return Err(HostError::InvalidFormat);
        }

        if input.starts_with('[') {
            return parse_ipv6_host(input);
        }

        let (host_part, port) = split_host_port(input)?;
        if host_part.is_empty() || host_part.contains('@') {
            return Err(HostError::InvalidHost);
        }

        if host_part.parse::<Ipv4Addr>().is_ok() {
            return Ok(Host {
                host: host_part.to_string(),
                port,
            });
        }

        if !is_valid_reg_name(host_part) {
            return Err(HostError::InvalidHost);
        }

        Ok(Host {
            host: host_part.to_string(),
            port,
        })
    }

    /// Host 名 (IPv6 は角括弧付き)
    pub fn host(&self) -> &str {
        &self.host
    }

    /// ポート番号 (任意)
    pub fn port(&self) -> Option<u16> {
        self.port
    }

    /// IPv6 リテラルかどうか
    pub fn is_ipv6(&self) -> bool {
        self.host.starts_with('[')
    }
}

impl fmt::Display for Host {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(port) = self.port {
            write!(f, "{}:{}", self.host, port)
        } else {
            write!(f, "{}", self.host)
        }
    }
}

fn parse_ipv6_host(input: &str) -> Result<Host, HostError> {
    let end = input.find(']').ok_or(HostError::InvalidHost)?;
    let host_inner = &input[1..end];
    let rest = &input[end + 1..];

    let port = if rest.is_empty() {
        None
    } else if let Some(port_str) = rest.strip_prefix(':') {
        Some(parse_port(port_str)?)
    } else {
        return Err(HostError::InvalidHost);
    };

    if host_inner.is_empty() {
        return Err(HostError::InvalidHost);
    }

    if !is_valid_ipv6_or_future(host_inner) {
        return Err(HostError::InvalidHost);
    }

    Ok(Host {
        host: input[..end + 1].to_string(),
        port,
    })
}

fn split_host_port(input: &str) -> Result<(&str, Option<u16>), HostError> {
    if let Some((host, port_str)) = input.rsplit_once(':') {
        if host.contains(':') {
            return Err(HostError::InvalidHost);
        }
        if port_str.is_empty() {
            return Err(HostError::InvalidPort);
        }
        let port = parse_port(port_str)?;
        return Ok((host, Some(port)));
    }
    Ok((input, None))
}

fn parse_port(input: &str) -> Result<u16, HostError> {
    if input.is_empty() || !input.chars().all(|c| c.is_ascii_digit()) {
        return Err(HostError::InvalidPort);
    }
    input.parse::<u16>().map_err(|_| HostError::InvalidPort)
}

fn is_valid_ipv6_or_future(input: &str) -> bool {
    if input.starts_with('v') || input.starts_with('V') {
        return is_valid_ipvfuture(input);
    }
    input.parse::<Ipv6Addr>().is_ok()
}

fn is_valid_ipvfuture(input: &str) -> bool {
    let bytes = input.as_bytes();
    if bytes.len() < 3 {
        return false;
    }
    if bytes[0] != b'v' && bytes[0] != b'V' {
        return false;
    }

    let mut i = 1;
    let mut hex_len = 0;
    while i < bytes.len() && is_hexdig(bytes[i]) {
        hex_len += 1;
        i += 1;
    }

    if hex_len == 0 || i >= bytes.len() || bytes[i] != b'.' {
        return false;
    }
    i += 1;

    if i >= bytes.len() {
        return false;
    }

    while i < bytes.len() {
        let b = bytes[i];
        if !is_ipvfuture_char(b) {
            return false;
        }
        i += 1;
    }

    true
}

fn is_ipvfuture_char(b: u8) -> bool {
    is_unreserved(b) || is_sub_delim(b) || b == b':'
}

fn is_valid_reg_name(input: &str) -> bool {
    let bytes = input.as_bytes();
    if bytes.is_empty() {
        return false;
    }

    let mut i = 0;
    while i < bytes.len() {
        let b = bytes[i];
        if is_unreserved(b) || is_sub_delim(b) {
            i += 1;
            continue;
        }
        if b == b'%' {
            if i + 2 >= bytes.len() {
                return false;
            }
            if !is_hexdig(bytes[i + 1]) || !is_hexdig(bytes[i + 2]) {
                return false;
            }
            i += 3;
            continue;
        }
        return false;
    }

    true
}

fn is_unreserved(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'-' || b == b'.' || b == b'_' || b == b'~'
}

fn is_sub_delim(b: u8) -> bool {
    matches!(
        b,
        b'!' | b'$' | b'&' | b'\'' | b'(' | b')' | b'*' | b'+' | b',' | b';' | b'='
    )
}

fn is_hexdig(b: u8) -> bool {
    b.is_ascii_hexdigit()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_hostname() {
        let host = Host::parse("example.com").unwrap();
        assert_eq!(host.host(), "example.com");
        assert_eq!(host.port(), None);
    }

    #[test]
    fn parse_hostname_port() {
        let host = Host::parse("example.com:8080").unwrap();
        assert_eq!(host.host(), "example.com");
        assert_eq!(host.port(), Some(8080));
    }

    #[test]
    fn parse_ipv4() {
        let host = Host::parse("127.0.0.1").unwrap();
        assert_eq!(host.host(), "127.0.0.1");
    }

    #[test]
    fn parse_ipv6() {
        let host = Host::parse("[::1]").unwrap();
        assert!(host.is_ipv6());
        assert_eq!(host.host(), "[::1]");
    }

    #[test]
    fn parse_invalid() {
        assert!(Host::parse("").is_err());
        assert!(Host::parse("example.com:").is_err());
        assert!(Host::parse("example.com:abc").is_err());
        assert!(Host::parse("exa mple.com").is_err());
    }

    #[test]
    fn display() {
        let host = Host::parse("example.com:8080").unwrap();
        assert_eq!(host.to_string(), "example.com:8080");
    }
}
