//! quoted-string テスト用の共通定数と境界値 (RFC 9110 Section 5.6.4 / 5.5)
//!
//! 複数のヘッダーモジュールの単体テストで再利用するため、CTL 集合や obs-text
//! 境界値を 1 箇所に定義する。tests/test_<module>.rs から `mod helpers;` で取り込む。

/// HTAB (0x09) を **除いた** ASCII CTL (0x00-0x1F) と DEL (0x7F) の網羅集合
///
/// RFC 9110 Section 5.5 で:
///
/// - CR / LF / NUL: MUST either reject the message or replace with SP (本リポジトリは reject を選択)
/// - その他の CTL (%x01-08, %x0B-0C, %x0E-1F, %x7F DEL): MAY retain (safe context 限定)
///
/// 本リポジトリでは Accept / Content-Type / Expect 等の標準ヘッダを safe context 外
/// として保守的に reject するため、HTAB を除く全 CTL + DEL を一括検証する。
pub const ALL_CTLS_EXCEPT_HTAB: &[u32] = &[
    0x00, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x0A, 0x0B, 0x0C, 0x0D, 0x0E, 0x0F, 0x10,
    0x11, 0x12, 0x13, 0x14, 0x15, 0x16, 0x17, 0x18, 0x19, 0x1A, 0x1B, 0x1C, 0x1D, 0x1E, 0x1F, 0x7F,
];
