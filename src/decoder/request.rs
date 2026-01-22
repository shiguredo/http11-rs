//! HTTP リクエストデコーダー

use crate::error::Error;
use crate::limits::DecoderLimits;
use crate::request::Request;

use super::body::{
    BodyDecoder, BodyKind, BodyProgress, find_line, parse_header_line, resolve_body_headers,
};
use super::head::RequestHead;
use super::phase::DecodePhase;

/// HTTP リクエストデコーダー (Sans I/O)
///
/// サーバー側でクライアントからのリクエストをパースする際に使用
#[derive(Debug)]
pub struct RequestDecoder {
    buf: Vec<u8>,
    phase: DecodePhase,
    start_line: Option<String>,
    headers: Vec<(String, String)>,
    body_decoder: BodyDecoder,
    limits: DecoderLimits,
    /// decode() 用: デコード済みヘッダー
    decoded_head: Option<RequestHead>,
    /// decode() 用: ボディ種別
    decoded_body_kind: Option<BodyKind>,
    /// decode() 用: デコード済みボディ
    decoded_body: Vec<u8>,
}

impl Default for RequestDecoder {
    fn default() -> Self {
        Self::new()
    }
}

impl RequestDecoder {
    /// 新しいデコーダーを作成
    pub fn new() -> Self {
        Self {
            buf: Vec::new(),
            phase: DecodePhase::StartLine,
            start_line: None,
            headers: Vec::new(),
            body_decoder: BodyDecoder::new(),
            limits: DecoderLimits::default(),
            decoded_head: None,
            decoded_body_kind: None,
            decoded_body: Vec::new(),
        }
    }

    /// 制限付きでデコーダーを作成
    pub fn with_limits(limits: DecoderLimits) -> Self {
        Self {
            buf: Vec::new(),
            phase: DecodePhase::StartLine,
            start_line: None,
            headers: Vec::new(),
            body_decoder: BodyDecoder::new(),
            limits,
            decoded_head: None,
            decoded_body_kind: None,
            decoded_body: Vec::new(),
        }
    }

    /// 制限設定を取得
    pub fn limits(&self) -> &DecoderLimits {
        &self.limits
    }

    /// バッファにデータを追加
    pub fn feed(&mut self, data: &[u8]) -> Result<(), Error> {
        let new_size = self.buf.len() + data.len();
        if new_size > self.limits.max_buffer_size {
            return Err(Error::BufferOverflow {
                size: new_size,
                limit: self.limits.max_buffer_size,
            });
        }
        self.buf.extend_from_slice(data);
        Ok(())
    }

    /// バッファにデータを追加 (制限チェックなし)
    pub fn feed_unchecked(&mut self, data: &[u8]) {
        self.buf.extend_from_slice(data);
    }

    /// バッファの残りデータを取得
    pub fn remaining(&self) -> &[u8] {
        &self.buf
    }

    /// デコーダーをリセット
    pub fn reset(&mut self) {
        self.buf.clear();
        self.phase = DecodePhase::StartLine;
        self.start_line = None;
        self.headers.clear();
        self.body_decoder.reset();
        self.decoded_head = None;
        self.decoded_body_kind = None;
        self.decoded_body.clear();
    }

    /// ボディモードを決定
    fn determine_body_kind(&self) -> Result<BodyKind, Error> {
        let (transfer_encoding_chunked, content_length) = resolve_body_headers(&self.headers)?;

        if transfer_encoding_chunked {
            return Ok(BodyKind::Chunked);
        }

        if let Some(len) = content_length {
            if len > self.limits.max_body_size {
                return Err(Error::BodyTooLarge {
                    size: len,
                    limit: self.limits.max_body_size,
                });
            }
            return Ok(BodyKind::ContentLength(len));
        }

        Ok(BodyKind::None)
    }

