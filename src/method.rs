//! HTTP メソッド型 (RFC 9110 Section 9.1, method = token)

use alloc::borrow::Cow;
use alloc::string::{String, ToString};
use alloc::vec::Vec;
use core::fmt;

use crate::validate::is_token_char;

/// HTTP メソッド (RFC 9110 Section 9.1, method = token)
///
/// case-sensitive (RFC 9110 Section 9.1 "The method token is case-sensitive")。
/// Eq/Hash も case-sensitive。
///
/// # 構築経路
///
/// | 用途 | API | 不正 token 時 |
/// |---|---|---|
/// | builder (`'static` リテラル) | `TryFrom<&'static str>` / `TryFrom<&'static [u8]>` | `Err(MethodError)` |
/// | 動的入力 | `Method::new()` | `Err(MethodError)` |
/// | `const` 定数 (compile-time 拒否) | `Method::from_static(b"...")` | コンパイル時 panic |
///
/// # 非 `'static` な `&str` について
///
/// `TryFrom<&'static str>` のみ実装しているため、非 `'static` な `&str` は
/// コンパイルエラーになる。動的な文字列を使う場合は `Method::new()` で
/// 構築した値を渡すこと。
///
/// ```compile_fail
/// use shiguredo_http11::Method;
///
/// fn make_method(m: &str) -> Method {
///     m.try_into().unwrap() // コンパイルエラー: &str は &'static str ではない
/// }
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Method(Cow<'static, [u8]>);

/// `Method` の構築エラー
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MethodError {
    /// 空のメソッド
    Empty { input: String },
    /// 不正なバイトを含む
    InvalidByte {
        byte: u8,
        position: usize,
        input: String,
    },
}

impl MethodError {
    /// エラーの原因となった入力文字列への参照を返す
    pub fn input(&self) -> &str {
        match self {
            MethodError::Empty { input } => input,
            MethodError::InvalidByte { input, .. } => input,
        }
    }

    /// エラーの原因となった入力文字列を消費して返す
    pub fn into_input(self) -> String {
        match self {
            MethodError::Empty { input } => input,
            MethodError::InvalidByte { input, .. } => input,
        }
    }
}

impl fmt::Display for MethodError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            MethodError::Empty { input } => {
                write!(f, "empty method: {:?}", input)
            }
            MethodError::InvalidByte {
                byte,
                position,
                input,
            } => {
                write!(
                    f,
                    "invalid byte 0x{:02X} at position {} in method: {:?}",
                    byte, position, input
                )
            }
        }
    }
}

impl core::error::Error for MethodError {}

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
    pub const QUERY: Self = Self::from_static(b"QUERY");

    /// ランタイム検査つきで構築する
    pub fn new(method: impl AsRef<[u8]>) -> Result<Self, MethodError> {
        let bytes = method.as_ref();
        if bytes.is_empty() {
            return Err(MethodError::Empty {
                input: String::from_utf8_lossy(bytes).into_owned(),
            });
        }
        let mut i = 0;
        while i < bytes.len() {
            if !is_token_char(bytes[i]) {
                return Err(MethodError::InvalidByte {
                    byte: bytes[i],
                    position: i,
                    input: String::from_utf8_lossy(bytes).into_owned(),
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
    ///
    /// # 正常系
    ///
    /// ```
    /// use shiguredo_http11::Method;
    ///
    /// const GET: Method = Method::from_static(b"GET");
    /// const POST: Method = Method::from_static(b"POST");
    /// const CUSTOM: Method = Method::from_static(b"WebDAV-MOVE");
    /// ```
    ///
    /// # 不正リテラルの compile-fail 例
    ///
    /// 空のメソッドは不正:
    ///
    /// ```compile_fail
    /// const _: shiguredo_http11::Method =
    ///     shiguredo_http11::Method::from_static(b"");
    /// ```
    ///
    /// CR を含むメソッドは不正:
    ///
    /// ```compile_fail
    /// const _: shiguredo_http11::Method =
    ///     shiguredo_http11::Method::from_static(b"GET\r");
    /// ```
    ///
    /// LF を含むメソッドは不正:
    ///
    /// ```compile_fail
    /// const _: shiguredo_http11::Method =
    ///     shiguredo_http11::Method::from_static(b"GET\n");
    /// ```
    pub const fn from_static(method: &'static [u8]) -> Self {
        if method.is_empty() {
            panic!("Method: empty method");
        }
        let mut i = 0;
        while i < method.len() {
            if !is_token_char(method[i]) {
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
        debug_assert!(!method.is_empty() && method.iter().all(|&b| is_token_char(b)));
        Self(Cow::Owned(method))
    }
}

impl TryFrom<&'static str> for Method {
    type Error = MethodError;

    fn try_from(s: &'static str) -> Result<Self, Self::Error> {
        let bytes = s.as_bytes();
        if bytes.is_empty() {
            return Err(MethodError::Empty {
                input: s.to_string(),
            });
        }
        for (i, &b) in bytes.iter().enumerate() {
            if !is_token_char(b) {
                return Err(MethodError::InvalidByte {
                    byte: b,
                    position: i,
                    input: s.to_string(),
                });
            }
        }
        Ok(Self(Cow::Borrowed(bytes)))
    }
}

impl TryFrom<&'static [u8]> for Method {
    type Error = MethodError;

    fn try_from(bytes: &'static [u8]) -> Result<Self, Self::Error> {
        if bytes.is_empty() {
            return Err(MethodError::Empty {
                input: String::from_utf8_lossy(bytes).into_owned(),
            });
        }
        for (i, &b) in bytes.iter().enumerate() {
            if !is_token_char(b) {
                return Err(MethodError::InvalidByte {
                    byte: b,
                    position: i,
                    input: String::from_utf8_lossy(bytes).into_owned(),
                });
            }
        }
        Ok(Self(Cow::Borrowed(bytes)))
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn from_validated_parts_matches_new() {
        let methods: &[&[u8]] = &[b"GET", b"post", b"Custom"];
        for &method in methods {
            let v1 = Method::new(method).unwrap();
            let v2 = Method::from_validated_bytes(method.to_vec());
            assert_eq!(v1, v2);
        }
    }
}
