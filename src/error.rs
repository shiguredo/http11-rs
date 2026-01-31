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
        }
    }
}

impl std::error::Error for EncodeError {}
