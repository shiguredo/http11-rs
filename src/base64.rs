//! RFC 4648 Section 4 Base64 エンコード/デコード (依存なし実装)
//!
//! ## 仕様
//!
//! - 標準アルファベット (`A-Z`, `a-z`, `0-9`, `+`, `/`) を使う
//! - エンコード時は 4 文字単位になるよう末尾を `=` でパディングする
//! - デコード時は末尾の `=` を許容し、空白文字 (` `, `\t`, `\n`, `\r`) は無視する
//! - デコードは RFC 4648 Section 3.5 のストリクト方針に従う:
//!   - 空白除去後の入力長は 4 の倍数 (パディング含む) でなければならない
//!   - 末尾 `=` の個数は 0 / 1 / 2 のいずれか
//!   - データ部分の文字数 mod 4 と `=` 個数の整合性を検証する
//!   - 末尾の余剰 bit (RFC 4648 §3.3「MUST be zero」) が 0 でない場合は reject
//!   - 本ライブラリは Basic / Digest 認証等で credential canonicalization を担保するため
//!     non-canonical base64 表現を受理しない方針とする

use alloc::string::String;
use alloc::vec::Vec;

/// Base64 デコードのエラー
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Base64Error {
    /// アルファベット外の文字を検出した
    InvalidCharacter,
    /// パディングが RFC 4648 Section 3.5 に従わない (入力長が 4 の倍数でない、
    /// `=` 個数が不正、末尾余剰 bit が 0 でない等)
    InvalidPadding,
}

const BASE64_ALPHABET: &[u8; 64] =
    b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

/// Base64 エンコード
pub(crate) fn encode(input: &[u8]) -> String {
    let mut result = String::new();
    let mut i = 0;

    while i < input.len() {
        let b0 = input[i];
        let b1 = if i + 1 < input.len() { input[i + 1] } else { 0 };
        let b2 = if i + 2 < input.len() { input[i + 2] } else { 0 };

        let n = ((b0 as u32) << 16) | ((b1 as u32) << 8) | (b2 as u32);

        result.push(BASE64_ALPHABET[(n >> 18 & 0x3F) as usize] as char);
        result.push(BASE64_ALPHABET[(n >> 12 & 0x3F) as usize] as char);

        if i + 1 < input.len() {
            result.push(BASE64_ALPHABET[(n >> 6 & 0x3F) as usize] as char);
        } else {
            result.push('=');
        }

        if i + 2 < input.len() {
            result.push(BASE64_ALPHABET[(n & 0x3F) as usize] as char);
        } else {
            result.push('=');
        }

        i += 3;
    }

    result
}

