//! ボディデコーダーの定義

use crate::error::Error;
use crate::limits::DecoderLimits;

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
                        trailers: std::mem::take(&mut self.trailers),
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
                        trailers: std::mem::take(&mut self.trailers),
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
                trailers: std::mem::take(&mut self.trailers),
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

            let line = String::from_utf8(buf[..pos].to_vec())
                .map_err(|e| Error::InvalidData(format!("invalid UTF-8: {e}")))?;
            buf.drain(..pos + 2);

            // チャンクサイズをパース (拡張は無視)
            let size_str = line.split(';').next().unwrap_or(&line).trim();
            let chunk_size = usize::from_str_radix(size_str, 16)
                .map_err(|_| Error::InvalidData(format!("invalid chunk size: {}", size_str)))?;

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

/// CRLF で終わる行を探す
pub(crate) fn find_line(buf: &[u8]) -> Option<usize> {
    buf.windows(2).position(|w| w == b"\r\n")
}

/// ヘッダー行をパース
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

    // ヘッダー値の検証 (RFC 9110 Section 5.5)
    let trimmed_value = value.trim();
    if !is_valid_field_value(trimmed_value) {
        return Err(Error::InvalidData(
            "invalid header line: invalid value (contains control characters)".to_string(),
        ));
    }

    Ok((name.to_string(), trimmed_value.to_string()))
}

/// ヘッダー名が有効か確認
pub(crate) fn is_valid_header_name(name: &str) -> bool {
    !name.is_empty() && name.bytes().all(is_token_char)
}

/// トークン文字か確認
pub(crate) fn is_token_char(b: u8) -> bool {
    matches!(
        b,
        b'!' | b'#' | b'$' | b'%' | b'&' | b'\'' | b'*' | b'+' | b'-' | b'.' |
        b'0'..=b'9' | b'A'..=b'Z' | b'^' | b'_' | b'`' | b'a'..=b'z' | b'|' | b'~'
    )
}

/// ヘッダー値に許可される文字か確認 (RFC 9110 Section 5.5)
///
/// field-value = *field-content
/// field-vchar = VCHAR / obs-text
/// VCHAR = %x21-7E (可視文字)
/// obs-text = %x80-FF
///
/// SP (0x20) と HTAB (0x09) も許可される (field-content の一部)
pub(crate) fn is_valid_field_vchar(b: u8) -> bool {
    matches!(b, 0x09 | 0x20..=0x7E | 0x80..=0xFF)
}

/// ヘッダー値が有効か確認 (RFC 9110 Section 5.5)
///
/// 制御文字 (0x00-0x08, 0x0A-0x1F, 0x7F) を含む場合は無効
pub(crate) fn is_valid_field_value(value: &str) -> bool {
    value.bytes().all(is_valid_field_vchar)
}

/// メソッド名が有効か確認
///
/// RFC 9110 Section 9 では method = token と定義されているが、
/// セキュリティ上の理由から大文字アルファベット、アンダースコア、ハイフンのみを許可する。
/// 小文字メソッドは正当なクライアントが使用しないため拒否する。
///
/// アンダースコアは RTSP (RFC 7826) の GET_PARAMETER, SET_PARAMETER などで使用されるため許可する。
pub(crate) fn is_valid_method(method: &str) -> bool {
    !method.is_empty()
        && method
            .bytes()
            .all(|b| matches!(b, b'A'..=b'Z' | b'_' | b'-'))
}

/// HTTP バージョンが有効か確認 (RFC 9112 Section 2.3)
///
/// HTTP-version = HTTP-name "/" DIGIT "." DIGIT
/// HTTP-name = %s"HTTP"
///
/// HTTP/1.0 または HTTP/1.1 のみ許可
pub(crate) fn is_valid_http_version(version: &str) -> bool {
    matches!(version, "HTTP/1.0" | "HTTP/1.1")
}

/// RFC 3986 で除外されている文字および request-target で許可されない文字
///
/// RFC 9112 Section 3.2: request-target は origin-form / absolute-form / authority-form / asterisk-form のいずれか
/// RFC 3986: absolute-URI にはフラグメントが含まれない (absolute-URI = scheme ":" hier-part [ "?" query ])
/// したがって request-target では "#" (フラグメント区切り) は許可されない
const RFC3986_EXCLUDED: &[u8] = b"\"#<>\\^`{|}";

