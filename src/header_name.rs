//! HTTP ヘッダー名型 (RFC 9110 Section 5.1, field-name = token)

use alloc::borrow::Cow;
use alloc::string::{String, ToString};
use alloc::vec::Vec;
use core::fmt;
use core::hash::{Hash, Hasher};

use crate::validate::is_token_char;

/// HTTP ヘッダー名 (RFC 9110 Section 5.1, field-name = token)
///
/// Eq/Hash は case-insensitive (RFC 9110 Section 5.1 "Field names are case-insensitive")。
/// `const fn from_static` では borrowed bytes を変更できないため、
/// 内部正規化を行わずに保持し、比較時に case-insensitive 判定を行う。
///
/// # 構築経路
///
/// | 用途 | API | 不正 token 時 |
/// |---|---|---|
/// | builder (`'static` リテラル) | `TryFrom<&'static str>` / `TryFrom<&'static [u8]>` | `Err(HeaderNameError)` |
/// | 動的入力 | `HeaderName::new()` | `Err(HeaderNameError)` |
/// | `const` 定数 (compile-time 拒否) | `HeaderName::from_static(b"...")` | コンパイル時 panic |
///
/// # 非 `'static` な `&str` について
///
/// `TryFrom<&'static str>` のみ実装しているため、非 `'static` な `&str` は
/// コンパイルエラーになる。動的な文字列を使う場合は `HeaderName::new()` で
/// 構築した値を渡すこと。
///
/// ```compile_fail
/// use shiguredo_http11::HeaderName;
///
/// fn make_header(name: &str) -> HeaderName {
///     name.try_into().unwrap() // コンパイルエラー: &str は &'static str ではない
/// }
/// ```
#[derive(Debug, Clone)]
pub struct HeaderName(Cow<'static, [u8]>);

/// `HeaderName` の構築エラー
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum HeaderNameError {
    /// 空のヘッダー名
    Empty { input: String },
    /// 不正なバイトを含む
    InvalidByte {
        byte: u8,
        position: usize,
        input: String,
    },
}

impl HeaderNameError {
    /// エラーの原因となった入力文字列への参照を返す
    pub fn input(&self) -> &str {
        match self {
            HeaderNameError::Empty { input } => input,
            HeaderNameError::InvalidByte { input, .. } => input,
        }
    }

    /// エラーの原因となった入力文字列を消費して返す
    pub fn into_input(self) -> String {
        match self {
            HeaderNameError::Empty { input } => input,
            HeaderNameError::InvalidByte { input, .. } => input,
        }
    }
}

impl fmt::Display for HeaderNameError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            HeaderNameError::Empty { input } => {
                write!(f, "empty header name: {:?}", input)
            }
            HeaderNameError::InvalidByte {
                byte,
                position,
                input,
            } => {
                write!(
                    f,
                    "invalid byte 0x{:02X} at position {} in header name: {:?}",
                    byte, position, input
                )
            }
        }
    }
}

impl core::error::Error for HeaderNameError {}

impl HeaderName {
    /// ランタイム検査つきで構築する
    pub fn new(name: impl AsRef<[u8]>) -> Result<Self, HeaderNameError> {
        let bytes = name.as_ref();
        if bytes.is_empty() {
            return Err(HeaderNameError::Empty {
                input: String::from_utf8_lossy(bytes).into_owned(),
            });
        }
        let mut i = 0;
        while i < bytes.len() {
            if !is_token_char(bytes[i]) {
                return Err(HeaderNameError::InvalidByte {
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
    /// use shiguredo_http11::HeaderName;
    ///
    /// const HOST: HeaderName = HeaderName::from_static(b"host");
    /// const CONTENT_TYPE: HeaderName = HeaderName::from_static(b"Content-Type");
    /// ```
    ///
    /// # 不正リテラルの compile-fail 例
    ///
    /// 空のヘッダー名は不正:
    ///
    /// ```compile_fail
    /// const _: shiguredo_http11::HeaderName =
    ///     shiguredo_http11::HeaderName::from_static(b"");
    /// ```
    ///
    /// CRLF 注入を含むヘッダー名は不正:
    ///
    /// ```compile_fail
    /// const _: shiguredo_http11::HeaderName =
    ///     shiguredo_http11::HeaderName::from_static(b"host\r\nX-Inject: evil");
    /// ```
    ///
    /// NUL バイトを含むヘッダー名は不正:
    ///
    /// ```compile_fail
    /// const _: shiguredo_http11::HeaderName =
    ///     shiguredo_http11::HeaderName::from_static(b"host\0");
    /// ```
    ///
    /// コロンを含むヘッダー名は不正:
    ///
    /// ```compile_fail
    /// const _: shiguredo_http11::HeaderName =
    ///     shiguredo_http11::HeaderName::from_static(b"host:name");
    /// ```
    ///
    /// 空白を含むヘッダー名は不正:
    ///
    /// ```compile_fail
    /// const _: shiguredo_http11::HeaderName =
    ///     shiguredo_http11::HeaderName::from_static(b"host name");
    /// ```
    pub const fn from_static(name: &'static [u8]) -> Self {
        if name.is_empty() {
            panic!("HeaderName: empty header name");
        }
        let mut i = 0;
        while i < name.len() {
            if !is_token_char(name[i]) {
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
        debug_assert!(!name.is_empty() && name.iter().all(|&b| is_token_char(b)));
        Self(Cow::Owned(name))
    }
}

impl TryFrom<&'static str> for HeaderName {
    type Error = HeaderNameError;

    fn try_from(s: &'static str) -> Result<Self, Self::Error> {
        let bytes = s.as_bytes();
        if bytes.is_empty() {
            return Err(HeaderNameError::Empty {
                input: s.to_string(),
            });
        }
        for (i, &b) in bytes.iter().enumerate() {
            if !is_token_char(b) {
                return Err(HeaderNameError::InvalidByte {
                    byte: b,
                    position: i,
                    input: s.to_string(),
                });
            }
        }
        Ok(Self(Cow::Borrowed(bytes)))
    }
}

impl TryFrom<&'static [u8]> for HeaderName {
    type Error = HeaderNameError;

    fn try_from(bytes: &'static [u8]) -> Result<Self, Self::Error> {
        if bytes.is_empty() {
            return Err(HeaderNameError::Empty {
                input: String::from_utf8_lossy(bytes).into_owned(),
            });
        }
        for (i, &b) in bytes.iter().enumerate() {
            if !is_token_char(b) {
                return Err(HeaderNameError::InvalidByte {
                    byte: b,
                    position: i,
                    input: String::from_utf8_lossy(bytes).into_owned(),
                });
            }
        }
        Ok(Self(Cow::Borrowed(bytes)))
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn from_validated_parts_matches_new() {
        let names: &[&[u8]] = &[b"host", b"Content-Type", b"X-Custom"];
        for &name in names {
            let v1 = HeaderName::new(name).unwrap();
            let v2 = HeaderName::from_validated_bytes(name.to_vec());
            assert_eq!(v1, v2);
        }
    }
}
