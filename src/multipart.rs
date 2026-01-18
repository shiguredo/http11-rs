//! multipart/form-data パース (RFC 7578)
//!
//! ## 概要
//!
//! RFC 7578 に基づいた multipart/form-data のパース/生成を提供します。
//!
//! ## 使い方
//!
//! ```rust
//! use shiguredo_http11::multipart::{MultipartParser, Part};
//!
//! // multipart ボディをパース
//! let boundary = "----WebKitFormBoundary";
//! let body = b"------WebKitFormBoundary\r\n\
//!     Content-Disposition: form-data; name=\"field1\"\r\n\r\n\
//!     value1\r\n\
//!     ------WebKitFormBoundary--\r\n";
//!
//! let mut parser = MultipartParser::new(boundary);
//! parser.feed(body);
//!
//! while let Some(part) = parser.next_part().unwrap() {
//!     println!("name: {:?}", part.name());
//!     println!("body: {:?}", std::str::from_utf8(part.body()));
//! }
//! ```

use crate::content_disposition::ContentDisposition;
use crate::content_type::ContentType;
use core::fmt;

/// multipart パースエラー
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MultipartError {
    /// 空の入力
    Empty,
    /// 不正な境界
    InvalidBoundary,
    /// 不正なヘッダー
    InvalidHeader,
    /// 不正なパート
    InvalidPart,
    /// パースが不完全
    Incomplete,
}

impl fmt::Display for MultipartError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            MultipartError::Empty => write!(f, "empty multipart body"),
            MultipartError::InvalidBoundary => write!(f, "invalid boundary"),
            MultipartError::InvalidHeader => write!(f, "invalid part header"),
            MultipartError::InvalidPart => write!(f, "invalid part"),
            MultipartError::Incomplete => write!(f, "incomplete multipart data"),
        }
    }
}

impl std::error::Error for MultipartError {}

/// multipart パートを表す構造体
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Part {
    /// Content-Disposition
    content_disposition: Option<ContentDisposition>,
    /// Content-Type
    content_type: Option<ContentType>,
    /// その他のヘッダー
    headers: Vec<(String, String)>,
    /// ボディ
    body: Vec<u8>,
}

impl Part {
    /// 新しいパートを作成
    pub fn new(name: &str) -> Self {
        Part {
            content_disposition: Some(
                ContentDisposition::new(crate::content_disposition::DispositionType::FormData)
                    .with_name(name),
            ),
            content_type: None,
            headers: Vec::new(),
            body: Vec::new(),
        }
    }

    /// ファイルパートを作成
    pub fn file(name: &str, filename: &str, content_type: &str) -> Self {
        let ct = ContentType::parse(content_type).ok();
        Part {
            content_disposition: Some(
                ContentDisposition::new(crate::content_disposition::DispositionType::FormData)
                    .with_name(name)
                    .with_filename(filename),
            ),
            content_type: ct,
            headers: Vec::new(),
            body: Vec::new(),
        }
    }

    /// ボディを設定
    pub fn with_body(mut self, body: &[u8]) -> Self {
        self.body = body.to_vec();
        self
    }

    /// Content-Type を設定
    pub fn with_content_type(mut self, content_type: ContentType) -> Self {
        self.content_type = Some(content_type);
        self
    }

    /// パートの名前を取得
    pub fn name(&self) -> Option<&str> {
        self.content_disposition.as_ref()?.name()
    }

    /// ファイル名を取得
    pub fn filename(&self) -> Option<&str> {
        self.content_disposition.as_ref()?.filename()
    }

    /// Content-Disposition を取得
    pub fn content_disposition(&self) -> Option<&ContentDisposition> {
        self.content_disposition.as_ref()
    }

    /// Content-Type を取得
    pub fn content_type(&self) -> Option<&ContentType> {
        self.content_type.as_ref()
    }

    /// ヘッダーを取得
    pub fn headers(&self) -> &[(String, String)] {
        &self.headers
    }

    /// ボディを取得
    pub fn body(&self) -> &[u8] {
        &self.body
    }

    /// ボディを文字列として取得
    pub fn body_str(&self) -> Option<&str> {
        std::str::from_utf8(&self.body).ok()
    }

    /// ファイルパートかどうか
    pub fn is_file(&self) -> bool {
        self.filename().is_some()
    }
}

