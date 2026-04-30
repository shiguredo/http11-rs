//! ボディデコーダーの定義
//!
//! # RFC 非準拠
//!
//! - RFC 9112 Section 2.2: HTTP/1.1 メッセージはオクテット列として解析すべき (SHOULD) だが、
//!   本実装ではチャンクサイズ行やトレーラーを UTF-8 として強制的に解析している。
//!   非 UTF-8 バイト列を含む場合はエラーとして拒否される。

use crate::error::Error;
use crate::limits::DecoderLimits;
use crate::request_target::RequestTargetForm;
use crate::trailer::is_prohibited_trailer_field;
use crate::validate::{
    is_pchar_or_slash, is_query_char, is_sub_delim_byte, is_token_char, is_unreserved_byte,
    is_valid_field_value, is_valid_header_name, is_valid_token,
};
use alloc::string::{String, ToString};
use alloc::vec::Vec;

use super::phase::DecodePhase;

/// ボディの種類
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BodyKind {
    /// Content-Length で指定された固定長
    ContentLength(usize),
    /// Transfer-Encoding: chunked
    Chunked,
    /// 接続が閉じるまでがボディ (close-delimited)
    ///
    /// RFC 9112: レスポンスで Transfer-Encoding も Content-Length もない場合、
    /// 接続が閉じられるまでをボディとして扱う
    CloseDelimited,
    /// ボディなし
    None,
    /// トンネルモード (CONNECT 2xx レスポンス用)
    ///
    /// RFC 9112 Section 6.3: CONNECT メソッドへの 2xx レスポンスは
    /// トンネルモードに切り替わり、Transfer-Encoding と Content-Length は無視される
    Tunnel,
}

/// ボディデコードの進捗
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BodyProgress {
    /// まだデータがある（続きを読む）
    Continue,
    /// 完了（トレーラーがある場合は含む）
    Complete { trailers: Vec<(String, String)> },
}

/// ボディデコーダー (内部用)
///
/// RequestDecoder と ResponseDecoder で共有されるボディデコードロジック
#[derive(Debug)]
pub(crate) struct BodyDecoder {
    /// トレーラーヘッダー
    trailers: Vec<(String, String)>,
    /// ボディ内での消費済みバイト数
    body_consumed: usize,
    /// トレーラー数
    trailer_count: usize,
}

impl Default for BodyDecoder {
    fn default() -> Self {
        Self::new()
    }
}

impl BodyDecoder {
    /// 新しいボディデコーダーを作成
    pub fn new() -> Self {
        Self {
            trailers: Vec::new(),
            body_consumed: 0,
            trailer_count: 0,
        }
    }

    /// リセット
    pub fn reset(&mut self) {
        self.trailers.clear();
        self.body_consumed = 0;
        self.trailer_count = 0;
    }

