//! 展開器の実装 (gzip, br, zstd)
//!
//! shiguredo_http11 の Decompressor トレイトを実装する。
//! 各展開形式は feature フラグで有効化される。

use shiguredo_http11::compression::CompressionError;

#[cfg(any(feature = "gzip", feature = "br", feature = "zstd"))]
use shiguredo_http11::compression::{CompressionStatus, Decompressor};

// ============================================================================
// gzip 展開器
// ============================================================================

#[cfg(feature = "gzip")]
#[allow(dead_code)]
pub struct GzipDecompressor {
    buffer: Vec<u8>,
}

#[cfg(feature = "gzip")]
impl std::fmt::Debug for GzipDecompressor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GzipDecompressor")
            .field("buffer_len", &self.buffer.len())
            .finish()
    }
}

#[cfg(feature = "gzip")]
#[allow(dead_code)]
impl GzipDecompressor {
    pub fn new() -> Self {
        Self { buffer: Vec::new() }
    }
}

#[cfg(feature = "gzip")]
impl Default for GzipDecompressor {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(feature = "gzip")]
impl Decompressor for GzipDecompressor {
    fn decompress(
        &mut self,
        input: &[u8],
        _output: &mut [u8],
    ) -> Result<CompressionStatus, CompressionError> {
        // 入力をバッファに追加
        self.buffer.extend_from_slice(input);

        // 空入力で Complete を返す（ストリーム終端）
        if input.is_empty() && self.buffer.is_empty() {
            return Ok(CompressionStatus::Complete {
                consumed: 0,
                produced: 0,
            });
        }

        Ok(CompressionStatus::Continue {
            consumed: input.len(),
            produced: 0,
        })
    }

    fn reset(&mut self) {
        self.buffer.clear();
    }
}

// ============================================================================
// Brotli 展開器
// ============================================================================

#[cfg(feature = "br")]
#[allow(dead_code)]
pub struct BrotliDecompressor {
    buffer: Vec<u8>,
}

#[cfg(feature = "br")]
impl std::fmt::Debug for BrotliDecompressor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BrotliDecompressor")
            .field("buffer_len", &self.buffer.len())
            .finish()
    }
}

#[cfg(feature = "br")]
#[allow(dead_code)]
impl BrotliDecompressor {
    pub fn new() -> Self {
        Self { buffer: Vec::new() }
    }
}

#[cfg(feature = "br")]
impl Default for BrotliDecompressor {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(feature = "br")]
impl Decompressor for BrotliDecompressor {
    fn decompress(
        &mut self,
        input: &[u8],
        _output: &mut [u8],
    ) -> Result<CompressionStatus, CompressionError> {
        // 入力をバッファに追加
        self.buffer.extend_from_slice(input);

        // 空入力で Complete を返す（ストリーム終端）
        if input.is_empty() && self.buffer.is_empty() {
            return Ok(CompressionStatus::Complete {
                consumed: 0,
                produced: 0,
            });
        }

        Ok(CompressionStatus::Continue {
            consumed: input.len(),
            produced: 0,
        })
    }

    fn reset(&mut self) {
        self.buffer.clear();
    }
}

// ============================================================================
// Zstandard 展開器
// ============================================================================

#[cfg(feature = "zstd")]
#[allow(dead_code)]
pub struct ZstdDecompressor {
    buffer: Vec<u8>,
}

#[cfg(feature = "zstd")]
impl std::fmt::Debug for ZstdDecompressor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ZstdDecompressor")
            .field("buffer_len", &self.buffer.len())
            .finish()
    }
}

#[cfg(feature = "zstd")]
#[allow(dead_code)]
impl ZstdDecompressor {
    pub fn new() -> Self {
        Self { buffer: Vec::new() }
    }
}

#[cfg(feature = "zstd")]
impl Default for ZstdDecompressor {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(feature = "zstd")]
impl Decompressor for ZstdDecompressor {
    fn decompress(
        &mut self,
        input: &[u8],
        _output: &mut [u8],
    ) -> Result<CompressionStatus, CompressionError> {
        // 入力をバッファに追加
        self.buffer.extend_from_slice(input);

        // 空入力で Complete を返す（ストリーム終端）
        if input.is_empty() && self.buffer.is_empty() {
            return Ok(CompressionStatus::Complete {
                consumed: 0,
                produced: 0,
            });
        }

        Ok(CompressionStatus::Continue {
            consumed: input.len(),
            produced: 0,
        })
    }

    fn reset(&mut self) {
        self.buffer.clear();
    }
}

// ============================================================================
// ユーティリティ関数
// ============================================================================

/// Content-Encoding に基づいてボディを一括展開
pub fn decompress_body(data: &[u8], encoding: &str) -> Result<Vec<u8>, CompressionError> {
    match encoding.to_lowercase().as_str() {
        #[cfg(feature = "gzip")]
        "gzip" | "x-gzip" => {
            use std::io::Read;
            let mut decoder = flate2::read::GzDecoder::new(data);
            let mut decompressed = Vec::new();
            decoder
                .read_to_end(&mut decompressed)
                .map_err(|e| CompressionError::InvalidData(e.to_string()))?;
            Ok(decompressed)
        }
        #[cfg(feature = "br")]
        "br" => {
            let mut decompressed = Vec::new();
            brotli::BrotliDecompress(&mut std::io::Cursor::new(data), &mut decompressed)
                .map_err(|e| CompressionError::InvalidData(e.to_string()))?;
            Ok(decompressed)
        }
        #[cfg(feature = "zstd")]
        "zstd" => {
            zstd::decode_all(data).map_err(|e| CompressionError::InvalidData(e.to_string()))
        }
        "identity" | "" => Ok(data.to_vec()),
        _ => Err(CompressionError::InvalidData(format!(
            "unsupported encoding: {}",
            encoding
        ))),
    }
}

/// サポートされている展開形式のリストを返す
pub fn supported_encodings() -> &'static str {
    #[cfg(all(feature = "gzip", feature = "br", feature = "zstd"))]
    {
        "gzip, br, zstd"
    }
    #[cfg(all(feature = "gzip", feature = "br", not(feature = "zstd")))]
    {
        "gzip, br"
    }
    #[cfg(all(feature = "gzip", not(feature = "br"), feature = "zstd"))]
    {
        "gzip, zstd"
    }
    #[cfg(all(not(feature = "gzip"), feature = "br", feature = "zstd"))]
    {
        "br, zstd"
    }
    #[cfg(all(feature = "gzip", not(feature = "br"), not(feature = "zstd")))]
    {
        "gzip"
    }
    #[cfg(all(not(feature = "gzip"), feature = "br", not(feature = "zstd")))]
    {
        "br"
    }
    #[cfg(all(not(feature = "gzip"), not(feature = "br"), feature = "zstd"))]
    {
        "zstd"
    }
    #[cfg(all(not(feature = "gzip"), not(feature = "br"), not(feature = "zstd")))]
    {
        ""
    }
}
