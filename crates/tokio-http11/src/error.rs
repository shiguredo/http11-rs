//! tokio-http11 エラー型

use std::fmt;

/// tokio-http11 エラー
#[derive(Debug)]
pub enum Error {
    /// I/O エラー
    Io(std::io::Error),
    /// HTTP パースエラー
    Http(shiguredo_http11::Error),
    /// TLS エラー
    Tls(String),
    /// 接続タイムアウト
    Timeout,
    /// 接続が閉じられた
    ConnectionClosed,
    /// 不正な URL
    #[cfg(feature = "client")]
    InvalidUrl(String),
    /// DNS 解決エラー
    #[cfg(feature = "client")]
    DnsResolution(String),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Io(e) => write!(f, "I/O error: {}", e),
            Error::Http(e) => write!(f, "HTTP error: {}", e),
            Error::Tls(e) => write!(f, "TLS error: {}", e),
            Error::Timeout => write!(f, "connection timeout"),
            Error::ConnectionClosed => write!(f, "connection closed"),
            #[cfg(feature = "client")]
            Error::InvalidUrl(msg) => write!(f, "invalid URL: {}", msg),
            #[cfg(feature = "client")]
            Error::DnsResolution(msg) => write!(f, "DNS resolution error: {}", msg),
        }
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Error::Io(e) => Some(e),
            Error::Http(e) => Some(e),
            _ => None,
        }
    }
}

impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Self {
        Error::Io(e)
    }
}

impl From<shiguredo_http11::Error> for Error {
    fn from(e: shiguredo_http11::Error) -> Self {
        Error::Http(e)
    }
}

impl From<tokio::time::error::Elapsed> for Error {
    fn from(_: tokio::time::error::Elapsed) -> Self {
        Error::Timeout
    }
}

impl From<rustls::Error> for Error {
    fn from(e: rustls::Error) -> Self {
        Error::Tls(e.to_string())
    }
}

impl From<rustls_pki_types::InvalidDnsNameError> for Error {
    fn from(e: rustls_pki_types::InvalidDnsNameError) -> Self {
        Error::Tls(e.to_string())
    }
}

/// Result 型エイリアス
pub type Result<T> = std::result::Result<T, Error>;
