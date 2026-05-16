//! PBT テスト共通ユーティリティ

use core::ops::RangeInclusive;

use proptest::prelude::*;

// ========================================
// 言語タグ生成 (BCP 47/RFC 5646)
// ========================================

/// 先頭サブタグ: ALPHA のみ (1-8 文字)
pub fn language_primary_subtag() -> impl Strategy<Value = String> {
    "[A-Za-z]{1,8}".prop_map(|s| s)
}

/// 後続サブタグ: ALPHA / DIGIT (1-8 文字)
pub fn language_subsequent_subtag() -> impl Strategy<Value = String> {
    "[A-Za-z0-9]{1,8}".prop_map(|s| s)
}

/// 言語タグ: primary-subtag *("-" subtag)
pub fn language_tag() -> impl Strategy<Value = String> {
    (
        language_primary_subtag(),
        proptest::collection::vec(language_subsequent_subtag(), 0..=2),
    )
        .prop_map(|(primary, rest)| {
            if rest.is_empty() {
                primary
            } else {
                format!("{}-{}", primary, rest.join("-"))
            }
        })
}

// ========================================
// quoted-string 用 char / String strategy (RFC 9110 Section 5.6.4)
// ========================================

/// 引用符内で使える文字 (qdtext + obs-text の Unicode scalar 拡張)
///
/// RFC 9110 Section 5.6.4 の qdtext ABNF (オクテット表現):
/// ```text
/// qdtext = HTAB / SP / %x21 / %x23-5B / %x5D-7E / obs-text
/// ```
/// を、char 単位走査の本実装に合わせて Unicode scalar に拡張解釈する
/// (issue 0059 で確立)。surrogate (`U+D800..=U+DFFF`) は char 型で構築不能。
pub fn qdtext_char() -> impl Strategy<Value = char> {
    prop_oneof![
        Just('\t'),
        Just(' '),
        Just('!'),
        prop::char::range('#', '['), // 0x23-0x5B (DQUOTE 0x22 除外)
        prop::char::range(']', '~'), // 0x5D-0x7E (バックスラッシュ 0x5C 除外)
        // obs-text を Unicode scalar として opaque 保持する範囲。
        // surrogate を跨がないよう二分割している。
        prop::char::range('\u{80}', '\u{D7FF}'),
        prop::char::range('\u{E000}', '\u{10FFFF}'),
    ]
}

/// 引用符付き文字列の中身 (エスケープなし、obs-text を含む)
///
/// 長さ範囲は呼び出し側で指定する。空文字列 (`0..=N`) を含む場合、ヘッダによっては
/// Display ラウンドトリップで `name=""` ではなく `name=` が出力されることに注意
/// (`needs_quoting` が空文字列で true を返すよう修正済みなら問題ない)。
pub fn qdtext_value(len_range: RangeInclusive<usize>) -> impl Strategy<Value = String> {
    proptest::collection::vec(qdtext_char(), len_range)
        .prop_map(|chars| chars.into_iter().collect())
}

// ========================================
// ヘッダー値用 char / String strategy (RFC 9110 Section 5.5)
// ========================================

/// ヘッダー値の文字 (RFC 9110 Section 5.5 field-vchar + obs-text)
///
/// field-vchar = VCHAR / obs-text
/// obs-text = %x80-FF (Unicode scalar 拡張: U+0080..=U+10FFFF)
///
/// 注: VCHAR (0x21-0x7E) + SP + HTAB + obs-text を生成する。
/// 一部のヘッダー (Cookie octet等) は obs-text を許容しないが、
/// その制約は個別の strategy で扱う。
pub fn field_vchar() -> impl Strategy<Value = char> {
    prop_oneof![
        prop::char::range('!', '~'), // VCHAR: 0x21-0x7E
        Just(' '),                   // SP: 0x20
        Just('\t'),                  // HTAB: 0x09
        // obs-text (Unicode scalar 拡張, surrogate 除く)
        prop::char::range('\u{80}', '\u{D7FF}'),
        prop::char::range('\u{E000}', '\u{10FFFF}'),
    ]
}

/// ヘッダー値文字列
pub fn header_value() -> impl Strategy<Value = String> {
    proptest::collection::vec(field_vchar(), 1..=64).prop_map(|chars| chars.into_iter().collect())
}
