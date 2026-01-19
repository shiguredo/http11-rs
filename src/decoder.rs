use crate::error::Error;
use crate::limits::DecoderLimits;
use crate::request::Request;
use crate::response::Response;

/// リクエストヘッダー（ボディなし）
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RequestHead {
    /// HTTP メソッド (GET, POST, etc.)
    pub method: String,
    /// リクエスト URI
    pub uri: String,
    /// HTTP バージョン (HTTP/1.1 等)
    pub version: String,
    /// ヘッダー
    pub headers: Vec<(String, String)>,
}

impl RequestHead {
    /// ヘッダーを取得 (大文字小文字を区別しない)
    pub fn get_header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(n, _)| n.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.as_str())
    }

    /// 指定した名前のヘッダーをすべて取得
    pub fn get_headers(&self, name: &str) -> Vec<&str> {
        self.headers
            .iter()
            .filter(|(n, _)| n.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.as_str())
            .collect()
    }

    /// ヘッダーが存在するか確認
    pub fn has_header(&self, name: &str) -> bool {
        self.headers
            .iter()
            .any(|(n, _)| n.eq_ignore_ascii_case(name))
    }

    /// Connection ヘッダーの値を取得
    pub fn connection(&self) -> Option<&str> {
        self.get_header("Connection")
    }

    /// キープアライブ接続かどうかを判定
    pub fn is_keep_alive(&self) -> bool {
        if let Some(conn) = self.connection() {
            if conn.eq_ignore_ascii_case("close") {
                return false;
            }
            if conn.eq_ignore_ascii_case("keep-alive") {
                return true;
            }
        }
        self.version.ends_with("/1.1")
    }

    /// Content-Length ヘッダーの値を取得
    pub fn content_length(&self) -> Option<usize> {
        self.get_header("Content-Length")
            .and_then(|v| v.parse().ok())
    }

    /// Transfer-Encoding が chunked かどうかを判定
    pub fn is_chunked(&self) -> bool {
        self.get_header("Transfer-Encoding")
            .is_some_and(|v| v.eq_ignore_ascii_case("chunked"))
    }
}

/// レスポンスヘッダー（ボディなし）
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResponseHead {
    /// HTTP バージョン (HTTP/1.1 等)
    pub version: String,
    /// ステータスコード (200, 404, etc.)
    pub status_code: u16,
    /// ステータスフレーズ (OK, Not Found, etc.)
    pub reason_phrase: String,
    /// ヘッダー
    pub headers: Vec<(String, String)>,
}

impl ResponseHead {
    /// ヘッダーを取得 (大文字小文字を区別しない)
    pub fn get_header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(n, _)| n.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.as_str())
    }

    /// 指定した名前のヘッダーをすべて取得
    pub fn get_headers(&self, name: &str) -> Vec<&str> {
        self.headers
            .iter()
            .filter(|(n, _)| n.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.as_str())
            .collect()
    }

    /// ヘッダーが存在するか確認
    pub fn has_header(&self, name: &str) -> bool {
        self.headers
            .iter()
            .any(|(n, _)| n.eq_ignore_ascii_case(name))
    }

    /// ステータスコードが成功 (2xx) か確認
    pub fn is_success(&self) -> bool {
        (200..300).contains(&self.status_code)
    }

    /// ステータスコードがリダイレクト (3xx) か確認
    pub fn is_redirect(&self) -> bool {
        (300..400).contains(&self.status_code)
    }

    /// ステータスコードがクライアントエラー (4xx) か確認
    pub fn is_client_error(&self) -> bool {
        (400..500).contains(&self.status_code)
    }

    /// ステータスコードがサーバーエラー (5xx) か確認
    pub fn is_server_error(&self) -> bool {
        (500..600).contains(&self.status_code)
    }

    /// ステータスコードが情報レスポンス (1xx) か確認
    pub fn is_informational(&self) -> bool {
        (100..200).contains(&self.status_code)
    }

    /// Connection ヘッダーの値を取得
    pub fn connection(&self) -> Option<&str> {
        self.get_header("Connection")
    }

    /// キープアライブ接続かどうかを判定
    pub fn is_keep_alive(&self) -> bool {
        if let Some(conn) = self.connection() {
            if conn.eq_ignore_ascii_case("close") {
                return false;
            }
            if conn.eq_ignore_ascii_case("keep-alive") {
                return true;
            }
        }
        self.version.ends_with("/1.1")
    }

    /// Content-Length ヘッダーの値を取得
    pub fn content_length(&self) -> Option<usize> {
        self.get_header("Content-Length")
            .and_then(|v| v.parse().ok())
    }

    /// Transfer-Encoding が chunked かどうかを判定
    pub fn is_chunked(&self) -> bool {
        self.get_header("Transfer-Encoding")
            .is_some_and(|v| v.eq_ignore_ascii_case("chunked"))
    }
}

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

