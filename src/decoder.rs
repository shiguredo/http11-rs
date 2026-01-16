use crate::error::Error;
use crate::limits::DecoderLimits;
use crate::request::Request;
use crate::response::Response;

/// ボディのデコード方法
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BodyMode {
    /// Content-Length で指定された固定長
    ContentLength(usize),
    /// Transfer-Encoding: chunked
    Chunked,
    /// ボディなし (HEAD, 1xx, 204, 304 など)
    None,
}

/// デコード状態
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DecodeState {
    StartLine,
    Headers,
    Body {
        content_length: usize,
    },
    /// Chunked body - チャンクサイズ待ち
    ChunkedSize,
    /// Chunked body - チャンクデータ待ち
    ChunkedData {
        remaining: usize,
    },
    /// Chunked body - トレーラーヘッダー待ち
    ChunkedTrailer,
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
    state: DecodeState,
    start_line: Option<String>,
    headers: Vec<(String, String)>,
    body_buf: Vec<u8>,
    limits: DecoderLimits,
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
            state: DecodeState::StartLine,
            start_line: None,
            headers: Vec::new(),
            body_buf: Vec::new(),
            limits: DecoderLimits::default(),
        }
    }

    /// 制限付きでデコーダーを作成
    pub fn with_limits(limits: DecoderLimits) -> Self {
        Self {
            buf: Vec::new(),
            state: DecodeState::StartLine,
            start_line: None,
            headers: Vec::new(),
            body_buf: Vec::new(),
            limits,
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
        self.state = DecodeState::StartLine;
        self.start_line = None;
        self.headers.clear();
        self.body_buf.clear();
    }

    /// CRLF で終わる行を探す
    fn find_line(&self) -> Option<usize> {
        self.buf.windows(2).position(|w| w == b"\r\n")
    }

    /// ボディモードを決定
    fn determine_body_mode(&self) -> Result<BodyMode, Error> {
        let (transfer_encoding_chunked, content_length) = resolve_body_headers(&self.headers)?;

        if transfer_encoding_chunked {
            return Ok(BodyMode::Chunked);
        }

        if let Some(len) = content_length {
            if len > self.limits.max_body_size {
                return Err(Error::BodyTooLarge {
                    size: len,
                    limit: self.limits.max_body_size,
                });
            }
            return Ok(BodyMode::ContentLength(len));
        }

        Ok(BodyMode::None)
    }

    /// リクエストをデコード
    pub fn decode(&mut self) -> Result<Option<Request>, Error> {
        loop {
            match self.state {
                DecodeState::StartLine => {
                    if let Some(pos) = self.find_line() {
                        let line = String::from_utf8(self.buf[..pos].to_vec())
                            .map_err(|e| Error::InvalidData(format!("invalid UTF-8: {e}")))?;
                        self.buf.drain(..pos + 2);
                        if line.contains('\r') || line.contains('\n') {
                            return Err(Error::InvalidData(
                                "invalid status line: contains CR/LF".to_string(),
                            ));
                        }
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
                        self.state = DecodeState::Headers;
                    } else {
                        return Ok(None);
                    }
                }
                DecodeState::Headers => {
                    if let Some(pos) = self.find_line() {
                        if pos == 0 {
                            // Empty line - end of headers
                            self.buf.drain(..2);

                            match self.determine_body_mode()? {
                                BodyMode::ContentLength(len) => {
                                    if len > 0 {
                                        self.state = DecodeState::Body {
                                            content_length: len,
                                        };
                                    } else {
                                        return self.finish_request();
                                    }
                                }
                                BodyMode::Chunked => {
                                    self.state = DecodeState::ChunkedSize;
                                }
                                BodyMode::None => {
                                    return self.finish_request();
                                }
                            }
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
                DecodeState::Body { content_length } => {
                    if self.buf.len() >= content_length {
                        let body: Vec<u8> = self.buf.drain(..content_length).collect();
                        self.body_buf = body;
                        return self.finish_request();
                    } else {
                        return Ok(None);
                    }
                }
                DecodeState::ChunkedSize => {
                    if let Some(pos) = self.find_line() {
                        let line = String::from_utf8(self.buf[..pos].to_vec())
                            .map_err(|e| Error::InvalidData(format!("invalid UTF-8: {e}")))?;
                        self.buf.drain(..pos + 2);

                        // チャンクサイズをパース (拡張は無視)
                        let size_str = line.split(';').next().unwrap_or(&line).trim();
                        let chunk_size = usize::from_str_radix(size_str, 16).map_err(|_| {
                            Error::InvalidData(format!("invalid chunk size: {}", size_str))
                        })?;

                        if chunk_size == 0 {
                            // 最終チャンク
                            self.state = DecodeState::ChunkedTrailer;
                        } else {
                            // ボディサイズ制限チェック
                            let new_size = self.body_buf.len() + chunk_size;
                            if new_size > self.limits.max_body_size {
                                return Err(Error::BodyTooLarge {
                                    size: new_size,
                                    limit: self.limits.max_body_size,
                                });
                            }
                            self.state = DecodeState::ChunkedData {
                                remaining: chunk_size,
                            };
                        }
                    } else {
                        return Ok(None);
                    }
                }
                DecodeState::ChunkedData { remaining } => {
                    // チャンクデータ + CRLF が必要
                    if self.buf.len() >= remaining + 2 {
                        self.body_buf.extend_from_slice(&self.buf[..remaining]);
                        self.buf.drain(..remaining + 2); // データ + CRLF
                        self.state = DecodeState::ChunkedSize;
                    } else {
                        return Ok(None);
                    }
                }
                DecodeState::ChunkedTrailer => {
                    // トレーラーヘッダーを処理 (空行まで読む)
                    if let Some(pos) = self.find_line() {
                        if pos == 0 {
                            // 空行 - トレーラー終了
                            self.buf.drain(..2);
                            return self.finish_request();
                        } else {
                            // トレーラーヘッダー (無視または追加)
                            self.buf.drain(..pos + 2);
                        }
                    } else {
                        return Ok(None);
                    }
                }
            }
        }
    }

    fn finish_request(&mut self) -> Result<Option<Request>, Error> {
        let start_line = match self.start_line.take() {
            Some(line) => line,
            None => {
                self.reset_on_error();
                return Err(Error::InvalidData("missing request line".to_string()));
            }
        };
        let parts: Vec<&str> = start_line.splitn(3, ' ').collect();

        let request = Request {
            method: parts[0].to_string(),
            uri: parts[1].to_string(),
            version: parts[2].to_string(),
            headers: std::mem::take(&mut self.headers),
            body: std::mem::take(&mut self.body_buf),
        };

        self.state = DecodeState::StartLine;
        Ok(Some(request))
    }

    /// エラー発生時に状態をリセット
    fn reset_on_error(&mut self) {
        self.state = DecodeState::StartLine;
        self.start_line = None;
        self.headers.clear();
        self.body_buf.clear();
    }
}

/// HTTP レスポンスデコーダー (Sans I/O)
///
/// クライアント側でサーバーからのレスポンスをパースする際に使用
#[derive(Debug)]
pub struct ResponseDecoder {
    buf: Vec<u8>,
    state: DecodeState,
    start_line: Option<String>,
    headers: Vec<(String, String)>,
    body_buf: Vec<u8>,
    limits: DecoderLimits,
    /// HEAD リクエストへのレスポンスかどうか
    expect_no_body: bool,
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
            state: DecodeState::StartLine,
            start_line: None,
            headers: Vec::new(),
            body_buf: Vec::new(),
            limits: DecoderLimits::default(),
            expect_no_body: false,
        }
    }

    /// 制限付きでデコーダーを作成
    pub fn with_limits(limits: DecoderLimits) -> Self {
        Self {
            buf: Vec::new(),
            state: DecodeState::StartLine,
            start_line: None,
            headers: Vec::new(),
            body_buf: Vec::new(),
            limits,
            expect_no_body: false,
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
        self.state = DecodeState::StartLine;
        self.start_line = None;
        self.headers.clear();
        self.body_buf.clear();
        self.expect_no_body = false;
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
    fn determine_body_mode(&self, status_code: u16) -> Result<BodyMode, Error> {
        let (transfer_encoding_chunked, content_length) = resolve_body_headers(&self.headers)?;

        // HEAD リクエストへのレスポンス、または 1xx/204/304 はボディなし
        if self.expect_no_body || !Self::status_has_body(status_code) {
            return Ok(BodyMode::None);
        }

        if transfer_encoding_chunked {
            return Ok(BodyMode::Chunked);
        }

        if let Some(len) = content_length {
            if len > self.limits.max_body_size {
                return Err(Error::BodyTooLarge {
                    size: len,
                    limit: self.limits.max_body_size,
                });
            }
            return Ok(BodyMode::ContentLength(len));
        }

        Ok(BodyMode::None)
    }

    /// レスポンスをデコード
    pub fn decode(&mut self) -> Result<Option<Response>, Error> {
        loop {
            match self.state {
                DecodeState::StartLine => {
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
                        self.state = DecodeState::Headers;
                    } else {
                        return Ok(None);
                    }
                }
                DecodeState::Headers => {
                    if let Some(pos) = self.find_line() {
                        if pos == 0 {
                            // Empty line - end of headers
                            self.buf.drain(..2);

                            // ステータスコードを取得
                            let start_line = match self.start_line.as_ref() {
                                Some(line) => line,
                                None => {
                                    self.reset_on_error();
                                    return Err(Error::InvalidData(
                                        "missing status line".to_string(),
                                    ));
                                }
                            };
                            let parts: Vec<&str> = start_line.splitn(3, ' ').collect();
                            let status_code: u16 = parts[1].parse().unwrap_or(0);

                            match self.determine_body_mode(status_code)? {
                                BodyMode::ContentLength(len) => {
                                    if len > 0 {
                                        self.state = DecodeState::Body {
                                            content_length: len,
                                        };
                                    } else {
                                        return self.finish_response();
                                    }
                                }
                                BodyMode::Chunked => {
                                    self.state = DecodeState::ChunkedSize;
                                }
                                BodyMode::None => {
                                    return self.finish_response();
                                }
                            }
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
                DecodeState::Body { content_length } => {
                    if self.buf.len() >= content_length {
                        let body: Vec<u8> = self.buf.drain(..content_length).collect();
                        self.body_buf = body;
                        return self.finish_response();
                    } else {
                        return Ok(None);
                    }
                }
                DecodeState::ChunkedSize => {
                    if let Some(pos) = self.find_line() {
                        let line = String::from_utf8(self.buf[..pos].to_vec())
                            .map_err(|e| Error::InvalidData(format!("invalid UTF-8: {e}")))?;
                        self.buf.drain(..pos + 2);

                        // チャンクサイズをパース (拡張は無視)
                        let size_str = line.split(';').next().unwrap_or(&line).trim();
                        let chunk_size = usize::from_str_radix(size_str, 16).map_err(|_| {
                            Error::InvalidData(format!("invalid chunk size: {}", size_str))
                        })?;

                        if chunk_size == 0 {
                            // 最終チャンク
                            self.state = DecodeState::ChunkedTrailer;
                        } else {
                            // ボディサイズ制限チェック
                            let new_size = self.body_buf.len() + chunk_size;
                            if new_size > self.limits.max_body_size {
                                return Err(Error::BodyTooLarge {
                                    size: new_size,
                                    limit: self.limits.max_body_size,
                                });
                            }
                            self.state = DecodeState::ChunkedData {
                                remaining: chunk_size,
                            };
                        }
                    } else {
                        return Ok(None);
                    }
                }
                DecodeState::ChunkedData { remaining } => {
                    // チャンクデータ + CRLF が必要
                    if self.buf.len() >= remaining + 2 {
                        self.body_buf.extend_from_slice(&self.buf[..remaining]);
                        self.buf.drain(..remaining + 2); // データ + CRLF
                        self.state = DecodeState::ChunkedSize;
                    } else {
                        return Ok(None);
                    }
                }
                DecodeState::ChunkedTrailer => {
                    // トレーラーヘッダーを処理 (空行まで読む)
                    if let Some(pos) = self.find_line() {
                        if pos == 0 {
                            // 空行 - トレーラー終了
                            self.buf.drain(..2);
                            return self.finish_response();
                        } else {
                            // トレーラーヘッダー (無視または追加)
                            self.buf.drain(..pos + 2);
                        }
                    } else {
                        return Ok(None);
                    }
                }
            }
        }
    }

    fn finish_response(&mut self) -> Result<Option<Response>, Error> {
        let start_line = match self.start_line.take() {
            Some(line) => line,
            None => {
                self.reset_on_error();
                return Err(Error::InvalidData("missing status line".to_string()));
            }
        };
        let parts: Vec<&str> = start_line.splitn(3, ' ').collect();

        let status_code: u16 = match parts[1].parse() {
            Ok(code) => code,
            Err(_) => {
                let msg = format!("invalid status code: {}", parts[1]);
                self.reset_on_error();
                return Err(Error::InvalidData(msg));
            }
        };

        let response = Response {
            version: parts[0].to_string(),
            status_code,
            reason_phrase: parts.get(2).unwrap_or(&"").to_string(),
            headers: std::mem::take(&mut self.headers),
            body: std::mem::take(&mut self.body_buf),
        };

        self.state = DecodeState::StartLine;
        Ok(Some(response))
    }

    /// エラー発生時に状態をリセット
    fn reset_on_error(&mut self) {
        self.state = DecodeState::StartLine;
        self.start_line = None;
        self.headers.clear();
        self.body_buf.clear();
    }
}