    /// 利用可能なボディデータを覗く（ゼロコピー）
    pub fn peek_body<'a>(&self, buf: &'a [u8], phase: &DecodePhase) -> Option<&'a [u8]> {
        match phase {
            DecodePhase::BodyContentLength { remaining } => {
                if buf.is_empty() {
                    return None;
                }
                let available = buf.len().min(*remaining);
                if available > 0 {
                    Some(&buf[..available])
                } else {
                    None
                }
            }
            DecodePhase::BodyChunkedSize => None,
            DecodePhase::BodyChunkedData { remaining } => {
                if buf.is_empty() {
                    return None;
                }
                let available = buf.len().min(*remaining);
                if available > 0 {
                    Some(&buf[..available])
                } else {
                    None
                }
            }
            DecodePhase::BodyCloseDelimited => {
                if buf.is_empty() {
                    return None;
                }
                Some(buf)
            }
            DecodePhase::BodyChunkedDataCrlf
            | DecodePhase::ChunkedTrailer
            | DecodePhase::Complete
            | DecodePhase::StartLine
            | DecodePhase::Headers
            | DecodePhase::Tunnel => None,
        }
    }

    /// ボディデータを消費
    pub fn consume_body(
        &mut self,
        buf: &mut Vec<u8>,
        phase: &mut DecodePhase,
        len: usize,
        limits: &DecoderLimits,
    ) -> Result<BodyProgress, Error> {
        match phase {
            DecodePhase::BodyContentLength { remaining } => {
                if len > *remaining {
                    return Err(Error::InvalidData(
                        "consume_body: len exceeds remaining".to_string(),
                    ));
                }
                if len > buf.len() {
                    return Err(Error::InvalidData(
                        "consume_body: len exceeds buffer".to_string(),
                    ));
                }

                buf.drain(..len);
                *remaining -= len;
                self.body_consumed =
                    self.body_consumed
                        .checked_add(len)
                        .ok_or(Error::BodyTooLarge {
                            size: usize::MAX,
                            limit: limits.max_body_size,
                        })?;

                if *remaining == 0 {
                    *phase = DecodePhase::Complete;
                    return Ok(BodyProgress::Complete {
                        trailers: Vec::new(),
                    });
                }

                Ok(BodyProgress::Continue)
            }
            DecodePhase::BodyChunkedSize => {
                self.process_chunked_size(buf, phase, limits)?;

                match phase {
                    DecodePhase::Complete => Ok(BodyProgress::Complete {
                        trailers: core::mem::take(&mut self.trailers),
                    }),
                    _ => Ok(BodyProgress::Continue),
                }
            }
            DecodePhase::BodyChunkedData { remaining } => {
                if len > *remaining {
                    return Err(Error::InvalidData(
                        "consume_body: len exceeds chunk remaining".to_string(),
                    ));
                }
                if len > buf.len() {
                    return Err(Error::InvalidData(
                        "consume_body: len exceeds buffer".to_string(),
                    ));
                }

                buf.drain(..len);
                *remaining -= len;
                self.body_consumed =
                    self.body_consumed
                        .checked_add(len)
                        .ok_or(Error::BodyTooLarge {
                            size: usize::MAX,
                            limit: limits.max_body_size,
                        })?;

                if *remaining == 0 {
                    // チャンクデータ終了、CRLF 待ちへ遷移
                    *phase = DecodePhase::BodyChunkedDataCrlf;
                    // CRLF が既にバッファにあれば即座に処理
                    if buf.len() >= 2 {
                        if buf[..2] != *b"\r\n" {
                            return Err(Error::InvalidData(
                                "invalid chunked encoding: expected CRLF after chunk data"
                                    .to_string(),
                            ));
                        }
                        buf.drain(..2);
                        *phase = DecodePhase::BodyChunkedSize;
                    }
                }

                Ok(BodyProgress::Continue)
            }
            DecodePhase::BodyChunkedDataCrlf => {
                // CRLF 待ち状態: バッファに CRLF があれば処理
                if buf.len() >= 2 {
                    if buf[..2] != *b"\r\n" {
                        return Err(Error::InvalidData(
                            "invalid chunked encoding: expected CRLF after chunk data".to_string(),
                        ));
                    }
                    buf.drain(..2);
                    *phase = DecodePhase::BodyChunkedSize;
                }
                Ok(BodyProgress::Continue)
            }
            DecodePhase::ChunkedTrailer => {
                self.process_trailers(buf, phase, limits)?;

                match phase {
                    DecodePhase::Complete => Ok(BodyProgress::Complete {
                        trailers: core::mem::take(&mut self.trailers),
                    }),
                    _ => Ok(BodyProgress::Continue),
                }
            }
            DecodePhase::BodyCloseDelimited => {
                // close-delimited: バッファにあるデータをすべて消費可能
                // Complete への遷移は mark_eof() で行う
                if len > buf.len() {
                    return Err(Error::InvalidData(
                        "consume_body: len exceeds buffer".to_string(),
                    ));
                }

                // max_body_size チェック (加算前にオーバーフロー検出)
                let new_size = self
                    .body_consumed
                    .checked_add(len)
                    .ok_or(Error::BodyTooLarge {
                        size: usize::MAX,
                        limit: limits.max_body_size,
                    })?;
                if new_size > limits.max_body_size {
                    return Err(Error::BodyTooLarge {
                        size: new_size,
                        limit: limits.max_body_size,
                    });
                }

                buf.drain(..len);
                self.body_consumed = new_size;

                // close-delimited は mark_eof() が呼ばれるまで Continue
                Ok(BodyProgress::Continue)
            }
            DecodePhase::Complete => Ok(BodyProgress::Complete {
                trailers: core::mem::take(&mut self.trailers),
            }),
            DecodePhase::StartLine | DecodePhase::Headers => Err(Error::InvalidData(
                "consume_body called before decode_headers".to_string(),
            )),
            DecodePhase::Tunnel => Err(Error::InvalidData(
                "consume_body cannot be used in tunnel mode, use take_remaining instead"
                    .to_string(),
            )),
        }
    }

    /// chunked のチャンクサイズ行を処理
    fn process_chunked_size(
        &mut self,
        buf: &mut Vec<u8>,
        phase: &mut DecodePhase,
        limits: &DecoderLimits,
    ) -> Result<(), Error> {
        if !matches!(phase, DecodePhase::BodyChunkedSize) {
            return Ok(());
        }

        if let Some(pos) = find_line(buf) {
            // チャンクサイズ行の長さ制限チェック
            if pos > limits.max_chunk_line_size {
                return Err(Error::ChunkLineTooLong {
                    size: pos,
                    limit: limits.max_chunk_line_size,
                });
            }

            // RFC 9112 Section 7.1.1: chunk extension の処理
            // 注: chunk extension は一般的に使われていない (RFC 9112 が "specialized service" 向けと明記)。
            // RFC 準拠のために処理しているが、内容は破棄する。
            // chunk-ext の quoted-string は obs-text を含む可能性があるため
            // UTF-8 変換せずバイト列として処理する。セミコロンまでを chunk-size として解釈。
            let line_bytes = &buf[..pos];

            // セミコロンの位置を探す (chunk-ext の開始)
            let semi_pos = line_bytes.iter().position(|&b| b == b';');
            let size_end = semi_pos.unwrap_or(pos);
            let size_bytes = &line_bytes[..size_end];

            // chunk-size = 1*HEXDIG (RFC 9112 Section 7.1)
            // HEXDIG の末尾位置を探す
            let hex_end = size_bytes
                .iter()
                .position(|b| !b.is_ascii_hexdigit())
                .unwrap_or(size_bytes.len());

            // chunk-size は 1 文字以上の HEXDIG で始まらなければならない
            if hex_end == 0 {
                let display = String::from_utf8_lossy(size_bytes);
                return Err(Error::InvalidData(format!(
                    "invalid chunk size: {}",
                    display
                )));
            }

            // HEXDIG の後にバイトがある場合の検証
            let trailing = &size_bytes[hex_end..];
            if !trailing.is_empty() {
                if semi_pos.is_some() {
                    // chunk-ext がある場合: HEXDIG と ";" の間は BWS (SP / HTAB) のみ許容
                    // RFC 9112 Section 7.1.1: chunk-ext = *( BWS ";" ... )
                    if !trailing.iter().all(|&b| b == b' ' || b == b'\t') {
                        let display = String::from_utf8_lossy(size_bytes);
                        return Err(Error::InvalidData(format!(
                            "invalid chunk size: {}",
                            display
                        )));
                    }
                } else {
                    // chunk-ext がない場合: chunk-size の後は CRLF のみ (BWS は不可)
                    let display = String::from_utf8_lossy(size_bytes);
                    return Err(Error::InvalidData(format!(
                        "invalid chunk size: {}",
                        display
                    )));
                }
            }

            // HEXDIG 部分のみを chunk-size として解釈
            let hex_bytes = &size_bytes[..hex_end];
            let size_str = core::str::from_utf8(hex_bytes)
                .map_err(|_| Error::InvalidData("invalid chunk size: not ASCII".to_string()))?;
            let chunk_size = usize::from_str_radix(size_str, 16)
                .map_err(|_| Error::InvalidData(format!("invalid chunk size: {}", size_str)))?;

            // chunk-ext の ABNF 検証 (RFC 9112 Section 7.1.1)
            if let Some(sp) = semi_pos {
                validate_chunk_ext(&line_bytes[sp..])?;
            }

            buf.drain(..pos + 2);

            if chunk_size == 0 {
                *phase = DecodePhase::ChunkedTrailer;
                return self.process_trailers(buf, phase, limits);
            } else {
                let new_size =
                    self.body_consumed
                        .checked_add(chunk_size)
                        .ok_or(Error::BodyTooLarge {
                            size: usize::MAX,
                            limit: limits.max_body_size,
                        })?;
                if new_size > limits.max_body_size {
                    return Err(Error::BodyTooLarge {
                        size: new_size,
                        limit: limits.max_body_size,
                    });
                }
                *phase = DecodePhase::BodyChunkedData {
                    remaining: chunk_size,
                };
            }
        }
        Ok(())
    }

    /// トレーラーヘッダーを処理
    fn process_trailers(
        &mut self,
        buf: &mut Vec<u8>,
        phase: &mut DecodePhase,
        limits: &DecoderLimits,
    ) -> Result<(), Error> {
        while matches!(phase, DecodePhase::ChunkedTrailer) {
            if let Some(pos) = find_line(buf) {
                if pos == 0 {
                    buf.drain(..2);
                    *phase = DecodePhase::Complete;
                    return Ok(());
                } else {
                    // 行長制限チェック
                    if pos > limits.max_header_line_size {
                        return Err(Error::HeaderLineTooLong {
                            size: pos,
                            limit: limits.max_header_line_size,
                        });
                    }

                    // 数制限チェック
                    if self.trailer_count >= limits.max_headers_count {
                        return Err(Error::TooManyHeaders {
                            count: self.trailer_count + 1,
                            limit: limits.max_headers_count,
                        });
                    }

                    let line = String::from_utf8(buf[..pos].to_vec())
                        .map_err(|e| Error::InvalidData(format!("invalid UTF-8: {e}")))?;
                    buf.drain(..pos + 2);

                    // 不正なトレーラー行はエラーにする
                    let (name, value) = parse_header_line(&line)?;

                    // RFC 9112 Section 7.1.2: 禁止フィールドチェック
                    if is_prohibited_trailer_field(&name) {
                        return Err(Error::InvalidData(format!(
                            "prohibited trailer field: {}",
                            name
                        )));
                    }

                    self.trailers.push((name, value));
                    self.trailer_count += 1;
                }
            } else {
                return Ok(());
            }
        }
        Ok(())
    }
}