/// デコード状態
#[derive(Debug, Clone, PartialEq, Eq)]
enum DecodePhase {
    /// スタートライン待ち
    StartLine,
    /// ヘッダー待ち
    Headers,
    /// ボディ読み取り中 (Content-Length)
    BodyContentLength { remaining: usize },
    /// ボディ読み取り中 (Chunked) - チャンクサイズ待ち
    BodyChunkedSize,
    /// ボディ読み取り中 (Chunked) - チャンクデータ待ち
    BodyChunkedData { remaining: usize },
    /// トレーラーヘッダー待ち
    ChunkedTrailer,
    /// 完了
    Complete,
}

fn parse_header_line(line: &str) -> Result<(String, String), Error> {
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

fn resolve_body_headers(headers: &[(String, String)]) -> Result<(bool, Option<usize>), Error> {
    let transfer_encoding_chunked = parse_transfer_encoding_chunked(headers)?;
    let content_length = parse_content_length(headers)?;

    if transfer_encoding_chunked && content_length.is_some() {
        return Err(Error::InvalidData(
            "invalid message: both Transfer-Encoding and Content-Length".to_string(),
        ));
    }

    Ok((transfer_encoding_chunked, content_length))
}

fn parse_transfer_encoding_chunked(headers: &[(String, String)]) -> Result<bool, Error> {
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

fn parse_content_length(headers: &[(String, String)]) -> Result<Option<usize>, Error> {
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

fn is_valid_header_name(name: &str) -> bool {
    !name.is_empty() && name.bytes().all(is_token_char)
}

fn is_token_char(b: u8) -> bool {
    matches!(
        b,
        b'!' | b'#' | b'$' | b'%' | b'&' | b'\'' | b'*' | b'+' | b'-' | b'.' |
        b'0'..=b'9' | b'A'..=b'Z' | b'^' | b'_' | b'`' | b'a'..=b'z' | b'|' | b'~'
    )
}

/// HTTP リクエストデコーダー (Sans I/O)
///
/// サーバー側でクライアントからのリクエストをパースする際に使用
#[derive(Debug)]
pub struct RequestDecoder {
    buf: Vec<u8>,
    phase: DecodePhase,
    start_line: Option<String>,
    headers: Vec<(String, String)>,
    trailers: Vec<(String, String)>,
    /// ボディ内での消費済みバイト数（Content-Length の場合）
    body_consumed: usize,
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
            trailers: Vec::new(),
            body_consumed: 0,
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
            trailers: Vec::new(),
            body_consumed: 0,
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
        self.trailers.clear();
        self.body_consumed = 0;
        self.decoded_head = None;
        self.decoded_body_kind = None;
        self.decoded_body.clear();
    }

    /// CRLF で終わる行を探す
    fn find_line(&self) -> Option<usize> {
        self.buf.windows(2).position(|w| w == b"\r\n")
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
                    if let Some(pos) = self.find_line() {
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
                    if let Some(pos) = self.find_line() {
                        if pos == 0 {
                            // Empty line - end of headers
                            self.buf.drain(..2);

                            let body_kind = self.determine_body_kind()?;

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
                                BodyKind::None => {
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
                _ => {
                    return Err(Error::InvalidData(
                        "decode_headers called after headers already decoded".to_string(),
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
        match &self.phase {
            DecodePhase::BodyContentLength { remaining } => {
                if self.buf.is_empty() {
                    return None;
                }
                // バッファにあるデータのうち、残り必要な分だけ返す
                let available = self.buf.len().min(*remaining);
                if available > 0 {
                    Some(&self.buf[..available])
                } else {
                    None
                }
            }
            DecodePhase::BodyChunkedSize => {
                // チャンクサイズ行を処理中、データはまだない
                None
            }
            DecodePhase::BodyChunkedData { remaining } => {
                if self.buf.is_empty() {
                    return None;
                }
                // チャンクデータのうち、残り必要な分だけ返す
                let available = self.buf.len().min(*remaining);
                if available > 0 {
                    Some(&self.buf[..available])
                } else {
                    None
                }
            }
            DecodePhase::ChunkedTrailer | DecodePhase::Complete => None,
            _ => None,
        }
    }

    /// ボディデータを消費
    ///
    /// `peek_body()` で取得したデータを処理した後に呼ぶ
    /// `len` は消費するバイト数
    pub fn consume_body(&mut self, len: usize) -> Result<BodyProgress, Error> {
        match &mut self.phase {
            DecodePhase::BodyContentLength { remaining } => {
                if len > *remaining {
                    return Err(Error::InvalidData(
                        "consume_body: len exceeds remaining".to_string(),
                    ));
                }
                if len > self.buf.len() {
                    return Err(Error::InvalidData(
                        "consume_body: len exceeds buffer".to_string(),
                    ));
                }

                self.buf.drain(..len);
                *remaining -= len;
                self.body_consumed += len;

                if *remaining == 0 {
                    self.phase = DecodePhase::Complete;
                    return Ok(BodyProgress::Complete {
                        trailers: Vec::new(),
                    });
                }

                Ok(BodyProgress::Continue)
            }
            DecodePhase::BodyChunkedSize => {
                // チャンクサイズを処理
                self.process_chunked_size()?;

                // 処理後の状態を確認
                match &self.phase {
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
                if len > self.buf.len() {
                    return Err(Error::InvalidData(
                        "consume_body: len exceeds buffer".to_string(),
                    ));
                }

                self.buf.drain(..len);
                *remaining -= len;
                self.body_consumed += len;

                if *remaining == 0 {
                    // チャンクデータ終了、CRLF をスキップ
                    if self.buf.len() >= 2 {
                        self.buf.drain(..2);
                        self.phase = DecodePhase::BodyChunkedSize;
                    }
                    // CRLF がまだ来ていない場合は次の consume で処理
                }

                Ok(BodyProgress::Continue)
            }
            DecodePhase::ChunkedTrailer => {
                // トレーラーを処理
                self.process_trailers()?;

                match &self.phase {
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
    fn process_chunked_size(&mut self) -> Result<(), Error> {
        if !matches!(self.phase, DecodePhase::BodyChunkedSize) {
            return Ok(());
        }

        if let Some(pos) = self.find_line() {
            let line = String::from_utf8(self.buf[..pos].to_vec())
                .map_err(|e| Error::InvalidData(format!("invalid UTF-8: {e}")))?;
            self.buf.drain(..pos + 2);

            // チャンクサイズをパース (拡張は無視)
            let size_str = line.split(';').next().unwrap_or(&line).trim();
            let chunk_size = usize::from_str_radix(size_str, 16)
                .map_err(|_| Error::InvalidData(format!("invalid chunk size: {}", size_str)))?;

            if chunk_size == 0 {
                // 最終チャンク
                self.phase = DecodePhase::ChunkedTrailer;
                return self.process_trailers();
            } else {
                // ボディサイズ制限チェック
                let new_size = self.body_consumed + chunk_size;
                if new_size > self.limits.max_body_size {
                    return Err(Error::BodyTooLarge {
                        size: new_size,
                        limit: self.limits.max_body_size,
                    });
                }
                self.phase = DecodePhase::BodyChunkedData {
                    remaining: chunk_size,
                };
            }
        }
        Ok(())
    }

    /// トレーラーヘッダーを処理
    fn process_trailers(&mut self) -> Result<(), Error> {
        while matches!(self.phase, DecodePhase::ChunkedTrailer) {
            if let Some(pos) = self.find_line() {
                if pos == 0 {
                    // 空行 - トレーラー終了
                    self.buf.drain(..2);
                    self.phase = DecodePhase::Complete;
                    return Ok(());
                } else {
                    // トレーラーヘッダー
                    let line = String::from_utf8(self.buf[..pos].to_vec())
                        .map_err(|e| Error::InvalidData(format!("invalid UTF-8: {e}")))?;
                    self.buf.drain(..pos + 2);

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
        let body_kind = *self.decoded_body_kind.as_ref().unwrap();
        match body_kind {
            BodyKind::ContentLength(_) | BodyKind::Chunked => loop {
                // 借用の問題を避けるため、先にコピーを取得
                let chunk = self.peek_body().map(|data| data.to_vec());
                match chunk {
                    Some(data) => {
                        let len = data.len();
                        self.decoded_body.extend_from_slice(&data);
                        match self.consume_body(len)? {
                            BodyProgress::Complete { .. } => break,
                            BodyProgress::Continue => {}
                        }
                    }
                    None => return Ok(None),
                }
            },
            BodyKind::None => {}
        }

        // Request を構築
        let head = self.decoded_head.take().unwrap();
        let body = std::mem::take(&mut self.decoded_body);

        Ok(Some(Request {
            method: head.method,
            uri: head.uri,
            version: head.version,
            headers: head.headers,
            body,
        }))
    }
}

/// HTTP レスポンスデコーダー (Sans I/O)
///
/// クライアント側でサーバーからのレスポンスをパースする際に使用
#[derive(Debug)]
pub struct ResponseDecoder {
    buf: Vec<u8>,
    phase: DecodePhase,
    start_line: Option<String>,
    headers: Vec<(String, String)>,
    trailers: Vec<(String, String)>,
    /// ボディ内での消費済みバイト数
    body_consumed: usize,
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
            trailers: Vec::new(),
            body_consumed: 0,
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
            trailers: Vec::new(),
            body_consumed: 0,
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
        self.trailers.clear();
        self.body_consumed = 0;
        self.expect_no_body = false;
        self.status_code = 0;
        self.decoded_head = None;
        self.decoded_body_kind = None;
        self.decoded_body.clear();
    }

    /// CRLF で終わる行を探す
    fn find_line(&self) -> Option<usize> {
        self.buf.windows(2).position(|w| w == b"\r\n")
    }

    /// ステータスコードからボディがあるかどうかを判定
    fn status_has_body(status_code: u16) -> bool {
        // 1xx, 204, 304 はボディなし
        !((100..200).contains(&status_code) || status_code == 204 || status_code == 304)
    }

    /// ボディモードを決定
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

        Ok(BodyKind::None)
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
                    if let Some(pos) = self.find_line() {
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
                    if let Some(pos) = self.find_line() {
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
                _ => {
                    return Err(Error::InvalidData(
                        "decode_headers called after headers already decoded".to_string(),
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
        match &self.phase {
            DecodePhase::BodyContentLength { remaining } => {
                if self.buf.is_empty() {
                    return None;
                }
                // バッファにあるデータのうち、残り必要な分だけ返す
                let available = self.buf.len().min(*remaining);
                if available > 0 {
                    Some(&self.buf[..available])
                } else {
                    None
                }
            }
            DecodePhase::BodyChunkedSize => {
                // チャンクサイズ行を処理中、データはまだない
                None
            }
            DecodePhase::BodyChunkedData { remaining } => {
                if self.buf.is_empty() {
                    return None;
                }
                // チャンクデータのうち、残り必要な分だけ返す
                let available = self.buf.len().min(*remaining);
                if available > 0 {
                    Some(&self.buf[..available])
                } else {
                    None
                }
            }
            DecodePhase::ChunkedTrailer | DecodePhase::Complete => None,
            _ => None,
        }
    }

    /// ボディデータを消費
    ///
    /// `peek_body()` で取得したデータを処理した後に呼ぶ
    /// `len` は消費するバイト数
    pub fn consume_body(&mut self, len: usize) -> Result<BodyProgress, Error> {
        match &mut self.phase {
            DecodePhase::BodyContentLength { remaining } => {
                if len > *remaining {
                    return Err(Error::InvalidData(
                        "consume_body: len exceeds remaining".to_string(),
                    ));
                }
                if len > self.buf.len() {
                    return Err(Error::InvalidData(
                        "consume_body: len exceeds buffer".to_string(),
                    ));
                }

                self.buf.drain(..len);
                *remaining -= len;
                self.body_consumed += len;

                if *remaining == 0 {
                    self.phase = DecodePhase::Complete;
                    return Ok(BodyProgress::Complete {
                        trailers: Vec::new(),
                    });
                }

                Ok(BodyProgress::Continue)
            }
            DecodePhase::BodyChunkedSize => {
                // チャンクサイズを処理
                self.process_chunked_size()?;

                // 処理後の状態を確認
                match &self.phase {
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
                if len > self.buf.len() {
                    return Err(Error::InvalidData(
                        "consume_body: len exceeds buffer".to_string(),
                    ));
                }

                self.buf.drain(..len);
                *remaining -= len;
                self.body_consumed += len;

                if *remaining == 0 {
                    // チャンクデータ終了、CRLF をスキップ
                    if self.buf.len() >= 2 {
                        self.buf.drain(..2);
                        self.phase = DecodePhase::BodyChunkedSize;
                    }
                    // CRLF がまだ来ていない場合は次の consume で処理
                }

                Ok(BodyProgress::Continue)
            }
            DecodePhase::ChunkedTrailer => {
                // トレーラーを処理
                self.process_trailers()?;

                match &self.phase {
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
    fn process_chunked_size(&mut self) -> Result<(), Error> {
        if !matches!(self.phase, DecodePhase::BodyChunkedSize) {
            return Ok(());
        }

        if let Some(pos) = self.find_line() {
            let line = String::from_utf8(self.buf[..pos].to_vec())
                .map_err(|e| Error::InvalidData(format!("invalid UTF-8: {e}")))?;
            self.buf.drain(..pos + 2);

            // チャンクサイズをパース (拡張は無視)
            let size_str = line.split(';').next().unwrap_or(&line).trim();
            let chunk_size = usize::from_str_radix(size_str, 16)
                .map_err(|_| Error::InvalidData(format!("invalid chunk size: {}", size_str)))?;

            if chunk_size == 0 {
                // 最終チャンク
                self.phase = DecodePhase::ChunkedTrailer;
                return self.process_trailers();
            } else {
                // ボディサイズ制限チェック
                let new_size = self.body_consumed + chunk_size;
                if new_size > self.limits.max_body_size {
                    return Err(Error::BodyTooLarge {
                        size: new_size,
                        limit: self.limits.max_body_size,
                    });
                }
                self.phase = DecodePhase::BodyChunkedData {
                    remaining: chunk_size,
                };
            }
        }
        Ok(())
    }

    /// トレーラーヘッダーを処理
    fn process_trailers(&mut self) -> Result<(), Error> {
        while matches!(self.phase, DecodePhase::ChunkedTrailer) {
            if let Some(pos) = self.find_line() {
                if pos == 0 {
                    // 空行 - トレーラー終了
                    self.buf.drain(..2);
                    self.phase = DecodePhase::Complete;
                    return Ok(());
                } else {
                    // トレーラーヘッダー
                    let line = String::from_utf8(self.buf[..pos].to_vec())
                        .map_err(|e| Error::InvalidData(format!("invalid UTF-8: {e}")))?;
                    self.buf.drain(..pos + 2);

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

    /// レスポンス全体を一括でデコード
    ///
    /// ストリーミング API (`decode_headers()` / `peek_body()` / `consume_body()`) を
    /// 内部で使用して、レスポンス全体をデコードする。
    ///
    /// データ不足の場合は `None` を返す。
    /// ストリーミング API と混在使用するとエラーを返す。
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
                // 借用の問題を避けるため、先にコピーを取得
                let chunk = self.peek_body().map(|data| data.to_vec());
                match chunk {
                    Some(data) => {
                        let len = data.len();
                        self.decoded_body.extend_from_slice(&data);
                        match self.consume_body(len)? {
                            BodyProgress::Complete { .. } => break,
                            BodyProgress::Continue => {}
                        }
                    }
                    None => return Ok(None),
                }
            },
            BodyKind::None => {}
        }

        // Response を構築
        let head = self.decoded_head.take().unwrap();
        let body = std::mem::take(&mut self.decoded_body);

        Ok(Some(Response {
            version: head.version,
            status_code: head.status_code,
            reason_phrase: head.reason_phrase,
            headers: head.headers,
            body,
        }))
    }
}
