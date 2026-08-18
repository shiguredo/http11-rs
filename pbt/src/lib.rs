//! PBT テスト共通ユーティリティ
//!
//! プロパティテストは noprop の命令型 API で書く。
//! 各ヘルパーは `TestCaseContext` を受け取り、1 サンプル分の値を返す。

use core::ops::RangeInclusive;

use noprop::TestCaseContext;

// ========================================
// 言語タグ生成 (BCP 47/RFC 5646)
// ========================================

/// ALPHA (A-Z / a-z) の 1 文字を生成する
fn alpha_char(ctx: &mut TestCaseContext) -> char {
    let index = noprop::sample_usize_in(ctx, 0..52);
    let byte = if index < 26 {
        b'A' + index as u8
    } else {
        b'a' + (index - 26) as u8
    };
    byte as char
}

/// DIGIT (0-9) の 1 文字を生成する
fn digit_char(ctx: &mut TestCaseContext) -> char {
    char::from(b'0' + noprop::sample_usize_in(ctx, 0..10) as u8)
}

/// 先頭サブタグ: ALPHA のみ (1-8 文字)
pub fn language_primary_subtag(ctx: &mut TestCaseContext) -> String {
    let len = noprop::sample_usize_in(ctx, 1..=8);
    let mut s = String::with_capacity(len);
    for _ in 0..len {
        s.push(alpha_char(ctx));
    }
    s
}

/// 後続サブタグ: ALPHA / DIGIT (1-8 文字)
pub fn language_subsequent_subtag(ctx: &mut TestCaseContext) -> String {
    let len = noprop::sample_usize_in(ctx, 1..=8);
    let mut s = String::with_capacity(len);
    for _ in 0..len {
        match noprop::sample_usize_in(ctx, 0..2) {
            0 => s.push(alpha_char(ctx)),
            _ => s.push(digit_char(ctx)),
        }
    }
    s
}

/// 言語タグ: primary-subtag *("-" subtag)
pub fn language_tag(ctx: &mut TestCaseContext) -> String {
    let mut tag = language_primary_subtag(ctx);
    let rest_count = noprop::sample_usize_in(ctx, 0..=2);
    for _ in 0..rest_count {
        tag.push('-');
        tag.push_str(&language_subsequent_subtag(ctx));
    }
    tag
}

// ========================================
// quoted-string 用 char / String strategy (RFC 9110 Section 5.6.4)
// ========================================

/// 文字コード範囲 [start, end] の char を一様に生成する
///
/// start から end の間に surrogate が含まれないこと (呼び出し側で保証する)。
fn char_in_range(ctx: &mut TestCaseContext, start: u32, end: u32) -> char {
    let offset = noprop::sample_u64_in(ctx, 0..=(end - start) as u64) as u32;
    char::from_u32(start + offset)
        .expect("surrogate を含まない範囲なので変換は必ず成功するはず (実装バグ)")
}

/// 引用符内で使える文字 (qdtext + obs-text の Unicode scalar 拡張)
///
/// RFC 9110 Section 5.6.4 の qdtext ABNF (オクテット表現):
/// ```text
/// qdtext = HTAB / SP / %x21 / %x23-5B / %x5D-7E / obs-text
/// ```
/// を、char 単位走査の本実装に合わせて Unicode scalar に拡張解釈する。
/// surrogate (`U+D800..=U+DFFF`) は char 型で構築不能。
pub fn qdtext_char(ctx: &mut TestCaseContext) -> char {
    match noprop::sample_usize_in(ctx, 0..6) {
        0 => '\t',
        1 => ' ',
        2 => '!',
        3 => char_in_range(ctx, 0x23, 0x5B), // %x23-5B (DQUOTE 0x22 除外)
        4 => char_in_range(ctx, 0x5D, 0x7E), // %x5D-7E (バックスラッシュ 0x5C 除外)
        // obs-text を Unicode scalar として opaque 保持する範囲。
        // surrogate を跨がないよう二分割している。
        5 => match noprop::sample_usize_in(ctx, 0..2) {
            0 => char_in_range(ctx, 0x80, 0xD7FF),
            _ => char_in_range(ctx, 0xE000, 0x10FFFF),
        },
        _ => unreachable!("分岐は 6 本なので (実装バグ)"),
    }
}