/// chunk-ext の ABNF を検証する (RFC 9112 Section 7.1.1)
///
/// chunk-ext      = *( BWS ";" BWS chunk-ext-name [ BWS "=" BWS chunk-ext-val ] )
/// chunk-ext-name = token
/// chunk-ext-val  = token / quoted-string
/// quoted-string  = DQUOTE *( qdtext / quoted-pair ) DQUOTE
/// qdtext         = HTAB / SP / %x21 / %x23-5B / %x5D-7E / obs-text
/// quoted-pair    = "\" ( HTAB / SP / VCHAR / obs-text )
/// obs-text       = %x80-FF
///
/// 入力は ";" で始まるバイト列 (セミコロン以降のチャンク行)
fn validate_chunk_ext(ext: &[u8]) -> Result<(), Error> {
    let mut i = 0;

    while i < ext.len() {
        // BWS をスキップ
        i = skip_bws(ext, i);
        if i >= ext.len() {
            break;
        }

        // ";" を期待
        if ext[i] != b';' {
            return Err(Error::InvalidData(
                "invalid chunk-ext: expected ';'".to_string(),
            ));
        }
        i += 1;

        // BWS をスキップ
        i = skip_bws(ext, i);

        // chunk-ext-name = token (1*tchar)
        let name_start = i;
        while i < ext.len() && is_token_char(ext[i]) {
            i += 1;
        }
        if i == name_start {
            return Err(Error::InvalidData(
                "invalid chunk-ext: empty or invalid name".to_string(),
            ));
        }

        // BWS をスキップ
        i = skip_bws(ext, i);

        // "=" があれば chunk-ext-val を解析
        if i < ext.len() && ext[i] == b'=' {
            i += 1;

            // BWS をスキップ
            i = skip_bws(ext, i);

            if i >= ext.len() {
                return Err(Error::InvalidData(
                    "invalid chunk-ext: missing value after '='".to_string(),
                ));
            }

            if ext[i] == b'"' {
                // quoted-string
                i = parse_quoted_string(ext, i)?;
            } else {
                // token
                let val_start = i;
                while i < ext.len() && is_token_char(ext[i]) {
                    i += 1;
                }
                if i == val_start {
                    return Err(Error::InvalidData(
                        "invalid chunk-ext: empty or invalid value".to_string(),
                    ));
                }
            }
        }
    }

    Ok(())
}