    /// ヘッダーをデコード
    ///
    /// ヘッダーが完了したら `Some((RequestHead, BodyKind))` を返す
    /// データ不足の場合は `None` を返す
    /// 既にヘッダーデコード済みの場合はエラー
    pub fn decode_headers(&mut self) -> Result<Option<(RequestHead, BodyKind)>, Error> {
        loop {
            match &self.phase {
                DecodePhase::StartLine => {
                    if let Some(pos) = find_line(&self.buf) {
                        let line = String::from_utf8(self.buf[..pos].to_vec())
                            .map_err(|e| Error::InvalidData(format!("invalid UTF-8: {e}")))?;
                        self.buf.drain(..pos + 2);
                        if line.contains('\r') || line.contains('\n') {
                            return Err(Error::InvalidData(
                                "invalid request line: contains CR/LF".to_string(),
                            ));
                        }

                        // Parse: METHOD SP URI SP VERSION CRLF
                        let parts: Vec<&str> = line.splitn(3, ' ').collect();
                        if parts.len() != 3 {
                            return Err(Error::InvalidData(format!(
                                "invalid request line: {}",
                                line
                            )));
                        }

                        self.start_line = Some(line);
                        self.phase = DecodePhase::Headers;
                    } else {
                        return Ok(None);
                    }
                }
                DecodePhase::Headers => {
                    if let Some(pos) = find_line(&self.buf) {
                        if pos == 0 {
                            // Empty line - end of headers
                            self.buf.drain(..2);

                            let body_kind = self.determine_body_kind()?;

                            // ヘッダー完了、ボディフェーズに遷移
                            // RFC 9112: リクエストは close-delimited を使わない
                            match body_kind {
                                BodyKind::ContentLength(len) => {
                                    if len > 0 {
                                        self.phase =
                                            DecodePhase::BodyContentLength { remaining: len };
                                    } else {
                                        self.phase = DecodePhase::Complete;
                                    }
                                }
                                BodyKind::Chunked => {
                                    self.phase = DecodePhase::BodyChunkedSize;
                                }
                                BodyKind::CloseDelimited | BodyKind::None => {
                                    self.phase = DecodePhase::Complete;
                                }
                            }

                            // RequestHead を構築
                            let start_line = self.start_line.take().ok_or_else(|| {
                                Error::InvalidData("missing request line".to_string())
                            })?;
                            let parts: Vec<&str> = start_line.splitn(3, ' ').collect();

                            let head = RequestHead {
                                method: parts[0].to_string(),
                                uri: parts[1].to_string(),
                                version: parts[2].to_string(),
                                headers: std::mem::take(&mut self.headers),
                            };

                            return Ok(Some((head, body_kind)));
                        } else {
                            // Check header line size limit
                            if pos > self.limits.max_header_line_size {
                                return Err(Error::HeaderLineTooLong {
                                    size: pos,
                                    limit: self.limits.max_header_line_size,
                                });
                            }

                            // Check header count limit
                            if self.headers.len() >= self.limits.max_headers_count {
                                return Err(Error::TooManyHeaders {
                                    count: self.headers.len() + 1,
                                    limit: self.limits.max_headers_count,
                                });
                            }

                            let line = String::from_utf8(self.buf[..pos].to_vec())
                                .map_err(|e| Error::InvalidData(format!("invalid UTF-8: {e}")))?;
                            self.buf.drain(..pos + 2);

                            let (name, value) = parse_header_line(&line)?;
                            self.headers.push((name, value));
                        }
                    } else {
                        return Ok(None);
                    }
                }
                DecodePhase::Complete => {
                    // 完了状態から次のメッセージへ遷移
                    self.phase = DecodePhase::StartLine;
                    self.start_line = None;
                    self.headers.clear();
                    self.body_decoder.reset();
                    continue;
                }
                _ => {
                    return Err(Error::InvalidData(
                        "decode_headers called during body decoding".to_string(),
                    ));
                }
            }
        }
    }

    /// 利用可能なボディデータを覗く（ゼロコピー）
    ///
    /// `decode_headers()` 成功後に呼ぶ
    /// データがある場合はスライスを返す
    /// ボディがない場合や完了済みの場合は `None` を返す
    pub fn peek_body(&self) -> Option<&[u8]> {
        self.body_decoder.peek_body(&self.buf, &self.phase)
    }