/// multipart パーサー
#[derive(Debug, Clone)]
pub struct MultipartParser {
    /// 境界文字列
    boundary: String,
    /// バッファ
    buffer: Vec<u8>,
    /// パース状態
    state: ParserState,
    /// 完了フラグ
    finished: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum ParserState {
    /// 初期状態 (最初の境界を待機)
    Initial,
    /// パート本体をパース中
    InPart,
    /// 終了境界を検出
    Finished,
}

impl MultipartParser {
    /// 新しいパーサーを作成
    pub fn new(boundary: &str) -> Self {
        MultipartParser {
            boundary: boundary.to_string(),
            buffer: Vec::new(),
            state: ParserState::Initial,
            finished: false,
        }
    }

    /// データを追加
    pub fn feed(&mut self, data: &[u8]) {
        self.buffer.extend_from_slice(data);
    }

    /// パースが完了したかどうか
    pub fn is_finished(&self) -> bool {
        self.finished
    }

    /// 次のパートを取得
    pub fn next_part(&mut self) -> Result<Option<Part>, MultipartError> {
        if self.finished {
            return Ok(None);
        }

        let delimiter = format!("--{}", self.boundary);

        loop {
            match self.state {
                ParserState::Initial => {
                    // 最初の境界を探す
                    if let Some(pos) = find_bytes(&self.buffer, delimiter.as_bytes()) {
                        let after_delim = pos + delimiter.len();
                        // CRLF をスキップ
                        if self.buffer.len() > after_delim + 2 {
                            if &self.buffer[after_delim..after_delim + 2] == b"\r\n" {
                                self.buffer = self.buffer[after_delim + 2..].to_vec();
                                self.state = ParserState::InPart;
                            } else if &self.buffer[after_delim..after_delim + 2] == b"--" {
                                // 終了境界
                                self.state = ParserState::Finished;
                                self.finished = true;
                                return Ok(None);
                            } else {
                                // CRLF 以外の場合もパートに進む
                                self.buffer = self.buffer[after_delim..].to_vec();
                                // 先頭の CRLF があればスキップ
                                if self.buffer.starts_with(b"\r\n") {
                                    self.buffer = self.buffer[2..].to_vec();
                                }
                                self.state = ParserState::InPart;
                            }
                        } else {
                            return Err(MultipartError::Incomplete);
                        }
                    } else {
                        return Err(MultipartError::Incomplete);
                    }
                }
                ParserState::InPart => {
                    // ヘッダーとボディの区切りを探す
                    if let Some(header_end) = find_bytes(&self.buffer, b"\r\n\r\n") {
                        let header_bytes = &self.buffer[..header_end];
                        let body_start = header_end + 4;

                        // ヘッダーをパース
                        let headers_str = std::str::from_utf8(header_bytes)
                            .map_err(|_| MultipartError::InvalidHeader)?;

                        let mut content_disposition = None;
                        let mut content_type = None;
                        let mut headers = Vec::new();

                        for line in headers_str.split("\r\n") {
                            if line.is_empty() {
                                continue;
                            }
                            if let Some((name, value)) = line.split_once(':') {
                                let name = name.trim();
                                let value = value.trim();

                                if name.eq_ignore_ascii_case("Content-Disposition") {
                                    content_disposition = ContentDisposition::parse(value).ok();
                                } else if name.eq_ignore_ascii_case("Content-Type") {
                                    content_type = ContentType::parse(value).ok();
                                } else {
                                    headers.push((name.to_string(), value.to_string()));
                                }
                            }
                        }

                        // 次の境界を探す
                        let body_buffer = &self.buffer[body_start..];
                        let next_delim = format!("\r\n--{}", self.boundary);

                        if let Some(body_end) = find_bytes(body_buffer, next_delim.as_bytes()) {
                            let body = body_buffer[..body_end].to_vec();

                            // 終了境界かどうか確認
                            let after_next = body_start + body_end + next_delim.len();
                            if self.buffer.len() >= after_next + 2 {
                                if &self.buffer[after_next..after_next + 2] == b"--" {
                                    self.finished = true;
                                    self.state = ParserState::Finished;
                                } else if &self.buffer[after_next..after_next + 2] == b"\r\n" {
                                    self.buffer = self.buffer[after_next + 2..].to_vec();
                                } else {
                                    self.buffer = self.buffer[after_next..].to_vec();
                                }
                            } else {
                                self.buffer = self.buffer[after_next..].to_vec();
                            }

                            return Ok(Some(Part {
                                content_disposition,
                                content_type,
                                headers,
                                body,
                            }));
                        } else {
                            return Err(MultipartError::Incomplete);
                        }
                    } else {
                        return Err(MultipartError::Incomplete);
                    }
                }
                ParserState::Finished => {
                    return Ok(None);
                }
            }
        }
    }
}

/// multipart ボディビルダー
#[derive(Debug, Clone)]
pub struct MultipartBuilder {
    /// 境界文字列
    boundary: String,
    /// パート
    parts: Vec<Part>,
}

impl MultipartBuilder {
    /// 乱数値を受け取って境界を生成する
    ///
    /// Sans I/O の原則に従い、乱数生成は呼び出し側の責任となる。
    ///
    /// # 例
    ///
    /// ```
    /// use shiguredo_http11::multipart::MultipartBuilder;
    ///
    /// // 乱数値を渡して境界を生成
    /// let builder = MultipartBuilder::new(12345678901234567890);
    /// assert!(builder.boundary().starts_with("----FormBoundary"));
    /// ```
    pub fn new(random_value: u64) -> Self {
        let boundary = format!("----FormBoundary{}", random_value);
        MultipartBuilder {
            boundary,
            parts: Vec::new(),
        }
    }