/// BWS (Bad WhiteSpace) をスキップ (RFC 9110 Section 5.6.3)
///
/// BWS = OWS = *( SP / HTAB )
fn skip_bws(data: &[u8], mut pos: usize) -> usize {
    while pos < data.len() && (data[pos] == b' ' || data[pos] == b'\t') {
        pos += 1;
    }
    pos
}

/// quoted-string をパースして終了位置を返す (RFC 9110 Section 5.6.4)
///
/// quoted-string = DQUOTE *( qdtext / quoted-pair ) DQUOTE
/// qdtext        = HTAB / SP / %x21 / %x23-5B / %x5D-7E / obs-text
/// quoted-pair   = "\" ( HTAB / SP / VCHAR / obs-text )
fn parse_quoted_string(data: &[u8], start: usize) -> Result<usize, Error> {
    debug_assert_eq!(data[start], b'"');
    let mut i = start + 1;

    while i < data.len() {
        let b = data[i];
        if b == b'"' {
            return Ok(i + 1);
        }
        if b == b'\\' {
            // quoted-pair
            i += 1;
            if i >= data.len() {
                return Err(Error::InvalidData(
                    "invalid chunk-ext: incomplete quoted-pair".to_string(),
                ));
            }
            let escaped = data[i];
            // HTAB / SP / VCHAR / obs-text
            if escaped == b'\t'
                || escaped == b' '
                || (0x21..=0x7E).contains(&escaped)
                || escaped >= 0x80
            {
                i += 1;
            } else {
                return Err(Error::InvalidData(
                    "invalid chunk-ext: invalid quoted-pair character".to_string(),
                ));
            }
        } else if is_qdtext(b) {
            i += 1;
        } else {
            return Err(Error::InvalidData(
                "invalid chunk-ext: invalid character in quoted-string".to_string(),
            ));
        }
    }

    Err(Error::InvalidData(
        "invalid chunk-ext: unterminated quoted-string".to_string(),
    ))
}

/// qdtext か確認 (RFC 9110 Section 5.6.4)
///
/// qdtext = HTAB / SP / %x21 / %x23-5B / %x5D-7E / obs-text
fn is_qdtext(b: u8) -> bool {
    b == b'\t'
        || b == b' '
        || b == 0x21
        || (0x23..=0x5B).contains(&b)
        || (0x5D..=0x7E).contains(&b)
        || b >= 0x80
}

/// CRLF で終わる行を探す
pub(crate) fn find_line(buf: &[u8]) -> Option<usize> {
    buf.windows(2).position(|w| w == b"\r\n")
}

/// OWS (Optional Whitespace) を前後から除去 (RFC 9110 Section 5.6.3)
///
/// OWS = *( SP / HTAB )
/// Rust の str::trim() は Unicode 空白全般を除去するため使用しない
fn trim_ows(s: &str) -> &str {
    let bytes = s.as_bytes();
    let start = bytes
        .iter()
        .position(|&b| b != b' ' && b != b'\t')
        .unwrap_or(bytes.len());
    let end = bytes
        .iter()
        .rposition(|&b| b != b' ' && b != b'\t')
        .map(|i| i + 1)
        .unwrap_or(0);
    if start >= end {
        ""
    } else {
        // SP/HTAB は ASCII なので UTF-8 境界は安全
        &s[start..end]
    }
}

/// ヘッダー行をパース
///
/// # RFC 非準拠
///
/// 現在の実装ではヘッダー行を UTF-8 として解釈しており、obs-text (0x80-0xFF) を
/// バイト列として扱っていない。RFC 9110 Section 5.5 では obs-text は任意のバイト列
/// として定義されているが、本実装では UTF-8 として解釈するため、不正な UTF-8
/// シーケンスを含むヘッダー行は拒否される。現時点ではこの制限を維持する。
pub(crate) fn parse_header_line(line: &str) -> Result<(String, String), Error> {
    if line.starts_with(' ') || line.starts_with('\t') {
        return Err(Error::InvalidData(
            "invalid header line: obs-fold".to_string(),
        ));
    }
    if line.contains('\r') || line.contains('\n') {
        return Err(Error::InvalidData(
            "invalid header line: contains CR/LF".to_string(),
        ));
    }

    let (name, value) = line
        .split_once(':')
        .ok_or_else(|| Error::InvalidData("invalid header line: missing colon".to_string()))?;
    if name.is_empty() {
        return Err(Error::InvalidData(
            "invalid header line: empty name".to_string(),
        ));
    }
    if name != name.trim() || name.bytes().any(|b| b == b' ' || b == b'\t') {
        return Err(Error::InvalidData(
            "invalid header line: invalid name whitespace".to_string(),
        ));
    }
    if !is_valid_header_name(name) {
        return Err(Error::InvalidData(
            "invalid header line: invalid name".to_string(),
        ));
    }

    // ヘッダー値の OWS を除去 (RFC 9110 Section 5.5: OWS = *( SP / HTAB ))
    let trimmed_value = trim_ows(value);
    if !is_valid_field_value(trimmed_value) {
        return Err(Error::InvalidData(
            "invalid header line: invalid value (contains control characters)".to_string(),
        ));
    }

    Ok((name.to_string(), trimmed_value.to_string()))
}

