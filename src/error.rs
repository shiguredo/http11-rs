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
}

impl fmt::Display for EncodeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            EncodeError::MissingHostHeader => {
                write!(f, "missing Host header (required for HTTP/1.1)")
            }
        }
    }
}

impl std::error::Error for EncodeError {}