/// Base64 デコード (RFC 4648 Section 3.5 strict)
///
/// 空白 (` ` / `\t` / `\n` / `\r`) は無視する。それ以外の文字 (パディング `=` 含む)
/// については RFC 4648 のストリクト方針で検証する:
///
/// - 空白除去後の全長は 4 の倍数
/// - 末尾の `=` 個数は 0 / 1 / 2 のいずれか
/// - パディング前のデータ文字数 mod 4 と `=` 個数の整合性 (mod 4 == 0 / 2 / 3 が対応)
/// - 末尾の余剰 bit (RFC 4648 §3.3 MUST zero) が 0 でない場合は reject
///
/// 本実装は Basic / Digest 認証の credential canonicalization を担保するため
/// non-canonical base64 表現 (例: `Zg===` のようなパディング過多、`A` のような
/// 不完全な末尾 6 bit 群) を全て reject する。
pub(crate) fn decode(input: &str) -> Result<Vec<u8>, Base64Error> {
    // 空白を除去した正規化バッファを作る
    let mut normalized: Vec<u8> = Vec::with_capacity(input.len());
    for c in input.chars() {
        match c {
            ' ' | '\t' | '\n' | '\r' => continue,
            _ => {
                if !c.is_ascii() {
                    return Err(Base64Error::InvalidCharacter);
                }
                normalized.push(c as u8);
            }
        }
    }

    if normalized.is_empty() {
        return Ok(Vec::new());
    }

    // 末尾 `=` 個数のカウント
    let pad_count = normalized.iter().rev().take_while(|&&b| b == b'=').count();
    if pad_count > 2 {
        return Err(Base64Error::InvalidPadding);
    }

    // 全長は 4 の倍数 (RFC 4648 §3.5)
    if !normalized.len().is_multiple_of(4) {
        return Err(Base64Error::InvalidPadding);
    }

    // データ部分 (パディング前) と末尾 `=` 個数の整合性検証
    let data = &normalized[..normalized.len() - pad_count];
    let last_block_chars = data.len() % 4;
    let valid = match pad_count {
        0 => last_block_chars == 0,
        1 => last_block_chars == 3,
        2 => last_block_chars == 2,
        _ => false,
    };
    if !valid {
        return Err(Base64Error::InvalidPadding);
    }

    let mut result = Vec::with_capacity((data.len() * 3) / 4);
    let mut buf: u32 = 0;
    let mut bits: u32 = 0;

    for &b in data {
        let val = match b {
            b'A'..=b'Z' => (b - b'A') as u32,
            b'a'..=b'z' => (b - b'a') as u32 + 26,
            b'0'..=b'9' => (b - b'0') as u32 + 52,
            b'+' => 62,
            b'/' => 63,
            _ => return Err(Base64Error::InvalidCharacter),
        };

        buf = (buf << 6) | val;
        bits += 6;

        if bits >= 8 {
            bits -= 8;
            result.push((buf >> bits) as u8);
            buf &= (1 << bits) - 1;
        }
    }

    // RFC 4648 §3.3: 末尾の余剰 bit は 0 でなければならない (MUST be zero)
    // canonical でない base64 (パディングを書きすぎて残余 bit が非ゼロのケース) を reject
    if buf != 0 {
        return Err(Base64Error::InvalidPadding);
    }

    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encode_basic() {
        assert_eq!(encode(b""), "");
        assert_eq!(encode(b"f"), "Zg==");
        assert_eq!(encode(b"fo"), "Zm8=");
        assert_eq!(encode(b"foo"), "Zm9v");
        assert_eq!(encode(b"foob"), "Zm9vYg==");
        assert_eq!(encode(b"fooba"), "Zm9vYmE=");
        assert_eq!(encode(b"foobar"), "Zm9vYmFy");
        assert_eq!(encode(b"user:password"), "dXNlcjpwYXNzd29yZA==");
    }

    #[test]
    fn decode_basic() {
        assert_eq!(decode("").unwrap(), b"");
        assert_eq!(decode("Zg==").unwrap(), b"f");
        assert_eq!(decode("Zm8=").unwrap(), b"fo");
        assert_eq!(decode("Zm9v").unwrap(), b"foo");
        assert_eq!(decode("Zm9vYg==").unwrap(), b"foob");
        assert_eq!(decode("Zm9vYmE=").unwrap(), b"fooba");
        assert_eq!(decode("Zm9vYmFy").unwrap(), b"foobar");
        assert_eq!(decode("dXNlcjpwYXNzd29yZA==").unwrap(), b"user:password");
    }

    #[test]
    fn decode_ignores_whitespace() {
        assert_eq!(decode("Zm9v\n").unwrap(), b"foo");
        assert_eq!(decode("Z m 9 v").unwrap(), b"foo");
        assert_eq!(decode("Zm9v\r\n").unwrap(), b"foo");
        assert_eq!(decode("Zm9v\t").unwrap(), b"foo");
    }

    #[test]
    fn decode_rejects_invalid_character() {
        assert_eq!(decode("Zm9*"), Err(Base64Error::InvalidCharacter));
        assert_eq!(decode("Z\u{00A0}m9v"), Err(Base64Error::InvalidCharacter));
    }

    /// RFC 4648 §3.5 ストリクト: 入力長が 4 の倍数でない (パディングなし)
    #[test]
    fn decode_rejects_unpadded_short_input() {
        // 1 文字 (パディングなし)
        assert_eq!(decode("A"), Err(Base64Error::InvalidPadding));
        // 2 文字 (パディングなし)
        assert_eq!(decode("Zg"), Err(Base64Error::InvalidPadding));
        // 3 文字 (パディングなし)
        assert_eq!(decode("Zm8"), Err(Base64Error::InvalidPadding));
    }

    /// RFC 4648 §3.5 ストリクト: 末尾 `=` 個数の不整合
    #[test]
    fn decode_rejects_excess_padding() {
        // `=` 3 個 (本来 0 / 1 / 2 のいずれか)
        assert_eq!(decode("Zg==="), Err(Base64Error::InvalidPadding));
        // `=` 4 個
        assert_eq!(decode("===="), Err(Base64Error::InvalidPadding));
    }

    /// データ部分の文字数 mod 4 と `=` 個数の不整合
    #[test]
    fn decode_rejects_mismatched_padding() {
        // data 2 文字 + `=` 1 個 → 全長 3 で 4 の倍数違反
        assert_eq!(decode("Zg="), Err(Base64Error::InvalidPadding));
        // data 3 文字 + `=` 2 個 → 全長 5 で 4 の倍数違反
        assert_eq!(decode("Zm8=="), Err(Base64Error::InvalidPadding));
        // data 1 文字 + `=` 2 個 → 不可能
        assert_eq!(decode("Z=="), Err(Base64Error::InvalidPadding));
    }

    /// RFC 4648 §3.3: 末尾の余剰 bit は 0 でなければならない
    ///
    /// "Zh==" は本来 'f' を表す "Zg==" の non-canonical 変形で、末尾 6 bit のうち
    /// 下位 4 bit が 0 でない (`Z` = 25 = 0b011001、`h` = 33 = 0b100001、
    /// 復元 1 バイト 0b01100110 = 'f' のあとに残る 4 bit が `0b0001` で非ゼロ)。
    #[test]
    fn decode_rejects_non_zero_trailing_bits() {
        // 'f' を表す canonical は "Zg=="。"Zh==" は same length / valid padding だが
        // canonical 化が壊れているため reject されるべき。
        let result = decode("Zh==");
        assert_eq!(result, Err(Base64Error::InvalidPadding));
    }
}