/// request-target の形式を判定
///
/// RFC 9112 Section 3.2:
/// - origin-form: "/" で始まる (absolute-path [ "?" query ])
/// - absolute-form: スキーム付き URI
/// - authority-form: host:port (CONNECT 用)
/// - asterisk-form: "*"
pub(crate) fn parse_request_target_form(target: &str) -> Result<RequestTargetForm, Error> {
    if target.is_empty() {
        return Err(Error::InvalidData(
            "invalid request-target: empty".to_string(),
        ));
    }

    // asterisk-form: "*"
    if target == "*" {
        return Ok(RequestTargetForm::Asterisk);
    }

    // origin-form: "/" で始まる
    if target.starts_with('/') {
        return validate_origin_form(target).map(|()| RequestTargetForm::Origin);
    }

    // absolute-form: "://" を含む場合は明確に absolute-form
    if target.contains("://") {
        return validate_absolute_form(target);
    }

    // authority-form: host:port (CONNECT 用)
    // authority-form を先に試す (host:port は scheme:hier-part と文法的に曖昧なため)
    if validate_authority_form(target).is_ok() {
        return Ok(RequestTargetForm::Authority);
    }

    // absolute-form: "://" を含まない absolute-URI (例: urn:isbn:0451450523)
    if let Some(_scheme_len) = detect_scheme(target) {
        return validate_absolute_form(target);
    }

    Err(Error::InvalidData(
        "invalid request-target: unrecognized form".to_string(),
    ))
}

/// absolute-form の検証
///
/// RFC 3986: absolute-URI = scheme ":" hier-part [ "?" query ]
fn validate_absolute_form(target: &str) -> Result<RequestTargetForm, Error> {
    let scheme_len = detect_scheme(target)
        .ok_or_else(|| Error::InvalidData("invalid request-target: invalid scheme".to_string()))?;
    let scheme = &target[..scheme_len];
    // スキームの検証
    if !scheme
        .chars()
        .next()
        .is_some_and(|c| c.is_ascii_alphabetic())
    {
        return Err(Error::InvalidData(
            "invalid request-target: invalid scheme".to_string(),
        ));
    }
    for c in scheme.chars().skip(1) {
        if !c.is_ascii_alphanumeric() && c != '+' && c != '-' && c != '.' {
            return Err(Error::InvalidData(
                "invalid request-target: invalid scheme".to_string(),
            ));
        }
    }
    // absolute-URI にフラグメントは含まれない (RFC 3986)
    if target.contains('#') {
        return Err(Error::InvalidData(
            "invalid request-target: fragment not allowed".to_string(),
        ));
    }
    // IPv6 リテラルの括弧対応を検証 (RFC 3986 Section 3.2.2)
    validate_ipv6_brackets(target)?;
    // scheme ":" 以降の URI 文字検証 (RFC 3986)
    let rest = &target[scheme_len + 1..];
    validate_absolute_uri_parts(rest)?;

    // RFC 9110 Section 4.2.1/4.2.2: http/https URI の検証
    // http-URI  = "http"  "://" authority path-abempty [ "?" query ]
    // https-URI = "https" "://" authority path-abempty [ "?" query ]
    let scheme_lower = scheme.to_ascii_lowercase();
    if scheme_lower == "http" || scheme_lower == "https" {
        let Some(after_slashes) = rest.strip_prefix("//") else {
            return Err(Error::InvalidData(
                "http/https URI must contain \"://\" (RFC 9110 Section 4.2)".to_string(),
            ));
        };
        let authority_end = after_slashes
            .find(['/', '?'])
            .unwrap_or(after_slashes.len());
        let authority = &after_slashes[..authority_end];

        // RFC 9110 Section 4.2.4: http/https URI の userinfo を拒否する (SHOULD)
        if authority.contains('@') {
            return Err(Error::InvalidData(
                "userinfo not allowed in http/https URI (RFC 9110 Section 4.2.4)".to_string(),
            ));
        }

        // RFC 9110 Section 4.2.1/4.2.2: 空 host を拒否する (MUST)
        if authority.is_empty() || authority.starts_with(':') {
            return Err(Error::InvalidData(
                "empty host identifier in http/https URI (RFC 9110 Section 4.2)".to_string(),
            ));
        }
    }

    Ok(RequestTargetForm::Absolute)
}

/// スキームを検出する
///
/// RFC 3986 Section 3.1: scheme = ALPHA *( ALPHA / DIGIT / "+" / "-" / "." )
/// absolute-URI = scheme ":" hier-part [ "?" query ]
///
/// target の先頭が有効なスキーム + ":" であればスキームの長さを返す。
/// "://" を含む URI だけでなく、":" の後に "//" がない absolute-URI にも対応する。
fn detect_scheme(target: &str) -> Option<usize> {
    let bytes = target.as_bytes();

    // 最初の文字が ALPHA でなければスキームではない
    if bytes.is_empty() || !bytes[0].is_ascii_alphabetic() {
        return None;
    }

    // ":" を探す
    let colon_pos = bytes.iter().position(|&b| b == b':')?;

    // スキーム部分が空でないこと (最低 1 文字)
    if colon_pos == 0 {
        return None;
    }

    // スキーム文字の検証
    for &b in &bytes[1..colon_pos] {
        if !b.is_ascii_alphanumeric() && b != b'+' && b != b'-' && b != b'.' {
            return None;
        }
    }

    // 意図的な RFC 非準拠: path-empty (scheme ":" のみ) を拒否する。
    // RFC 3986 の ABNF では path-empty は合法だが、HTTP request-target として
    // path-empty が単独で出現する実用的なケースはないため、不正な入力として扱う。
    if colon_pos + 1 >= bytes.len() {
        return None;
    }

    Some(colon_pos)
}

