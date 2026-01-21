//! HTTP レスポンスデコーダー

use crate::error::Error;
use crate::limits::DecoderLimits;
use crate::response::Response;

use super::body::{
    BodyDecoder, BodyKind, BodyProgress, find_line, parse_header_line, resolve_body_headers,
};
use super::head::ResponseHead;
use super::phase::DecodePhase;

/// HTTP レスポンスデコーダー (Sans I/O)
///
/// クライアント側でサーバーからのレスポンスをパースする際に使用
#[derive(Debug)]
pub struct ResponseDecoder {
    buf: Vec<u8>,
    phase: DecodePhase,
    start_line: Option<String>,
    headers: Vec<(String, String)>,
    body_decoder: BodyDecoder,
    limits: DecoderLimits,
    /// HEAD リクエストへのレスポンスかどうか
    expect_no_body: bool,
    /// ステータスコード（ヘッダーデコード後に保持）
    status_code: u16,
    /// decode() 用: デコード済みヘッダー
    decoded_head: Option<ResponseHead>,
    /// decode() 用: ボディ種別
    decoded_body_kind: Option<BodyKind>,
    /// decode() 用: デコード済みボディ
    decoded_body: Vec<u8>,
}

impl Default for ResponseDecoder {
    fn default() -> Self {
        Self::new()
    }
}

impl ResponseDecoder {
    /// 新しいデコーダーを作成
    pub fn new() -> Self {
        Self {
            buf: Vec::new(),
            phase: DecodePhase::StartLine,
            start_line: None,
            headers: Vec::new(),
            body_decoder: BodyDecoder::new(),
            limits: DecoderLimits::default(),
            expect_no_body: false,
            status_code: 0,
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
            expect_no_body: false,
            status_code: 0,
            decoded_head: None,
            decoded_body_kind: None,
            decoded_body: Vec::new(),
        }
    }

    /// HEAD リクエストへのレスポンスとしてデコード (ボディなし)
    pub fn set_expect_no_body(&mut self, expect_no_body: bool) {
        self.expect_no_body = expect_no_body;
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
        self.expect_no_body = false;
        self.status_code = 0;
        self.decoded_head = None;
        self.decoded_body_kind = None;
        self.decoded_body.clear();
    }

    /// 接続終了を通知 (close-delimited ボディ用)
    ///
    /// close-delimited ボディを読み取り中に接続が閉じられた場合に呼び出す。
    /// これにより、バッファ内の残りデータがボディとして確定し、Complete に遷移する。
    ///
    /// close-delimited 以外の状態で呼び出した場合は何もしない。
    pub fn mark_eof(&mut self) {
        if matches!(self.phase, DecodePhase::BodyCloseDelimited) {
            self.phase = DecodePhase::Complete;
        }
    }

    /// close-delimited ボディを読み取り中かどうかを判定
    pub fn is_close_delimited(&self) -> bool {
        matches!(self.phase, DecodePhase::BodyCloseDelimited)
    }

    /// ステータスコードからボディがあるかどうかを判定
    fn status_has_body(status_code: u16) -> bool {
        // 1xx, 204, 304 はボディなし
        !((100..200).contains(&status_code) || status_code == 204 || status_code == 304)
    }

    /// ボディモードを決定
    ///
    /// RFC 9112 Section 6.3 の優先順位に従う:
    /// 1. HEAD レスポンス、1xx/204/304 はボディなし
    /// 2. Transfer-Encoding がある場合は chunked
    /// 3. Content-Length がある場合は固定長
    /// 4. それ以外は close-delimited (接続が閉じるまでがボディ)
    fn determine_body_kind(&self, status_code: u16) -> Result<BodyKind, Error> {
        let (transfer_encoding_chunked, content_length) = resolve_body_headers(&self.headers)?;

        // HEAD リクエストへのレスポンス、または 1xx/204/304 はボディなし
        if self.expect_no_body || !Self::status_has_body(status_code) {
            return Ok(BodyKind::None);
        }

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

        // RFC 9112: TE も CL もない場合は close-delimited
        // 接続が閉じられるまでをボディとして扱う
        Ok(BodyKind::CloseDelimited)
    }

