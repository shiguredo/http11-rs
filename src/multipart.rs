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
//! parser.feed(body).unwrap();
//!
//! while let Some(part) = parser.next_part().unwrap() {
//!     println!("name: {:?}", part.name());
//!     println!("body: {:?}", core::str::from_utf8(part.body()));
//! }
//! ```

use crate::content_disposition::ContentDisposition;
use crate::content_type::ContentType;
use crate::validate::is_token_char;
use alloc::string::{String, ToString};
use alloc::vec::Vec;
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
    /// Content-Disposition が欠落している
    /// RFC 7578 Section 4.2: 各パートは Content-Disposition ヘッダーを含まなければならない
    MissingContentDisposition,
    /// Content-Disposition の disposition-type が form-data ではない
    /// RFC 7578 Section 4.2: disposition type は "form-data" でなければならない
    InvalidContentDisposition,
    /// Content-Disposition に name パラメータが欠落している
    /// RFC 7578 Section 4.2: "name" パラメータを含まなければならない
    MissingName,
    /// バッファサイズが上限を超えた
    BufferOverflow {
        /// 超過後のサイズ
        size: usize,
        /// 設定された上限
        limit: usize,
    },
}

impl fmt::Display for MultipartError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            MultipartError::Empty => write!(f, "empty multipart body"),
            MultipartError::InvalidBoundary => write!(f, "invalid boundary"),
            MultipartError::InvalidHeader => write!(f, "invalid part header"),
            MultipartError::InvalidPart => write!(f, "invalid part"),
            MultipartError::Incomplete => write!(f, "incomplete multipart data"),
            MultipartError::MissingContentDisposition => {
                write!(
                    f,
                    "missing Content-Disposition header (RFC 7578 Section 4.2)"
                )
            }
            MultipartError::InvalidContentDisposition => {
                write!(
                    f,
                    "Content-Disposition type must be form-data (RFC 7578 Section 4.2)"
                )
            }
            MultipartError::MissingName => {
                write!(
                    f,
                    "Content-Disposition must contain name parameter (RFC 7578 Section 4.2)"
                )
            }
            MultipartError::BufferOverflow { size, limit } => {
                write!(f, "buffer overflow: size={size}, limit={limit}")
            }
        }
    }
}

impl core::error::Error for MultipartError {}

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
        core::str::from_utf8(&self.body).ok()
    }

    /// ファイルパートかどうか
    pub fn is_file(&self) -> bool {
        self.filename().is_some()
    }
}

/// multipart パーサー
#[derive(Debug, Clone)]
pub struct MultipartParser {
    /// 先頭 boundary delimiter (`--<boundary>`)
    /// 構築時に事前計算しておき、`next_part()` ごとの再生成を避ける
    first_delimiter: Vec<u8>,
    /// パート間 boundary delimiter (`\r\n--<boundary>`)
    /// 構築時に事前計算しておき、`next_part()` ごとの再生成を避ける
    inner_delimiter: Vec<u8>,
    /// バッファ
    buffer: Vec<u8>,
    /// バッファ内の読み取り位置オフセット
    /// `buffer[pos..]` がまだ消費されていない有効領域
    /// 前詰めは `next_part()` がパートを返す直前に閾値判定して `drain` する
    pos: usize,
    /// パース状態
    state: ParserState,
    /// 完了フラグ
    finished: bool,
    /// バッファ最大サイズ (デフォルト: 10MB)
    max_buffer_size: usize,
    /// boundary 検索 (`first_delimiter` / `inner_delimiter`) の再開位置 (絶対オフセット)
    ///
    /// `find_bytes` で境界が見つからず `Incomplete` を返す前に「次回はどこから
    /// 再開するか」を覚えておくためのフィールド。Sans I/O で feed が複数回に
    /// 分かれた場合、毎回 `&self.buffer[self.pos..]` 全体を再走査すると O(N²·M)
    /// になり、`max_buffer_size` 範囲内でも攻撃者が CPU を浪費させる経路を
    /// 生む。本フィールドにより断片入力時の再走査を haystack 末尾から
    /// `needle.len() - 1` 分の overlap のみに抑え、検索コストを線形化する。
    ///
    /// 検索開始位置は `max(self.pos, self.boundary_scan_offset)` で算出する
    /// (pos が前進した場合は scan_offset が古いオフセットを指していても無害)。
    /// パートを切り出して状態遷移するときに `pos` 以上にリセットする。
    boundary_scan_offset: usize,
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

/// RFC 2046 Section 5.1.1: boundary で許可される文字
fn is_valid_boundary_char(b: u8) -> bool {
    matches!(b,
        b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' |
        b'\'' | b'(' | b')' | b'+' | b'_' | b',' | b'-' | b'.' |
        b'/' | b':' | b'=' | b'?' | b' '
    )
}

/// RFC 2046 Section 5.1.1: boundary の検証
fn is_valid_boundary(boundary: &str) -> bool {
    let bytes = boundary.as_bytes();
    // 1-70 文字
    if bytes.is_empty() || bytes.len() > 70 {
        return false;
    }
    // 許可された文字のみ
    if !bytes.iter().all(|&b| is_valid_boundary_char(b)) {
        return false;
    }
    // 末尾スペース不可
    bytes.last() != Some(&b' ')
}

impl MultipartParser {
    /// 新しいパーサーを作成
    ///
    /// バッファ上限は 10MB。変更する場合は `with_max_buffer_size()` を使用する。
    pub fn new(boundary: &str) -> Self {
        let boundary_bytes = boundary.as_bytes();
        let mut first_delimiter = Vec::with_capacity(2 + boundary_bytes.len());
        first_delimiter.extend_from_slice(b"--");
        first_delimiter.extend_from_slice(boundary_bytes);

        let mut inner_delimiter = Vec::with_capacity(4 + boundary_bytes.len());
        inner_delimiter.extend_from_slice(b"\r\n--");
        inner_delimiter.extend_from_slice(boundary_bytes);

        MultipartParser {
            first_delimiter,
            inner_delimiter,
            buffer: Vec::new(),
            pos: 0,
            state: ParserState::Initial,
            finished: false,
            max_buffer_size: 10 * 1024 * 1024,
            boundary_scan_offset: 0,
        }
    }

