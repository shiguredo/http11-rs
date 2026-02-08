use std::fmt;

use crate::compression::CompressionError;

/// HTTP パースエラー
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    /// 不正なデータ
    InvalidData(String),
    /// バッファサイズ超過
    BufferOverflow { size: usize, limit: usize },
    /// ヘッダー数超過
    TooManyHeaders { count: usize, limit: usize },
    /// ヘッダー行が長すぎる
    HeaderLineTooLong { size: usize, limit: usize },
    /// ボディサイズ超過
    BodyTooLarge { size: usize, limit: usize },
    /// チャンクサイズ行が長すぎる
    ChunkLineTooLong { size: usize, limit: usize },
    /// 圧縮/展開エラー
    Compression(CompressionError),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::InvalidData(msg) => write!(f, "invalid data: {}", msg),
            Error::BufferOverflow { size, limit } => {
                write!(f, "buffer overflow: {} > {}", size, limit)
            }
            Error::TooManyHeaders { count, limit } => {
                write!(f, "too many headers: {} > {}", count, limit)
            }
            Error::HeaderLineTooLong { size, limit } => {
                write!(f, "header line too long: {} > {}", size, limit)
            }
            Error::BodyTooLarge { size, limit } => {
                write!(f, "body too large: {} > {}", size, limit)
            }
            Error::ChunkLineTooLong { size, limit } => {
                write!(f, "chunk line too long: {} > {}", size, limit)
            }
            Error::Compression(e) => write!(f, "compression error: {}", e),
        }
    }
}

impl std::error::Error for Error {}

impl From<CompressionError> for Error {
    fn from(e: CompressionError) -> Self {
        Error::Compression(e)
    }
}

/// HTTP エンコードエラー
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EncodeError {
    /// Host ヘッダーがない (HTTP/1.1 必須)
    MissingHostHeader,
    /// Transfer-Encoding と Content-Length が同時に設定されている
    /// RFC 9112 Section 6.2: 送信者は Transfer-Encoding を含むメッセージに
    /// Content-Length を含めてはならない (MUST NOT)
    ConflictingTransferEncodingAndContentLength,
    /// 1xx / 204 レスポンスで Transfer-Encoding が設定されている
    /// RFC 9112 Section 6.1: サーバーは 1xx または 204 レスポンスに
    /// Transfer-Encoding を含めてはならない (MUST NOT)
    ForbiddenTransferEncoding { status_code: u16 },
    /// 不正なメソッド名
    InvalidMethod { method: String },
    /// 不正なリクエストターゲット
    InvalidRequestTarget { uri: String },
    /// 不正な HTTP バージョン
    InvalidVersion { version: String },
    /// 不正なヘッダー名
    InvalidHeaderName { name: String },
    /// 不正なヘッダー値
    InvalidHeaderValue { name: String, value: String },
    /// 不正なステータスコード
    InvalidStatusCode { code: u16 },
    /// 不正な reason-phrase
    InvalidReasonPhrase { phrase: String },
    /// Host ヘッダーが重複している
    DuplicateHostHeader,
    /// Host ヘッダーの値が不正
    InvalidHostHeader { value: String },
    /// Host ヘッダーと request-target の authority が一致しない
    HostAuthorityMismatch { host: String, authority: String },
    /// 205 Reset Content レスポンスにボディが含まれている
    /// RFC 9110 Section 15.3.6: 205 レスポンスはボディを生成してはならない
    ForbiddenBodyFor205,
    /// 1xx / 204 レスポンスで Content-Length が設定されている
    /// RFC 9110 Section 8.6: サーバーは 1xx または 204 レスポンスに
    /// Content-Length を含めてはならない (MUST NOT)
    ForbiddenContentLength { status_code: u16 },
    /// Content-Length ヘッダーの値と実際のボディサイズが一致しない
    ContentLengthMismatch { header_value: u64, body_length: u64 },
}

impl fmt::Display for EncodeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            EncodeError::MissingHostHeader => {
                write!(f, "missing Host header (required for HTTP/1.1)")
            }
            EncodeError::ConflictingTransferEncodingAndContentLength => {
                write!(
                    f,
                    "conflicting Transfer-Encoding and Content-Length headers (RFC 9112 Section 6.2)"
                )
            }
            EncodeError::ForbiddenTransferEncoding { status_code } => {
                write!(
                    f,
                    "Transfer-Encoding not allowed for {} response (RFC 9112 Section 6.1)",
                    status_code
                )
            }
            EncodeError::InvalidMethod { method } => {
                write!(f, "invalid method: {:?}", method)
            }
            EncodeError::InvalidRequestTarget { uri } => {
                write!(f, "invalid request-target: {:?}", uri)
            }
            EncodeError::InvalidVersion { version } => {
                write!(f, "invalid HTTP version: {:?}", version)
            }
            EncodeError::InvalidHeaderName { name } => {
                write!(f, "invalid header name: {:?}", name)
            }
            EncodeError::InvalidHeaderValue { name, value } => {
                write!(f, "invalid header value for {:?}: {:?}", name, value)
            }
            EncodeError::InvalidStatusCode { code } => {
                write!(f, "invalid status code: {}", code)
            }
            EncodeError::InvalidReasonPhrase { phrase } => {
                write!(f, "invalid reason-phrase: {:?}", phrase)
            }
            EncodeError::DuplicateHostHeader => {
                write!(f, "duplicate Host header")
            }
            EncodeError::InvalidHostHeader { value } => {
                write!(f, "invalid Host header value: {:?}", value)
            }
            EncodeError::HostAuthorityMismatch { host, authority } => {
                write!(
                    f,
                    "Host header {:?} does not match request-target authority {:?}",
                    host, authority
                )
            }
            EncodeError::ForbiddenBodyFor205 => {
                write!(
                    f,
                    "205 Reset Content must not contain a body (RFC 9110 Section 15.3.6)"
                )
            }
            EncodeError::ForbiddenContentLength { status_code } => {
                write!(
                    f,
                    "Content-Length not allowed for {} response (RFC 9110 Section 8.6)",
                    status_code
                )
            }
            EncodeError::ContentLengthMismatch {
                header_value,
                body_length,
            } => {
                write!(
                    f,
                    "Content-Length header value {} does not match body length {}",
                    header_value, body_length
                )
            }
        }
    }
}

impl std::error::Error for EncodeError {}