    /// 境界を指定して作成
    pub fn with_boundary(boundary: &str) -> Self {
        MultipartBuilder {
            boundary: boundary.to_string(),
            parts: Vec::new(),
        }
    }

    /// 境界文字列を取得
    pub fn boundary(&self) -> &str {
        &self.boundary
    }

    /// Content-Type ヘッダー値を取得
    pub fn content_type(&self) -> String {
        format!("multipart/form-data; boundary={}", self.boundary)
    }

    /// テキストフィールドを追加
    pub fn text_field(mut self, name: &str, value: &str) -> Self {
        let part = Part::new(name).with_body(value.as_bytes());
        self.parts.push(part);
        self
    }

    /// ファイルフィールドを追加
    pub fn file_field(
        mut self,
        name: &str,
        filename: &str,
        content_type: &str,
        data: &[u8],
    ) -> Self {
        let part = Part::file(name, filename, content_type).with_body(data);
        self.parts.push(part);
        self
    }

    /// パートを追加
    pub fn part(mut self, part: Part) -> Self {
        self.parts.push(part);
        self
    }

    /// ボディをビルド
    pub fn build(&self) -> Vec<u8> {
        let mut result = Vec::new();

        for part in &self.parts {
            // 境界
            result.extend_from_slice(b"--");
            result.extend_from_slice(self.boundary.as_bytes());
            result.extend_from_slice(b"\r\n");

            // Content-Disposition
            if let Some(cd) = &part.content_disposition {
                result.extend_from_slice(b"Content-Disposition: ");
                result.extend_from_slice(cd.to_string().as_bytes());
                result.extend_from_slice(b"\r\n");
            }

            // Content-Type
            if let Some(ct) = &part.content_type {
                result.extend_from_slice(b"Content-Type: ");
                result.extend_from_slice(ct.to_string().as_bytes());
                result.extend_from_slice(b"\r\n");
            }

            // その他のヘッダー
            for (name, value) in &part.headers {
                result.extend_from_slice(name.as_bytes());
                result.extend_from_slice(b": ");
                result.extend_from_slice(value.as_bytes());
                result.extend_from_slice(b"\r\n");
            }

            // ヘッダーとボディの区切り
            result.extend_from_slice(b"\r\n");

            // ボディ
            result.extend_from_slice(&part.body);
            result.extend_from_slice(b"\r\n");
        }

        // 終了境界
        result.extend_from_slice(b"--");
        result.extend_from_slice(self.boundary.as_bytes());
        result.extend_from_slice(b"--\r\n");

        result
    }
}

/// バイト列から部分列を検索
fn find_bytes(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() {
        return Some(0);
    }
    if needle.len() > haystack.len() {
        return None;
    }

    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_simple() {
        let boundary = "----WebKitFormBoundary";
        let body = b"------WebKitFormBoundary\r\n\
            Content-Disposition: form-data; name=\"field1\"\r\n\r\n\
            value1\r\n\
            ------WebKitFormBoundary--\r\n";

        let mut parser = MultipartParser::new(boundary);
        parser.feed(body);

        let part = parser.next_part().unwrap().unwrap();
        assert_eq!(part.name(), Some("field1"));
        assert_eq!(part.body_str(), Some("value1"));

        assert!(parser.next_part().unwrap().is_none());
    }

    #[test]
    fn test_parse_multiple_parts() {
        let boundary = "boundary";
        let body = b"--boundary\r\n\
            Content-Disposition: form-data; name=\"field1\"\r\n\r\n\
            value1\r\n\
            --boundary\r\n\
            Content-Disposition: form-data; name=\"field2\"\r\n\r\n\
            value2\r\n\
            --boundary--\r\n";

        let mut parser = MultipartParser::new(boundary);
        parser.feed(body);

        let part1 = parser.next_part().unwrap().unwrap();
        assert_eq!(part1.name(), Some("field1"));
        assert_eq!(part1.body_str(), Some("value1"));

        let part2 = parser.next_part().unwrap().unwrap();
        assert_eq!(part2.name(), Some("field2"));
        assert_eq!(part2.body_str(), Some("value2"));

        assert!(parser.next_part().unwrap().is_none());
    }

    #[test]
    fn test_parse_with_file() {
        let boundary = "boundary";
        let body = b"--boundary\r\n\
            Content-Disposition: form-data; name=\"file\"; filename=\"test.txt\"\r\n\
            Content-Type: text/plain\r\n\r\n\
            file content\r\n\
            --boundary--\r\n";

        let mut parser = MultipartParser::new(boundary);
        parser.feed(body);

        let part = parser.next_part().unwrap().unwrap();
        assert_eq!(part.name(), Some("file"));
        assert_eq!(part.filename(), Some("test.txt"));
        assert!(part.is_file());
        assert_eq!(part.body_str(), Some("file content"));
        assert!(part.content_type().is_some());
    }

    #[test]
    fn test_builder_simple() {
        let body = MultipartBuilder::with_boundary("boundary")
            .text_field("field1", "value1")
            .build();

        let expected = b"--boundary\r\n\
            Content-Disposition: form-data; name=\"field1\"\r\n\r\n\
            value1\r\n\
            --boundary--\r\n";

        assert_eq!(body, expected);
    }

    #[test]
    fn test_builder_with_file() {
        let body = MultipartBuilder::with_boundary("boundary")
            .file_field("file", "test.txt", "text/plain", b"content")
            .build();

        let body_str = String::from_utf8_lossy(&body);
        assert!(
            body_str
                .contains("Content-Disposition: form-data; name=\"file\"; filename=\"test.txt\"")
        );
        assert!(body_str.contains("Content-Type: text/plain"));
        assert!(body_str.contains("content"));
    }

    #[test]
    fn test_roundtrip() {
        let original_body = MultipartBuilder::with_boundary("test-boundary")
            .text_field("name", "John")
            .text_field("age", "30")
            .file_field("photo", "photo.jpg", "image/jpeg", b"\xFF\xD8\xFF\xE0")
            .build();

        let mut parser = MultipartParser::new("test-boundary");
        parser.feed(&original_body);

        let part1 = parser.next_part().unwrap().unwrap();
        assert_eq!(part1.name(), Some("name"));
        assert_eq!(part1.body_str(), Some("John"));

        let part2 = parser.next_part().unwrap().unwrap();
        assert_eq!(part2.name(), Some("age"));
        assert_eq!(part2.body_str(), Some("30"));

        let part3 = parser.next_part().unwrap().unwrap();
        assert_eq!(part3.name(), Some("photo"));
        assert_eq!(part3.filename(), Some("photo.jpg"));
        assert_eq!(part3.body(), b"\xFF\xD8\xFF\xE0");

        assert!(parser.next_part().unwrap().is_none());
    }

    #[test]
    fn test_content_type() {
        let builder = MultipartBuilder::with_boundary("abc123");
        assert_eq!(
            builder.content_type(),
            "multipart/form-data; boundary=abc123"
        );
    }

    #[test]
    fn test_part_new() {
        let part = Part::new("field").with_body(b"value");
        assert_eq!(part.name(), Some("field"));
        assert_eq!(part.body(), b"value");
        assert!(!part.is_file());
    }

    #[test]
    fn test_part_file() {
        let part = Part::file("upload", "file.txt", "text/plain").with_body(b"data");
        assert_eq!(part.name(), Some("upload"));
        assert_eq!(part.filename(), Some("file.txt"));
        assert!(part.is_file());
        assert!(part.content_type().is_some());
    }

    #[test]
    fn test_find_bytes() {
        assert_eq!(find_bytes(b"hello world", b"world"), Some(6));
        assert_eq!(find_bytes(b"hello", b"x"), None);
        assert_eq!(find_bytes(b"hello", b""), Some(0));
        assert_eq!(find_bytes(b"", b"x"), None);
    }

    #[test]
    fn test_binary_content() {
        let binary_data = vec![0x00, 0xFF, 0x10, 0x20];
        let body = MultipartBuilder::with_boundary("boundary")
            .file_field(
                "data",
                "binary.bin",
                "application/octet-stream",
                &binary_data,
            )
            .build();

        let mut parser = MultipartParser::new("boundary");
        parser.feed(&body);

        let part = parser.next_part().unwrap().unwrap();
        assert_eq!(part.body(), &binary_data);
    }
}