    /// バッファ最大サイズを設定
    pub fn with_max_buffer_size(mut self, max_buffer_size: usize) -> Self {
        self.max_buffer_size = max_buffer_size;
        self
    }

    /// boundary を検証して新しいパーサーを作成
    ///
    /// RFC 2046 Section 5.1.1 に従い、boundary 文字列を検証します。
    pub fn try_new(boundary: &str) -> Result<Self, MultipartError> {
        if !is_valid_boundary(boundary) {
            return Err(MultipartError::InvalidBoundary);
        }
        Ok(Self::new(boundary))
    }

    /// データを追加
    ///
    /// バッファサイズが `max_buffer_size` を超える場合は `MultipartError::BufferOverflow` を返す。
    /// 上限判定は未消費データ長 (`buffer.len() - pos`) に対して行う。
    /// `pos` 分は次回の `drain` で物理的に解放されるため、上限のセマンティクスは
    /// 「未消費の有効データ量」とする (オフセット方式に変更する前のセマンティクスを維持する)。
    pub fn feed(&mut self, data: &[u8]) -> Result<(), MultipartError> {
        let effective = self.buffer.len() - self.pos;
        let new_size = effective.saturating_add(data.len());
        if new_size > self.max_buffer_size {
            return Err(MultipartError::BufferOverflow {
                size: new_size,
                limit: self.max_buffer_size,
            });
        }
        self.buffer.extend_from_slice(data);
        Ok(())
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

        loop {
            match self.state {
                ParserState::Initial => {
                    // 最初の境界を探す。前回失敗位置 `boundary_scan_offset` から
                    // 再開することで断片入力時の O(N²·M) 再走査を回避する。
                    // `start` は `pos` 以上に揃える (pos が前進した場合に
                    // scan_offset が古い値のまま残っているケースを吸収)。
                    let start = self.pos.max(self.boundary_scan_offset);
                    let view = &self.buffer[start..];
                    if let Some(rel_pos) = find_bytes(view, &self.first_delimiter) {
                        let after_delim = start + rel_pos + self.first_delimiter.len();
                        // 直後 2 バイトで終端 (`--`) / 通常パート開始 (`\r\n`) を判定する。
                        // `self.buffer[after_delim..after_delim + 2]` を安全に参照できる
                        // 条件はバイト長が `after_delim + 2` 以上であること。`>=` で
                        // 等値も拾うことで「終端境界 `--<boundary>--` が feed 末尾
                        // ぴったりで止まった」入力でも Incomplete に落ちず、正しく
                        // 終端を検出できる (Sans I/O での断片入力対応)。
                        if self.buffer.len() >= after_delim + 2 {
                            if &self.buffer[after_delim..after_delim + 2] == b"\r\n" {
                                self.pos = after_delim + 2;
                                // 状態遷移したので scan_offset を pos に揃える
                                self.boundary_scan_offset = self.pos;
                                self.state = ParserState::InPart;
                            } else if &self.buffer[after_delim..after_delim + 2] == b"--" {
                                // 終了境界
                                self.state = ParserState::Finished;
                                self.finished = true;
                                return Ok(None);
                            } else {
                                // CRLF 以外の場合もパートに進む
                                self.pos = after_delim;
                                // 先頭の CRLF があればスキップ
                                if self.buffer[self.pos..].starts_with(b"\r\n") {
                                    self.pos += 2;
                                }
                                // 状態遷移したので scan_offset を pos に揃える
                                self.boundary_scan_offset = self.pos;
                                self.state = ParserState::InPart;
                            }
                        } else {
                            // 終端 2 バイトの判定が不能。次回 feed 後に同じ
                            // 検索位置から再開できるよう、見つけた boundary の
                            // 直前を覚えておく。
                            self.boundary_scan_offset = (start + rel_pos).min(self.buffer.len());
                            return Err(MultipartError::Incomplete);
                        }
                    } else {
                        // boundary が見つからなかった。haystack 末尾近くで
                        // overlap が起きる可能性があるので `needle.len() - 1`
                        // 分の overlap を残して次回再開位置を保存する。
                        let overlap = self.first_delimiter.len().saturating_sub(1);
                        self.boundary_scan_offset = self.buffer.len().saturating_sub(overlap);
                        return Err(MultipartError::Incomplete);
                    }
                }
                ParserState::InPart => {
                    // ヘッダーとボディの区切りを `buffer[pos..]` から相対位置で探す
                    let view = &self.buffer[self.pos..];
                    if let Some(header_end_rel) = find_bytes(view, b"\r\n\r\n") {
                        let header_end = self.pos + header_end_rel;
                        let header_bytes = &self.buffer[self.pos..header_end];
                        let body_start = header_end + 4;

                        // ヘッダーをパース
                        let headers_str = core::str::from_utf8(header_bytes)
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

                        // RFC 7578 Section 4.2: 各パートは Content-Disposition ヘッダーを
                        // 含まなければならない (MUST)
                        let content_disposition =
                            content_disposition.ok_or(MultipartError::MissingContentDisposition)?;

                        // RFC 7578 Section 4.2: disposition type は "form-data" でなければならない (MUST)
                        if !content_disposition.is_form_data() {
                            return Err(MultipartError::InvalidContentDisposition);
                        }

                        // RFC 7578 Section 4.2: "name" パラメータを含まなければならない (MUST)
                        if content_disposition.name().is_none() {
                            return Err(MultipartError::MissingName);
                        }

                        // 次の境界を探す。body_start は絶対オフセット、相対位置で検索。
                        // 前回失敗位置 `boundary_scan_offset` から再開して断片
                        // 入力時の O(N²·M) 再走査を回避する (body_start 以上に揃える)。
                        let search_start = body_start.max(self.boundary_scan_offset);
                        let body_view = &self.buffer[search_start..];
                        if let Some(body_end_rel) = find_bytes(body_view, &self.inner_delimiter) {
                            let body_end = search_start + body_end_rel;
                            // パートのボディは所有権移転で 1 回だけコピーする
                            let body = self.buffer[body_start..body_end].to_vec();

                            // 終了境界かどうか確認
                            let after_next = body_end + self.inner_delimiter.len();
                            if self.buffer.len() >= after_next + 2 {
                                if &self.buffer[after_next..after_next + 2] == b"--" {
                                    self.finished = true;
                                    self.state = ParserState::Finished;
                                } else if &self.buffer[after_next..after_next + 2] == b"\r\n" {
                                    self.pos = after_next + 2;
                                } else {
                                    self.pos = after_next;
                                }
                            } else {
                                self.pos = after_next;
                            }
                            // 次のパート検索は新しい開始位置から行うので scan_offset を pos に揃える
                            self.boundary_scan_offset = self.pos;

                            // 累積コピー量を amortized O(N) に抑える前詰め
                            // 発動条件は `pos` が物理バッファの過半を超えたときのみ
                            if self.pos > self.buffer.len() / 2 {
                                let drained = self.pos;
                                self.buffer.drain(..drained);
                                self.pos = 0;
                                // 前詰めしたので scan_offset も同じだけ前に移動
                                self.boundary_scan_offset =
                                    self.boundary_scan_offset.saturating_sub(drained);
                            }

                            return Ok(Some(Part {
                                content_disposition: Some(content_disposition),
                                content_type,
                                headers,
                                body,
                            }));
                        } else {
                            // boundary が見つからなかった。haystack 末尾近くで
                            // overlap が起きる可能性があるので `needle.len() - 1`
                            // 分の overlap を残して次回再開位置を保存する。
                            let overlap = self.inner_delimiter.len().saturating_sub(1);
                            self.boundary_scan_offset =
                                self.buffer.len().saturating_sub(overlap).max(body_start);
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
        let boundary = alloc::format!("----FormBoundary{}", random_value);
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

    /// boundary を検証して作成
    ///
    /// RFC 2046 Section 5.1.1 に従い、boundary 文字列を検証します。
    pub fn try_with_boundary(boundary: &str) -> Result<Self, MultipartError> {
        if !is_valid_boundary(boundary) {
            return Err(MultipartError::InvalidBoundary);
        }
        Ok(Self::with_boundary(boundary))
    }

    /// 境界文字列を取得
    pub fn boundary(&self) -> &str {
        &self.boundary
    }

    /// Content-Type ヘッダー値を取得
    ///
    /// RFC 9110 Section 5.6.6: boundary が token に該当しない場合は quoted-string で囲む
    pub fn content_type(&self) -> String {
        if self.boundary.bytes().all(is_token_char) {
            alloc::format!("multipart/form-data; boundary={}", self.boundary)
        } else {
            alloc::format!("multipart/form-data; boundary=\"{}\"", self.boundary)
        }
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
///
/// 実装は「needle の先頭バイト一致点を `iter().position()` で skip し、
/// 一致したら needle 全体を比較する」 first-byte skip 方式。最悪計算量は
/// O(N·M) のままだが、needle が稀なバイト (multipart boundary は `\r` で
/// 始まる) で始まるケースでは比較スキップにより定数倍を削減できる。
///
/// `memchr` クレートは導入しない (CLAUDE.md「依存は最小限」)。
fn find_bytes(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() {
        return Some(0);
    }
    if needle.len() > haystack.len() {
        return None;
    }

    let first = needle[0];
    let max_start = haystack.len() - needle.len();
    let mut i = 0;
    while i <= max_start {
        // 次の最初のバイト一致点までジャンプ
        let remaining = &haystack[i..=max_start];
        match remaining.iter().position(|&b| b == first) {
            Some(offset) => {
                i += offset;
                if &haystack[i..i + needle.len()] == needle {
                    return Some(i);
                }
                i += 1;
            }
            None => return None,
        }
    }
    None
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
        parser.feed(body).unwrap();

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
        parser.feed(body).unwrap();

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
        parser.feed(body).unwrap();

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
        parser.feed(&original_body).unwrap();

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
        parser.feed(&body).unwrap();

        let part = parser.next_part().unwrap().unwrap();
        assert_eq!(part.body(), &binary_data);
    }

    #[test]
    fn test_try_new_valid_boundary() {
        // RFC 2046 Section 5.1.1: 有効な boundary
        assert!(MultipartParser::try_new("simple").is_ok());
        assert!(MultipartParser::try_new("a-b_c.d").is_ok());
        assert!(MultipartParser::try_new("with space").is_ok());
        assert!(MultipartParser::try_new("----WebKitFormBoundary").is_ok());
    }

    #[test]
    fn test_try_new_invalid_boundary() {
        // RFC 2046 Section 5.1.1: 無効な boundary
        // 空
        assert!(MultipartParser::try_new("").is_err());
        // 71 文字以上
        assert!(MultipartParser::try_new(&"a".repeat(71)).is_err());
        // 末尾スペース
        assert!(MultipartParser::try_new("boundary ").is_err());
        // 許可されない文字
        assert!(MultipartParser::try_new("bound\x00ary").is_err());
        assert!(MultipartParser::try_new("bound*ary").is_err());
    }

    #[test]
    fn test_builder_try_with_boundary() {
        assert!(MultipartBuilder::try_with_boundary("valid-boundary").is_ok());
        assert!(MultipartBuilder::try_with_boundary("").is_err());
    }

    /// 多数パートを順次パースしたときに `buffer.drain` が発動して
    /// 物理バッファ長が累積入力長より十分小さくなることを検証する
    /// `buffer` / `pos` は非公開のためこのテストはモジュール内に置く
    #[test]
    fn test_parser_drain_keeps_buffer_small() {
        let boundary = "drain-boundary";
        let parts_count = 32;
        let part_body = vec![b'X'; 4096];
        let mut builder = MultipartBuilder::with_boundary(boundary);
        for i in 0..parts_count {
            let name = alloc::format!("field{i}");
            builder = builder.text_field(&name, core::str::from_utf8(&part_body).unwrap());
        }
        let body = builder.build();
        let total_len = body.len();

        let mut parser = MultipartParser::new(boundary).with_max_buffer_size(total_len);
        parser.feed(&body).unwrap();
        // 初期状態は累積入力長そのもの
        assert_eq!(parser.buffer.len(), total_len);

        let mut collected = 0usize;
        let mut min_buffer_len_after_drain = usize::MAX;
        while let Some(part) = parser.next_part().unwrap() {
            assert_eq!(part.body(), part_body.as_slice());
            collected += 1;
            // drain 発動済みなら `buffer.len()` は `total_len` より小さくなる
            if parser.buffer.len() < total_len {
                min_buffer_len_after_drain = min_buffer_len_after_drain.min(parser.buffer.len());
            }
        }
        assert_eq!(collected, parts_count);
        // drain が一度も発動しないと旧 O(N²) コピー相当のままなので、
        // 少なくとも 1 回は累積入力長を下回る縮小を観測することを要件にする
        assert!(
            min_buffer_len_after_drain < total_len,
            "drain never fired: total_len={total_len}",
        );
        // drain 発動時は残りデータ分まで縮む。緩めに `total_len * 3 / 4` を上限にして
        // 少なくとも一度は 1/4 以上の縮小が起きていることを確認する
        assert!(
            min_buffer_len_after_drain <= total_len * 3 / 4,
            "drain shrink too small: min={min_buffer_len_after_drain}, total_len={total_len}",
        );
    }

    /// `feed()` の上限判定は物理バッファ長 (`buffer.len()`) ではなく
    /// 未消費データ長 (`buffer.len() - pos`) で行う
    /// (オフセット方式に変更する前のセマンティクスを維持する)
    #[test]
    fn test_feed_buffer_limit_uses_unconsumed_length() {
        let mut parser = MultipartParser::new("b").with_max_buffer_size(40);
        // 30 バイト feed して pos を進める
        parser.feed(&[b'X'; 30]).unwrap();
        // 内部状態を直接いじって「20 バイト消費済み、10 バイト未消費」をシミュレートする
        // (drain 発動を待たずに pos > 0 の状態を作る)
        parser.pos = 20;
        assert_eq!(parser.buffer.len(), 30);
        // 未消費長は 10。max_buffer_size = 40 なので 30 バイト追加 feed は可能
        // 物理バッファ長 (30) で判定すると 30 + 30 = 60 > 40 で false positive になる
        assert!(parser.feed(&[b'Y'; 30]).is_ok());
        // 未消費長は 40 ぴったり。これ以上は弾く
        assert!(matches!(
            parser.feed(b"Z"),
            Err(MultipartError::BufferOverflow {
                size: 41,
                limit: 40
            })
        ));
    }

    /// 部分 feed をまたいだ pos / buffer の整合性を検証する
    #[test]
    fn test_parser_partial_feed_sequence() {
        let boundary = "split-boundary";
        let body = MultipartBuilder::with_boundary(boundary)
            .text_field("field1", "value1")
            .text_field("field2", "value2")
            .build();

        let mut parser = MultipartParser::new(boundary);
        // 1 バイトずつ feed しながら部分パースする
        let mut feed_pos = 0usize;
        let mut collected: Vec<Part> = Vec::new();
        while feed_pos < body.len() {
            // 数バイトずつ送り込む
            let chunk_end = (feed_pos + 7).min(body.len());
            parser.feed(&body[feed_pos..chunk_end]).unwrap();
            feed_pos = chunk_end;

            loop {
                match parser.next_part() {
                    Ok(Some(part)) => collected.push(part),
                    Ok(None) => break,
                    Err(MultipartError::Incomplete) => break,
                    Err(e) => panic!("unexpected error: {e:?}"),
                }
            }
        }

        assert_eq!(collected.len(), 2);
        assert_eq!(collected[0].name(), Some("field1"));
        assert_eq!(collected[0].body_str(), Some("value1"));
        assert_eq!(collected[1].name(), Some("field2"));
        assert_eq!(collected[1].body_str(), Some("value2"));
        assert!(parser.is_finished());
    }
}