/// 引用符付き文字列の中身 (エスケープなし、obs-text を含む)
///
/// 長さ範囲は呼び出し側で指定する。空文字列 (`0..=N`) を含む場合、ヘッダによっては
/// Display ラウンドトリップで `name=""` ではなく `name=` が出力されることに注意
/// (`needs_quoting` が空文字列で true を返すよう修正済みなら問題ない)。
pub fn qdtext_value(ctx: &mut TestCaseContext, len_range: RangeInclusive<usize>) -> String {
    let len = noprop::sample_usize_in(ctx, len_range);
    let mut s = String::with_capacity(len);
    for _ in 0..len {
        s.push(qdtext_char(ctx));
    }
    s
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
/// その制約は個別の生成関数で扱う。
pub fn field_vchar(ctx: &mut TestCaseContext) -> char {
    match noprop::sample_usize_in(ctx, 0..5) {
        0 => char_in_range(ctx, 0x21, 0x7E), // VCHAR: 0x21-0x7E
        1 => ' ',                            // SP: 0x20
        2 => '\t',                           // HTAB: 0x09
        // obs-text (Unicode scalar 拡張, surrogate 除く)
        3 => char_in_range(ctx, 0x80, 0xD7FF),
        _ => char_in_range(ctx, 0xE000, 0x10FFFF),
    }
}

/// ヘッダー値文字列
pub fn header_value(ctx: &mut TestCaseContext) -> String {
    let len = noprop::sample_usize_in(ctx, 1..=64);
    let mut s = String::with_capacity(len);
    for _ in 0..len {
        s.push(field_vchar(ctx));
    }
    s
}

// ========================================
// 構築時検査型の戦略 (HeaderName / Method / Scheme)
// ========================================

/// RFC 9110 Section 5.6.2: token = 1*tchar
///
/// tchar = "!" / "#" / "$" / "%" / "&" / "'" / "*"
///       / "+" / "-" / "." / "^" / "_" / "`" / "|" / "~"
///       / DIGIT / ALPHA
///
/// 大文字 A-Z と小文字 a-z の両方を含む。
pub fn valid_header_name(ctx: &mut TestCaseContext) -> Vec<u8> {
    let len = noprop::sample_usize_in(ctx, 1..=32);
    let mut v = Vec::new();
    for _ in 0..len {
        v.push(tchar_byte(ctx));
    }
    v
}

/// 不正なヘッダー名: 空 / 不正文字を含む
pub fn invalid_header_name(ctx: &mut TestCaseContext) -> Vec<u8> {
    match noprop::sample_usize_in(ctx, 0..2) {
        0 => Vec::new(),
        _ => {
            let prefix_len = noprop::sample_usize_in(ctx, 0..=8);
            let mut v = Vec::new();
            for _ in 0..prefix_len {
                v.push(tchar_byte(ctx));
            }
            v.push(invalid_header_name_byte(ctx));
            v
        }
    }
}

/// tchar の 1 バイト (RFC 9110 Section 5.6.2)
fn tchar_byte(ctx: &mut TestCaseContext) -> u8 {
    const SPECIAL: &[u8] = b"!#$%&'*+-.^_`|~";
    match noprop::sample_usize_in(ctx, 0..3) {
        0 => SPECIAL[noprop::sample_usize_in(ctx, 0..SPECIAL.len())],
        1 => b'0' + noprop::sample_usize_in(ctx, 0..10) as u8,
        _ => {
            let index = noprop::sample_usize_in(ctx, 0..52);
            if index < 26 {
                b'A' + index as u8
            } else {
                b'a' + (index - 26) as u8
            }
        }
    }
}

/// token として不正な 1 バイト (RFC 9110 Section 5.6.2 の tchar に含まれない文字)
fn invalid_header_name_byte(ctx: &mut TestCaseContext) -> u8 {
    const INVALID: &[u8] = &[
        b'\r', b'\n', 0x00, b':', b',', b' ', b'(', b')', b';', b'<', b'=', b'>', b'?', b'@', b'[',
        b'\\', b']', b'{', b'}', b'"',
    ];
    INVALID[noprop::sample_usize_in(ctx, 0..INVALID.len())]
}

/// RFC 9110 Section 9.1: method = token
pub fn valid_method(ctx: &mut TestCaseContext) -> Vec<u8> {
    valid_header_name(ctx)
}

pub fn invalid_method(ctx: &mut TestCaseContext) -> Vec<u8> {
    invalid_header_name(ctx)
}

/// ALPHA (A-Z / a-z) の 1 バイトを生成する
fn alpha_byte(ctx: &mut TestCaseContext) -> u8 {
    let index = noprop::sample_usize_in(ctx, 0..52);
    if index < 26 {
        b'A' + index as u8
    } else {
        b'a' + (index - 26) as u8
    }
}

/// RFC 3986 Section 3.1: scheme = ALPHA *( ALPHA / DIGIT / "+" / "-" / "." )
///
/// 先頭は ALPHA（大文字小文字両方）、2 文字目以降は ALPHA / DIGIT / "+" / "-" / "."
pub fn valid_scheme(ctx: &mut TestCaseContext) -> Vec<u8> {
    let mut v = vec![alpha_byte(ctx)];
    let rest_len = noprop::sample_usize_in(ctx, 0..=16);
    for _ in 0..rest_len {
        v.push(scheme_char_byte(ctx));
    }
    v
}

/// scheme の後続文字: ALPHA / DIGIT / "+" / "-" / "."
fn scheme_char_byte(ctx: &mut TestCaseContext) -> u8 {
    const SPECIAL: &[u8] = b"+-.";
    match noprop::sample_usize_in(ctx, 0..3) {
        0 => alpha_byte(ctx),
        1 => b'0' + noprop::sample_usize_in(ctx, 0..10) as u8,
        _ => SPECIAL[noprop::sample_usize_in(ctx, 0..SPECIAL.len())],
    }
}

/// 不正なスキーム: 空 / 数字開始 / コロンを含む / token だが scheme では不正な文字
pub fn invalid_scheme(ctx: &mut TestCaseContext) -> Vec<u8> {
    match noprop::sample_usize_in(ctx, 0..3) {
        0 => Vec::new(),
        1 => {
            // 数字開始
            let mut v = vec![b'0' + noprop::sample_usize_in(ctx, 0..10) as u8];
            let rest_len = noprop::sample_usize_in(ctx, 0..=8);
            for _ in 0..rest_len {
                v.push(scheme_char_byte(ctx));
            }
            v
        }
        _ => {
            // 末尾にコロンを含む
            let mut v = vec![alpha_byte(ctx)];
            let rest_len = noprop::sample_usize_in(ctx, 0..=6);
            for _ in 0..rest_len {
                v.push(scheme_char_byte(ctx));
            }
            v.push(b':');
            v
        }
    }
}