    /// ヘッダーをデコード
    ///
    /// ヘッダーが完了したら `Some((ResponseHead, BodyKind))` を返す
    /// データ不足の場合は `None` を返す
    /// 既にヘッダーデコード済みの場合はエラー
    pub fn decode_headers(&mut self) -> Result<Option<(ResponseHead, BodyKind)>, Error> {
        loop {
            match &self.phase {
                DecodePhase::StartLine => {
                    if let Some(pos) = find_line(&self.buf) {
                        let line = String::from_utf8(self.buf[..pos].to_vec())
                            .map_err(|e| Error::InvalidData(format!("invalid UTF-8: {e}")))?;
                        self.buf.drain(..pos + 2);

                        // Parse: VERSION SP STATUS-CODE SP REASON-PHRASE CRLF
                        let parts: Vec<&str> = line.splitn(3, ' ').collect();
                        if parts.len() < 2 {
                            return Err(Error::InvalidData(format!(
                                "invalid status line: {}",
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

                            // ステータスコードを取得
                            let start_line = self.start_line.as_ref().ok_or_else(|| {
                                Error::InvalidData("missing status line".to_string())
                            })?;
                            let parts: Vec<&str> = start_line.splitn(3, ' ').collect();
                            let status_code: u16 = parts[1].parse().map_err(|_| {
                                Error::InvalidData(format!("invalid status code: {}", parts[1]))
                            })?;

                            self.status_code = status_code;
                            let body_kind = self.determine_body_kind(status_code)?;

                            // ヘッダー完了、ボディフェーズに遷移
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
                                BodyKind::CloseDelimited => {
                                    self.phase = DecodePhase::BodyCloseDelimited;
                                }
                                BodyKind::None => {
                                    self.phase = DecodePhase::Complete;
                                }
                            }

                            // ResponseHead を構築
                            let start_line = self.start_line.take().unwrap();
                            let parts: Vec<&str> = start_line.splitn(3, ' ').collect();

                            let head = ResponseHead {
                                version: parts[0].to_string(),
                                status_code,
                                reason_phrase: parts.get(2).unwrap_or(&"").to_string(),
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
                    self.expect_no_body = false;
                    self.status_code = 0;
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
            DecodePhase::BodyCloseDelimited => self.buf.len(),
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

    /// レスポンス全体を一括でデコード
    ///
    /// ストリーミング API (`decode_headers()` / `peek_body()` / `consume_body()`) を
    /// 内部で使用して、レスポンス全体をデコードする。
    ///
    /// データ不足の場合は `None` を返す。
    /// ストリーミング API と混在使用するとエラーを返す。
    ///
    /// ## close-delimited ボディの場合
    ///
    /// `BodyKind::CloseDelimited` の場合、接続が閉じられるまでがボディとなる。
    /// `decode()` を使う場合は、接続終了後に `mark_eof()` を呼んでから
    /// 再度 `decode()` を呼ぶ必要がある。
    pub fn decode(&mut self) -> Result<Option<Response>, Error> {
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
            BodyKind::CloseDelimited => {
                // close-delimited: バッファにあるデータを読み込み、mark_eof() を待つ
                let available = self.available_body_len();
                if available > 0 {
                    self.decoded_body.extend_from_slice(&self.buf[..available]);
                    self.consume_body(available)?;
                }

                // mark_eof() が呼ばれて Complete になったか確認
                if !matches!(self.phase, DecodePhase::Complete) {
                    // まだ EOF でないのでデータ不足
                    return Ok(None);
                }
            }
            BodyKind::None => {}
        }

        // Response を構築
        let head = self.decoded_head.take().unwrap();
        let body = std::mem::take(&mut self.decoded_body);

        // Keep-Alive 対応: 次のレスポンスのために状態をリセット
        self.phase = DecodePhase::StartLine;
        self.decoded_body_kind = None;
        self.body_decoder.reset();
        self.expect_no_body = false;
        self.status_code = 0;

        Ok(Some(Response {
            version: head.version,
            status_code: head.status_code,
            reason_phrase: head.reason_phrase,
            headers: head.headers,
            body,
            omit_content_length: false,
        }))
    }
}