/// リクエストターゲット (URI) が有効か確認
///
/// RFC 9112 Section 3: request-target には制御文字を含めない
/// RFC 3986 Section 2: URI で許可されない文字を拒否
///
/// 拒否する文字:
/// - 制御文字 (0x00-0x20, 0x7F)
/// - RFC 3986 で除外されている文字: " < > \ ^ ` { | }
/// - 不正なパーセントエンコーディング (% の後に 2 桁の 16 進数がない)
/// - パーセントエンコーディングされた NUL バイト (%00)
///
/// 許可する文字:
/// - VCHAR (0x21-0x7E) のうち RFC 3986 除外文字以外
/// - obs-text (0x80-0xFF) - RFC 9112 準拠
pub(crate) fn is_valid_request_target(target: &str) -> bool {
    if target.is_empty() {
        return false;
    }

    let bytes = target.as_bytes();
    let mut i = 0;

    while i < bytes.len() {
        let b = bytes[i];

        // 制御文字の拒否 (0x00-0x20, 0x7F)
        if b <= 0x20 || b == 0x7F {
            return false;
        }

        // RFC 3986 除外文字の拒否
        if RFC3986_EXCLUDED.contains(&b) {
            return false;
        }

        // パーセントエンコーディングの検証
        if b == b'%' {
            if i + 2 >= bytes.len() {
                return false; // 不完全
            }
            let high = bytes[i + 1];
            let low = bytes[i + 2];

            if !high.is_ascii_hexdigit() || !low.is_ascii_hexdigit() {
                return false; // 不正な 16 進数
            }

            // %00 (NUL) の拒否
            if high == b'0' && low == b'0' {
                return false;
            }

            i += 3;
            continue;
        }

        i += 1;
    }

    true
}

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

    // absolute-form: スキーム付き URI (http:// または https://)
    if target.contains("://") {
        // スキームの検証
        let scheme_end = target.find("://").unwrap();
        let scheme = &target[..scheme_end];
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
        return Ok(RequestTargetForm::Absolute);
    }

    // authority-form: host:port (CONNECT 用)
    validate_authority_form(target).map(|()| RequestTargetForm::Authority)
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

/// pchar または "/" か確認 (RFC 3986)
fn is_pchar_or_slash(b: u8) -> bool {
    is_pchar_byte(b) || b == b'/'
}

/// pchar か確認 (RFC 3986 Section 3.3)
/// pchar = unreserved / pct-encoded / sub-delims / ":" / "@"
fn is_pchar_byte(b: u8) -> bool {
    is_unreserved_byte(b) || is_sub_delim_byte(b) || b == b':' || b == b'@'
}

/// query で許可される文字か確認 (RFC 3986 Section 3.4)
/// query = *( pchar / "/" / "?" )
fn is_query_char(b: u8) -> bool {
    is_pchar_byte(b) || b == b'/' || b == b'?'
}

/// unreserved か確認 (RFC 3986 Section 2.3)
fn is_unreserved_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'-' || b == b'.' || b == b'_' || b == b'~'
}

/// sub-delims か確認 (RFC 3986 Section 2.2)
fn is_sub_delim_byte(b: u8) -> bool {
    matches!(
        b,
        b'!' | b'$' | b'&' | b'\'' | b'(' | b')' | b'*' | b'+' | b',' | b';' | b'='
    )
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

/// ステータスコードが有効か確認 (RFC 9110 Section 15)
///
/// ステータスコードは 3 桁の数字で、100-599 の範囲
pub(crate) fn is_valid_status_code(code: u16) -> bool {
    (100..=599).contains(&code)
}

/// reason-phrase が有効か確認 (RFC 9112 Section 4)
///
/// reason-phrase = 1*( HTAB / SP / VCHAR / obs-text )
/// VCHAR = %x21-7E
/// obs-text = %x80-FF
pub(crate) fn is_valid_reason_phrase(phrase: &str) -> bool {
    phrase
        .bytes()
        .all(|b| matches!(b, 0x09 | 0x20..=0x7E | 0x80..=0xFF))
}

/// Transfer-Encoding ヘッダーを解析
///
/// RFC 9112: chunked は一度だけ指定可能で、最後のエンコーディングでなければならない
/// 複数の Transfer-Encoding ヘッダーは連結して単一のリストとして扱う
pub(crate) fn parse_transfer_encoding_chunked(headers: &[(String, String)]) -> Result<bool, Error> {
    let mut chunked_count = 0;

    for (name, value) in headers {
        if name.eq_ignore_ascii_case("Transfer-Encoding") {
            let mut has_token = false;
            for token in value.split(',') {
                let token = token.trim();
                if token.is_empty() {
                    return Err(Error::InvalidData(
                        "invalid Transfer-Encoding: empty token".to_string(),
                    ));
                }
                has_token = true;

                if token.eq_ignore_ascii_case("chunked") {
                    chunked_count += 1;
                    if chunked_count > 1 {
                        return Err(Error::InvalidData(
                            "invalid Transfer-Encoding: duplicate chunked".to_string(),
                        ));
                    }
                } else {
                    // chunked 以外のエンコーディングはサポートしない
                    return Err(Error::InvalidData(
                        "invalid Transfer-Encoding: unsupported coding".to_string(),
                    ));
                }
            }
            if !has_token {
                return Err(Error::InvalidData(
                    "invalid Transfer-Encoding: empty value".to_string(),
                ));
            }
        }
    }

    Ok(chunked_count == 1)
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
fn parse_content_length_value(input: &str) -> Result<usize, Error> {
    let input = input.trim();
    if input.is_empty() || !input.chars().all(|c| c.is_ascii_digit()) {
        return Err(Error::InvalidData(
            "invalid Content-Length: not a number".to_string(),
        ));
    }
    input
        .parse::<usize>()
        .map_err(|_| Error::InvalidData("invalid Content-Length: overflow".to_string()))
}

/// ボディ関連ヘッダーを解決
pub(crate) fn resolve_body_headers(
    headers: &[(String, String)],
) -> Result<(bool, Option<usize>), Error> {
    let transfer_encoding_chunked = parse_transfer_encoding_chunked(headers)?;
    let content_length = parse_content_length(headers)?;

    if transfer_encoding_chunked && content_length.is_some() {
        return Err(Error::InvalidData(
            "invalid message: both Transfer-Encoding and Content-Length".to_string(),
        ));
    }

    Ok((transfer_encoding_chunked, content_length))
}
