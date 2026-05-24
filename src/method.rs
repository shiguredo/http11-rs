//! HTTP メソッド型 (RFC 9110 Section 9.1, method = token)

use alloc::borrow::Cow;
use alloc::string::String;
use alloc::vec::Vec;
use core::fmt;

/// HTTP メソッド (RFC 9110 Section 9.1, method = token)
///
/// case-sensitive (RFC 9110 Section 9.1 "The method token is case-sensitive")。
/// Eq/Hash も case-sensitive。
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Method(Cow<'static, [u8]>);

/// `Method` の構築エラー
#[derive(Debug)]
#[non_exhaustive]
pub enum MethodError {
    /// 空のメソッド
    Empty,
    /// 不正なバイトを含む
    InvalidByte { byte: u8, position: usize },
}

impl fmt::Display for MethodError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            MethodError::Empty => f.write_str("empty method"),
            MethodError::InvalidByte { byte, position } => {
                write!(
                    f,
                    "invalid byte 0x{:02X} at position {} in method",
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

impl Method {
    /// 標準メソッド定数
    pub const GET: Self = Self::from_static(b"GET");
    pub const POST: Self = Self::from_static(b"POST");
    pub const PUT: Self = Self::from_static(b"PUT");
    pub const DELETE: Self = Self::from_static(b"DELETE");
    pub const HEAD: Self = Self::from_static(b"HEAD");
    pub const OPTIONS: Self = Self::from_static(b"OPTIONS");
    pub const CONNECT: Self = Self::from_static(b"CONNECT");
    pub const TRACE: Self = Self::from_static(b"TRACE");
    pub const PATCH: Self = Self::from_static(b"PATCH");

    /// ランタイム検査つきで構築する
    pub fn new(method: impl AsRef<[u8]>) -> Result<Self, MethodError> {
        let bytes = method.as_ref();
        if bytes.is_empty() {
            return Err(MethodError::Empty);
        }
        let mut i = 0;
        while i < bytes.len() {
            if !is_tchar(bytes[i]) {
                return Err(MethodError::InvalidByte {
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
    pub const fn from_static(method: &'static [u8]) -> Self {
        if method.is_empty() {
            panic!("Method: empty method");
        }
        let mut i = 0;
        while i < method.len() {
            if !is_tchar(method[i]) {
                panic!("Method: invalid byte in method");
            }
            i += 1;
        }
        Self(Cow::Borrowed(method))
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
    pub(crate) fn from_validated_bytes(method: Vec<u8>) -> Self {
        debug_assert!(!method.is_empty() && method.iter().all(|&b| is_tchar(b)));
        Self(Cow::Owned(method))
    }
}

impl PartialEq<str> for Method {
    fn eq(&self, other: &str) -> bool {
        self.as_bytes() == other.as_bytes()
    }
}

impl PartialEq<&str> for Method {
    fn eq(&self, other: &&str) -> bool {
        self.as_bytes() == other.as_bytes()
    }
}

impl PartialEq<Method> for str {
    fn eq(&self, other: &Method) -> bool {
        self.as_bytes() == other.as_bytes()
    }
}

impl fmt::Display for Method {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl From<Method> for String {
    fn from(method: Method) -> Self {
        String::from_utf8(method.0.into_owned()).expect("Method is always valid ASCII")
    }
}
