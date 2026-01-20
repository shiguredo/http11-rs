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
    /// ボディなし
    None,
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
        }
    }

    /// リセット
    pub fn reset(&mut self) {
        self.trailers.clear();
        self.body_consumed = 0;
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
            DecodePhase::BodyChunkedDataCrlf
            | DecodePhase::ChunkedTrailer
            | DecodePhase::Complete => None,
            _ => None,
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
                self.body_consumed += len;

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
                self.body_consumed += len;

                if *remaining == 0 {
                    // チャンクデータ終了、CRLF 待ちへ遷移
                    *phase = DecodePhase::BodyChunkedDataCrlf;
                    // CRLF が既にバッファにあれば即座に処理
                    if buf.len() >= 2 {
                        buf.drain(..2);
                        *phase = DecodePhase::BodyChunkedSize;
                    }
                }

                Ok(BodyProgress::Continue)
            }
            DecodePhase::BodyChunkedDataCrlf => {
                // CRLF 待ち状態: バッファに CRLF があれば処理
                if buf.len() >= 2 {
                    buf.drain(..2);
                    *phase = DecodePhase::BodyChunkedSize;
                }
                Ok(BodyProgress::Continue)
            }
            DecodePhase::ChunkedTrailer => {
                self.process_trailers(buf, phase)?;

                match phase {
                    DecodePhase::Complete => Ok(BodyProgress::Complete {
                        trailers: std::mem::take(&mut self.trailers),
                    }),
                    _ => Ok(BodyProgress::Continue),
                }
            }
            DecodePhase::Complete => Ok(BodyProgress::Complete {
                trailers: std::mem::take(&mut self.trailers),
            }),
            _ => Err(Error::InvalidData(
                "consume_body called before decode_headers".to_string(),
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
            let line = String::from_utf8(buf[..pos].to_vec())
                .map_err(|e| Error::InvalidData(format!("invalid UTF-8: {e}")))?;
            buf.drain(..pos + 2);

            // チャンクサイズをパース (拡張は無視)
            let size_str = line.split(';').next().unwrap_or(&line).trim();
            let chunk_size = usize::from_str_radix(size_str, 16)
                .map_err(|_| Error::InvalidData(format!("invalid chunk size: {}", size_str)))?;

            if chunk_size == 0 {
                *phase = DecodePhase::ChunkedTrailer;
                return self.process_trailers(buf, phase);
            } else {
                let new_size = self.body_consumed + chunk_size;
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
    ) -> Result<(), Error> {
        while matches!(phase, DecodePhase::ChunkedTrailer) {
            if let Some(pos) = find_line(buf) {
                if pos == 0 {
                    buf.drain(..2);
                    *phase = DecodePhase::Complete;
                    return Ok(());
                } else {
                    let line = String::from_utf8(buf[..pos].to_vec())
                        .map_err(|e| Error::InvalidData(format!("invalid UTF-8: {e}")))?;
                    buf.drain(..pos + 2);

                    if let Ok((name, value)) = parse_header_line(&line) {
                        self.trailers.push((name, value));
                    }
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

    Ok((name.to_string(), value.trim().to_string()))
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

/// Transfer-Encoding ヘッダーを解析
pub(crate) fn parse_transfer_encoding_chunked(headers: &[(String, String)]) -> Result<bool, Error> {
    let mut found = false;
    for (name, value) in headers {
        if name.eq_ignore_ascii_case("Transfer-Encoding") {
            found = true;
            let mut has_token = false;
            for token in value.split(',') {
                let token = token.trim();
                if token.is_empty() {
                    return Err(Error::InvalidData(
                        "invalid Transfer-Encoding: empty token".to_string(),
                    ));
                }
                has_token = true;
                if !token.eq_ignore_ascii_case("chunked") {
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
    Ok(found)
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
