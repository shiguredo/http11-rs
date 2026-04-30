//! request-target の形式 (RFC 9112 Section 3.2)
//!
//! encoder と decoder で共有される概念。

/// RFC 9112 Section 3.2 request-target の形式
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RequestTargetForm {
    /// origin-form: absolute-path [ "?" query ]
    /// 例: /path/to/resource?query=value
    Origin,
    /// absolute-form: absolute-URI
    /// 例: http://example.com/path
    Absolute,
    /// authority-form: uri-host ":" port (CONNECT のみ)
    /// 例: example.com:443
    Authority,
    /// asterisk-form: "*" (OPTIONS のみ)
    Asterisk,
}