/// IPv6 リテラルの括弧対応を検証
///
/// RFC 3986 Section 3.2.2:
/// IP-literal = "[" ( IPv6address / IPvFuture ) "]"
///
/// "[" があれば対応する "]" が必要
fn validate_ipv6_brackets(target: &str) -> Result<(), Error> {
    let open_count = target.chars().filter(|&c| c == '[').count();
    let close_count = target.chars().filter(|&c| c == ']').count();

    if open_count != close_count {
        return Err(Error::InvalidData(
            "invalid request-target: unmatched IPv6 brackets".to_string(),
        ));
    }

    // "[" の後に "]" があることを確認 (順序チェック)
    let mut depth = 0i32;
    for c in target.chars() {
        match c {
            '[' => depth += 1,
            ']' => {
                depth -= 1;
                if depth < 0 {
                    return Err(Error::InvalidData(
                        "invalid request-target: unmatched IPv6 brackets".to_string(),
                    ));
                }
            }
            _ => {}
        }
    }

    Ok(())
}

/// absolute-URI の hier-part と query の文字を検証する
///
/// RFC 3986: absolute-URI = scheme ":" hier-part [ "?" query ]
/// hier-part = "//" authority path-abempty / path-absolute / path-rootless / path-empty
///
/// authority 部分では "[" / "]" を IPv6 リテラル用に許可する
/// path/query 部分では pchar + "/" + "?" のみを許可する
fn validate_absolute_uri_parts(rest: &str) -> Result<(), Error> {
    // "//" で始まる場合は authority + path-abempty
    let path_start = if let Some(after_slashes) = rest.strip_prefix("//") {
        let authority_len = after_slashes
            .find(['/', '?'])
            .unwrap_or(after_slashes.len());
        validate_authority_chars(&after_slashes[..authority_len])?;
        authority_len + 2
    } else {
        0
    };

    let rest = &rest[path_start..];

    // "?" でパスとクエリを分割
    let (path, query) = if let Some(pos) = rest.find('?') {
        (&rest[..pos], Some(&rest[pos + 1..]))
    } else {
        (rest, None)
    };

    if !path.is_empty() {
        validate_path_chars(path)?;
    }
    if let Some(q) = query {
        validate_query_chars(q)?;
    }

    Ok(())
}

/// authority 部分の文字検証 (RFC 3986 Section 3.2)
///
/// authority = [ userinfo "@" ] host [ ":" port ]
/// "[" / "]" は IP-literal 用に許可する
fn validate_authority_chars(authority: &str) -> Result<(), Error> {
    let bytes = authority.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        let b = bytes[i];
        if b == b'%' {
            if i + 2 >= bytes.len()
                || !bytes[i + 1].is_ascii_hexdigit()
                || !bytes[i + 2].is_ascii_hexdigit()
            {
                return Err(Error::InvalidData(
                    "invalid authority: invalid percent-encoding".to_string(),
                ));
            }
            i += 3;
        } else if is_unreserved_byte(b)
            || is_sub_delim_byte(b)
            || b == b':'
            || b == b'@'
            || b == b'['
            || b == b']'
        {
            i += 1;
        } else {
            return Err(Error::InvalidData(format!(
                "invalid authority: illegal character 0x{:02X}",
                b
            )));
        }
    }
    Ok(())
}

/// origin-form の検証
///
/// RFC 9112 Section 3.2.1:
/// origin-form = absolute-path [ "?" query ]
fn validate_origin_form(target: &str) -> Result<(), Error> {
    if !target.starts_with('/') {
        return Err(Error::InvalidData(
            "invalid origin-form: must start with '/'".to_string(),
        ));
    }

    // フラグメントは request-target に含まれない
    if target.contains('#') {
        return Err(Error::InvalidData(
            "invalid request-target: fragment not allowed".to_string(),
        ));
    }

    // "?" でパスとクエリを分割
    let (path, query) = if let Some(pos) = target.find('?') {
        (&target[..pos], Some(&target[pos + 1..]))
    } else {
        (target, None)
    };

    // パスの検証 (RFC 3986 pchar + "/")
    validate_path_chars(path)?;

    // クエリの検証 (RFC 3986 query)
    if let Some(q) = query {
        validate_query_chars(q)?;
    }

    Ok(())
}

/// パス文字の検証 (RFC 3986 Section 3.3)
fn validate_path_chars(path: &str) -> Result<(), Error> {
    let bytes = path.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        let b = bytes[i];
        if b == b'%' {
            // パーセントエンコーディング検証
            if i + 2 >= bytes.len() {
                return Err(Error::InvalidData(
                    "invalid path: incomplete percent-encoding".to_string(),
                ));
            }
            if !bytes[i + 1].is_ascii_hexdigit() || !bytes[i + 2].is_ascii_hexdigit() {
                return Err(Error::InvalidData(
                    "invalid path: invalid percent-encoding".to_string(),
                ));
            }
            i += 3;
        } else if is_pchar_or_slash(b) {
            i += 1;
        } else {
            return Err(Error::InvalidData(format!(
                "invalid path: illegal character 0x{:02X}",
                b
            )));
        }
    }
    Ok(())
}

/// クエリ文字の検証 (RFC 3986 Section 3.4)
fn validate_query_chars(query: &str) -> Result<(), Error> {
    let bytes = query.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        let b = bytes[i];
        if b == b'%' {
            // パーセントエンコーディング検証
            if i + 2 >= bytes.len() {
                return Err(Error::InvalidData(
                    "invalid query: incomplete percent-encoding".to_string(),
                ));
            }
            if !bytes[i + 1].is_ascii_hexdigit() || !bytes[i + 2].is_ascii_hexdigit() {
                return Err(Error::InvalidData(
                    "invalid query: invalid percent-encoding".to_string(),
                ));
            }
            i += 3;
        } else if is_query_char(b) {
            i += 1;
        } else {
            return Err(Error::InvalidData(format!(
                "invalid query: illegal character 0x{:02X}",
                b
            )));
        }
    }
    Ok(())
}

