//! HTTP ヘッダー名型 (RFC 9110 Section 5.1, field-name = token)

use alloc::borrow::Cow;
use alloc::string::String;
use alloc::vec::Vec;
use core::fmt;
use core::hash::{Hash, Hasher};

/// HTTP ヘッダー名 (RFC 9110 Section 5.1, field-name = token)
///
/// Eq/Hash は case-insensitive (RFC 9110 Section 5.1 "Field names are case-insensitive")。
/// `const fn from_static` では borrowed bytes を変更できないため、
/// 内部正規化を行わずに保持し、比較時に case-insensitive 判定を行う。
#[derive(Debug, Clone)]
pub struct HeaderName(Cow<'static, [u8]>);

/// `HeaderName` の構築エラー
#[derive(Debug)]
#[non_exhaustive]
pub enum HeaderNameError {
    /// 空のヘッダー名
    Empty,
    /// 不正なバイトを含む
    InvalidByte { byte: u8, position: usize },
}

impl fmt::Display for HeaderNameError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            HeaderNameError::Empty => f.write_str("empty header name"),
            HeaderNameError::InvalidByte { byte, position } => {
                write!(
                    f,
                    "invalid byte 0x{:02X} at position {} in header name",
                    byte, position
                )
            }
        }
    }
}

/// RFC 9110 Section 5.6.2 tchar 判定 (const 文脈で使用可能)
const fn is_tchar(b: u8) -> bool {
    matches!(
        b,
        b'!' | b'#' | b'$' | b'%' | b'&' | b'\'' | b'*' | b'+' | b'-' | b'.'
        | b'0'..=b'9' | b'A'..=b'Z' | b'^' | b'_' | b'`' | b'a'..=b'z' | b'|' | b'~'
    )
}

impl HeaderName {
    /// ランタイム検査つきで構築する
    pub fn new(name: impl AsRef<[u8]>) -> Result<Self, HeaderNameError> {
        let bytes = name.as_ref();
        if bytes.is_empty() {
            return Err(HeaderNameError::Empty);
        }
        let mut i = 0;
        while i < bytes.len() {
            if !is_tchar(bytes[i]) {
                return Err(HeaderNameError::InvalidByte {
                    byte: bytes[i],
                    position: i,
                });
            }
            i += 1;
        }
        Ok(Self(Cow::Owned(bytes.to_vec())))
    }

    /// コンパイル時検査つきで構築する
    ///
    /// 不正な入力はコンパイル時に panic する。
    /// リテラル定数の構築に使用する。
    pub const fn from_static(name: &'static [u8]) -> Self {
        if name.is_empty() {
            panic!("HeaderName: empty header name");
        }
        let mut i = 0;
        while i < name.len() {
            if !is_tchar(name[i]) {
                panic!("HeaderName: invalid byte in header name");
            }
            i += 1;
        }
        Self(Cow::Borrowed(name))
    }

    /// 内部バイト列を返す
    pub fn as_bytes(&self) -> &[u8] {
        &self.0
    }

    /// 文字列として返す (全 tchar は ASCII のため安全)
    pub fn as_str(&self) -> &str {
        // SAFETY: tchar は全て ASCII 範囲内であるため UTF-8 として有効
        unsafe { core::str::from_utf8_unchecked(self.as_bytes()) }
    }

    /// 検証済みのバイト列から構築する (crate 内部用)
    pub(crate) fn from_validated_bytes(name: Vec<u8>) -> Self {
        debug_assert!(!name.is_empty() && name.iter().all(|&b| is_tchar(b)));
        Self(Cow::Owned(name))
    }
}

impl PartialEq for HeaderName {
    fn eq(&self, other: &Self) -> bool {
        self.as_bytes().eq_ignore_ascii_case(other.as_bytes())
    }
}

impl Eq for HeaderName {}

impl Hash for HeaderName {
    fn hash<H: Hasher>(&self, state: &mut H) {
        for &b in self.as_bytes() {
            state.write_u8(b.to_ascii_lowercase());
        }
    }
}

impl PartialEq<str> for HeaderName {
    fn eq(&self, other: &str) -> bool {
        self.as_bytes().eq_ignore_ascii_case(other.as_bytes())
    }
}

impl PartialEq<&str> for HeaderName {
    fn eq(&self, other: &&str) -> bool {
        self.as_bytes().eq_ignore_ascii_case(other.as_bytes())
    }
}

impl PartialEq<HeaderName> for str {
    fn eq(&self, other: &HeaderName) -> bool {
        self.as_bytes().eq_ignore_ascii_case(other.as_bytes())
    }
}

impl fmt::Display for HeaderName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl From<HeaderName> for String {
    fn from(name: HeaderName) -> Self {
        String::from_utf8(name.0.into_owned()).expect("HeaderName is always valid ASCII")
    }
}