    /// 利用可能なボディデータのバイト数を取得
    fn available_body_len(&self) -> usize {
        match &self.phase {
            DecodePhase::BodyContentLength { remaining } => self.buf.len().min(*remaining),
            DecodePhase::BodyChunkedData { remaining } => self.buf.len().min(*remaining),
            _ => 0,
        }
    }

    /// ボディデータを消費
    ///
    /// `peek_body()` で取得したデータを処理した後に呼ぶ
    /// `len` は消費するバイト数 (1 以上)
    pub fn consume_body(&mut self, len: usize) -> Result<BodyProgress, Error> {
        if len == 0 {
            return Err(Error::InvalidData(
                "consume_body(0) is not allowed, use progress() instead".to_string(),
            ));
        }
        self.body_decoder
            .consume_body(&mut self.buf, &mut self.phase, len, &self.limits)
    }

    /// 状態機械を進める (ボディデータは消費しない)
    ///
    /// Chunked エンコーディングの場合、チャンクサイズ行のパースや
    /// 終端チャンクの処理を行う。
    pub fn progress(&mut self) -> Result<BodyProgress, Error> {
        self.body_decoder
            .consume_body(&mut self.buf, &mut self.phase, 0, &self.limits)
    }

    /// リクエスト全体を一括でデコード
    ///
    /// ストリーミング API (`decode_headers()` / `peek_body()` / `consume_body()`) を
    /// 内部で使用して、リクエスト全体をデコードする。
    ///
    /// データ不足の場合は `None` を返す。
    /// ストリーミング API と混在使用するとエラーを返す。
    pub fn decode(&mut self) -> Result<Option<Request>, Error> {
        // ヘッダーがまだデコードされていない場合はデコード
        if self.decoded_head.is_none() {
            match self.phase {
                DecodePhase::StartLine | DecodePhase::Headers => match self.decode_headers()? {
                    Some((head, body_kind)) => {
                        self.decoded_head = Some(head);
                        self.decoded_body_kind = Some(body_kind);
                    }
                    None => return Ok(None),
                },
                _ => {
                    return Err(Error::InvalidData(
                        "decode cannot be mixed with streaming API".to_string(),
                    ));
                }
            }
        }

        // ボディを読む
        // RFC 9112: リクエストは close-delimited を使わないため、CloseDelimited は None と同じ
        let body_kind = *self.decoded_body_kind.as_ref().unwrap();
        match body_kind {
            BodyKind::ContentLength(_) | BodyKind::Chunked => loop {
                // 直接バッファから利用可能なデータ長を取得（コピーなし）
                let available = self.available_body_len();
                if available > 0 {
                    // バッファから直接コピー
                    self.decoded_body.extend_from_slice(&self.buf[..available]);
                    match self.consume_body(available)? {
                        BodyProgress::Complete { .. } => break,
                        BodyProgress::Continue => continue,
                    }
                }

                // データがない場合、状態機械を進める
                match self.progress()? {
                    BodyProgress::Complete { .. } => break,
                    BodyProgress::Continue => {
                        // 状態遷移後にデータが利用可能になったか確認
                        if self.available_body_len() > 0 {
                            continue;
                        }
                        // データ不足
                        return Ok(None);
                    }
                }
            },
            BodyKind::CloseDelimited | BodyKind::None => {}
        }

        // Request を構築
        let head = self.decoded_head.take().unwrap();
        let body = std::mem::take(&mut self.decoded_body);

        // Keep-Alive 対応: 次のリクエストのために状態をリセット
        self.phase = DecodePhase::StartLine;
        self.decoded_body_kind = None;
        self.body_decoder.reset();

        Ok(Some(Request {
            method: head.method,
            uri: head.uri,
            version: head.version,
            headers: head.headers,
            body,
        }))
    }
}