/// authority-form の検証
///
/// RFC 9112 Section 3.2.3:
/// authority-form = uri-host ":" port
/// CONNECT メソッドでのみ使用
fn validate_authority_form(target: &str) -> Result<(), Error> {
    // ポートは必須
    let colon_pos = target
        .rfind(':')
        .ok_or_else(|| Error::InvalidData("invalid authority-form: missing port".to_string()))?;

    let host = &target[..colon_pos];
    let port_str = &target[colon_pos + 1..];

    // ホストが空でないこと
    if host.is_empty() {
        return Err(Error::InvalidData(
            "invalid authority-form: empty host".to_string(),
        ));
    }

    // ポートの検証
    if port_str.is_empty() {
        return Err(Error::InvalidData(
            "invalid authority-form: empty port".to_string(),
        ));
    }
    if !port_str.chars().all(|c| c.is_ascii_digit()) {
        return Err(Error::InvalidData(
            "invalid authority-form: port must be numeric".to_string(),
        ));
    }
    let _port: u16 = port_str
        .parse()
        .map_err(|_| Error::InvalidData("invalid authority-form: port out of range".to_string()))?;

    // Host パーサで host を検証
    crate::host::Host::parse(host)
        .map_err(|_| Error::InvalidData("invalid authority-form: invalid host".to_string()))?;

    Ok(())
}

/// メソッドと request-target 形式の組み合わせ検証
///
/// RFC 9112 Section 3.2:
/// - CONNECT: authority-form のみ
/// - OPTIONS: asterisk-form または origin-form/absolute-form
/// - その他: origin-form または absolute-form
pub(crate) fn validate_request_target_for_method(
    method: &str,
    form: &RequestTargetForm,
) -> Result<(), Error> {
    match method {
        "CONNECT" => {
            if *form != RequestTargetForm::Authority {
                return Err(Error::InvalidData(
                    "CONNECT method requires authority-form request-target".to_string(),
                ));
            }
        }
        "OPTIONS" => {
            // OPTIONS は asterisk-form, origin-form, absolute-form を許可
            if *form == RequestTargetForm::Authority {
                return Err(Error::InvalidData(
                    "OPTIONS method does not allow authority-form request-target".to_string(),
                ));
            }
        }
        _ => {
            // その他のメソッドは origin-form または absolute-form のみ
            match form {
                RequestTargetForm::Origin | RequestTargetForm::Absolute => {}
                RequestTargetForm::Authority => {
                    return Err(Error::InvalidData(format!(
                        "{} method does not allow authority-form request-target",
                        method
                    )));
                }
                RequestTargetForm::Asterisk => {
                    return Err(Error::InvalidData(format!(
                        "{} method does not allow asterisk-form request-target",
                        method
                    )));
                }
            }
        }
    }
    Ok(())
}

/// Transfer-Encoding 解析結果
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TransferEncodingResult {
    /// Transfer-Encoding なし
    None,
    /// chunked が最後 (chunked フレーミング)
    Chunked,
    /// chunked がないか最後でない (レスポンス: close-delimited)
    Other,
}

/// Transfer-Encoding ヘッダーを解析 (リクエスト用)
///
/// RFC 9112 Section 6.1: リクエストでは chunked 以外のエンコーディングを
/// サーバーがサポートしているか不明なため、chunked のみ許可する
///
/// - chunked のみ → Ok(true)
/// - chunked 以外がある → Err (RFC: 400 Bad Request)
/// - Transfer-Encoding なし → Ok(false)
pub(crate) fn parse_transfer_encoding_for_request(
    headers: &[(String, String)],
) -> Result<bool, Error> {
    let mut chunked_count = 0;

    for (name, value) in headers {
        if name.eq_ignore_ascii_case("Transfer-Encoding") {
            for token in value.split(',') {
                let token = token.trim();
                // RFC 9110 Section 5.6.1.2: 受信者は空リスト要素を無視する (MUST)
                if token.is_empty() {
                    continue;
                }

                // RFC 9112 Section 7.1: chunked のパラメータは定義されていない
                let base_coding = token.split(';').next().unwrap_or(token).trim();
                // RFC 9110 Section 10.1.4: transfer-coding = token
                if !is_valid_token(base_coding) {
                    return Err(Error::InvalidData(
                        "invalid Transfer-Encoding: not a valid token".to_string(),
                    ));
                }
                if base_coding.eq_ignore_ascii_case("chunked") {
                    if token.contains(';') {
                        return Err(Error::InvalidData(
                            "invalid Transfer-Encoding: chunked does not accept parameters (RFC 9112 Section 7.1)".to_string(),
                        ));
                    }
                    chunked_count += 1;
                    if chunked_count > 1 {
                        return Err(Error::InvalidData(
                            "invalid Transfer-Encoding: duplicate chunked".to_string(),
                        ));
                    }
                } else {
                    // リクエストでは chunked 以外のエンコーディングはサポートしない
                    return Err(Error::InvalidData(
                        "invalid Transfer-Encoding: unsupported coding".to_string(),
                    ));
                }
            }
        }
    }

    Ok(chunked_count == 1)
}

/// Transfer-Encoding ヘッダーを解析 (レスポンス用)
///
/// RFC 9112 Section 6.1:
/// - chunked が最後のエンコーディング → Chunked (chunked フレーミング)
/// - chunked がないか最後でない → Other (close-delimited)
/// - Transfer-Encoding なし → None
pub(crate) fn parse_transfer_encoding_for_response(
    headers: &[(String, String)],
) -> Result<TransferEncodingResult, Error> {
    // すべての Transfer-Encoding ヘッダーを連結してトークンリストを作成
    let mut all_tokens: Vec<String> = Vec::new();
    let mut chunked_count = 0;

    for (name, value) in headers {
        if name.eq_ignore_ascii_case("Transfer-Encoding") {
            for token in value.split(',') {
                let token = token.trim();
                // RFC 9110 Section 5.6.1.2: 受信者は空リスト要素を無視する (MUST)
                if token.is_empty() {
                    continue;
                }

                // RFC 9112 Section 7.1: chunked のパラメータは定義されていない
                let base_coding = token.split(';').next().unwrap_or(token).trim();
                // RFC 9110 Section 10.1.4: transfer-coding = token
                if !is_valid_token(base_coding) {
                    return Err(Error::InvalidData(
                        "invalid Transfer-Encoding: not a valid token".to_string(),
                    ));
                }
                if base_coding.eq_ignore_ascii_case("chunked") {
                    if token.contains(';') {
                        return Err(Error::InvalidData(
                            "invalid Transfer-Encoding: chunked does not accept parameters (RFC 9112 Section 7.1)".to_string(),
                        ));
                    }
                    chunked_count += 1;
                    if chunked_count > 1 {
                        return Err(Error::InvalidData(
                            "invalid Transfer-Encoding: duplicate chunked".to_string(),
                        ));
                    }
                }
                all_tokens.push(base_coding.to_ascii_lowercase());
            }
        }
    }

    if all_tokens.is_empty() {
        return Ok(TransferEncodingResult::None);
    }

    // RFC 9112 Section 6.3: chunked が最後の場合のみ chunked フレーミング
    if all_tokens.last().map(|s| s.as_str()) == Some("chunked") {
        Ok(TransferEncodingResult::Chunked)
    } else {
        // chunked がないか最後でない場合は close-delimited
        Ok(TransferEncodingResult::Other)
    }
}

/// Content-Length ヘッダーを解析
pub(crate) fn parse_content_length(headers: &[(String, String)]) -> Result<Option<usize>, Error> {
    let mut value: Option<usize> = None;
    for (name, raw_value) in headers {
        if name.eq_ignore_ascii_case("Content-Length") {
            let parsed = parse_content_length_value(raw_value)?;
            if let Some(prev) = value {
                if prev != parsed {
                    return Err(Error::InvalidData(
                        "invalid Content-Length: mismatched values".to_string(),
                    ));
                }
            } else {
                value = Some(parsed);
            }
        }
    }
    Ok(value)
}

/// Content-Length 値をパース
///
/// RFC 9110 Section 8.6: Content-Length はカンマ区切りで複数値を持てる
/// すべての値が同一でなければならない
fn parse_content_length_value(input: &str) -> Result<usize, Error> {
    let mut result: Option<usize> = None;

    for part in input.split(',') {
        let part = part.trim();
        if part.is_empty() {
            return Err(Error::InvalidData(
                "invalid Content-Length: empty value in list".to_string(),
            ));
        }
        if !part.chars().all(|c| c.is_ascii_digit()) {
            return Err(Error::InvalidData(
                "invalid Content-Length: not a number".to_string(),
            ));
        }
        let value = part
            .parse::<usize>()
            .map_err(|_| Error::InvalidData("invalid Content-Length: overflow".to_string()))?;

        match result {
            None => result = Some(value),
            Some(prev) if prev != value => {
                return Err(Error::InvalidData(
                    "invalid Content-Length: mismatched values in list".to_string(),
                ));
            }
            Some(_) => {} // 同じ値なので OK
        }
    }

    result.ok_or_else(|| Error::InvalidData("invalid Content-Length: empty".to_string()))
}

/// リクエスト用: ボディヘッダー解決
///
/// RFC 9112 Section 6.3:
/// - Transfer-Encoding と Content-Length の両方がある場合はエラー
/// - リクエストでは chunked 以外の Transfer-Encoding は拒否
pub(crate) fn resolve_body_headers_for_request(
    headers: &[(String, String)],
) -> Result<(bool, Option<usize>), Error> {
    let transfer_encoding_chunked = parse_transfer_encoding_for_request(headers)?;
    let content_length = parse_content_length(headers)?;

    if transfer_encoding_chunked && content_length.is_some() {
        return Err(Error::InvalidData(
            "invalid message: both Transfer-Encoding and Content-Length".to_string(),
        ));
    }

    Ok((transfer_encoding_chunked, content_length))
}

/// レスポンス用: ボディヘッダー解決
///
/// RFC 9112 Section 6.3:
/// - Transfer-Encoding と Content-Length の両方がある場合、Transfer-Encoding を優先
///   (Content-Length は無視。ただし警告ログを出すべきとあるが、本実装では無視のみ)
/// - chunked が最後でない場合は close-delimited
pub(crate) fn resolve_body_headers_for_response(
    headers: &[(String, String)],
) -> Result<(TransferEncodingResult, Option<usize>), Error> {
    let te_result = parse_transfer_encoding_for_response(headers)?;
    let content_length = parse_content_length(headers)?;

    // RFC 9112 Section 6.3: Transfer-Encoding がある場合は Content-Length を無視
    // (リクエスト送信者はこの組み合わせを送るべきではないが、受信者は TE を優先)
    if te_result != TransferEncodingResult::None {
        return Ok((te_result, None));
    }

    Ok((TransferEncodingResult::None, content_length))
}
