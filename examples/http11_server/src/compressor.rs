//! 圧縮器の実装 (gzip, br, zstd)
//!
//! shiguredo_http11 の Compressor トレイトを実装する。
//! 各圧縮形式は feature フラグで有効化される。

#[cfg(feature = "gzip")]
use std::io::Write;

use shiguredo_http11::compression::CompressionError;

#[cfg(any(feature = "gzip", feature = "br", feature = "zstd"))]
use shiguredo_http11::compression::{CompressionStatus, Compressor};

// ============================================================================
// gzip 圧縮器
// ============================================================================

#[cfg(feature = "gzip")]
#[allow(dead_code)]
pub struct GzipCompressor {
    encoder: Option<flate2::write::GzEncoder<Vec<u8>>>,
    finished: bool,
}

#[cfg(feature = "gzip")]
impl std::fmt::Debug for GzipCompressor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GzipCompressor")
            .field("finished", &self.finished)
            .finish()
    }
}

#[cfg(feature = "gzip")]
#[allow(dead_code)]
impl GzipCompressor {
    pub fn new() -> Self {
        Self {
            encoder: Some(flate2::write::GzEncoder::new(
                Vec::new(),
                flate2::Compression::default(),
            )),
            finished: false,
        }
    }

    pub fn with_level(level: u32) -> Self {
        Self {
            encoder: Some(flate2::write::GzEncoder::new(
                Vec::new(),
                flate2::Compression::new(level),
            )),
            finished: false,
        }
    }
}

#[cfg(feature = "gzip")]
impl Default for GzipCompressor {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(feature = "gzip")]
impl Compressor for GzipCompressor {
    fn compress(
        &mut self,
        input: &[u8],
        output: &mut [u8],
    ) -> Result<CompressionStatus, CompressionError> {
        if self.finished {
            return Err(CompressionError::AlreadyFinished);
        }

        let encoder = self
            .encoder
            .as_mut()
            .ok_or_else(|| CompressionError::Internal("encoder not available".to_string()))?;

        encoder
            .write_all(input)
            .map_err(|e| CompressionError::Internal(e.to_string()))?;

        // 内部バッファから出力にコピー
        let inner_len = encoder.get_ref().len();
        let len = inner_len.min(output.len());
        output[..len].copy_from_slice(&encoder.get_ref()[..len]);

        // コピーした分を内部バッファから削除
        encoder.get_mut().drain(..len);

        if len < inner_len {
            Ok(CompressionStatus::OutputFull {
                consumed: input.len(),
                produced: len,
            })
        } else {
            Ok(CompressionStatus::Continue {
                consumed: input.len(),
                produced: len,
            })
        }
    }

    fn finish(&mut self, output: &mut [u8]) -> Result<CompressionStatus, CompressionError> {
        if self.finished {
            return Err(CompressionError::AlreadyFinished);
        }

        let encoder = self
            .encoder
            .take()
            .ok_or_else(|| CompressionError::Internal("encoder not available".to_string()))?;

        let compressed = encoder
            .finish()
            .map_err(|e| CompressionError::Internal(e.to_string()))?;

        let len = compressed.len().min(output.len());
        output[..len].copy_from_slice(&compressed[..len]);
        self.finished = true;

        if len < compressed.len() {
            Ok(CompressionStatus::OutputFull {
                consumed: 0,
                produced: len,
            })
        } else {
            Ok(CompressionStatus::Complete {
                consumed: 0,
                produced: len,
            })
        }
    }

    fn reset(&mut self) {
        self.encoder = Some(flate2::write::GzEncoder::new(
            Vec::new(),
            flate2::Compression::default(),
        ));
        self.finished = false;
    }
}

// ============================================================================
// Brotli 圧縮器
// ============================================================================

#[cfg(feature = "br")]
#[allow(dead_code)]
pub struct BrotliCompressor {
    buffer: Vec<u8>,
    quality: u32,
    finished: bool,
}

#[cfg(feature = "br")]
impl std::fmt::Debug for BrotliCompressor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BrotliCompressor")
            .field("quality", &self.quality)
            .field("finished", &self.finished)
            .finish()
    }
}

#[cfg(feature = "br")]
#[allow(dead_code)]
impl BrotliCompressor {
    pub fn new() -> Self {
        Self {
            buffer: Vec::new(),
            quality: 4, // デフォルト品質 (0-11)
            finished: false,
        }
    }

    pub fn with_quality(quality: u32) -> Self {
        Self {
            buffer: Vec::new(),
            quality: quality.min(11),
            finished: false,
        }
    }
}

#[cfg(feature = "br")]
impl Default for BrotliCompressor {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(feature = "br")]
impl Compressor for BrotliCompressor {
    fn compress(
        &mut self,
        input: &[u8],
        _output: &mut [u8],
    ) -> Result<CompressionStatus, CompressionError> {
        if self.finished {
            return Err(CompressionError::AlreadyFinished);
        }

        // 入力をバッファに追加
        self.buffer.extend_from_slice(input);

        Ok(CompressionStatus::Continue {
            consumed: input.len(),
            produced: 0,
        })
    }

    fn finish(&mut self, output: &mut [u8]) -> Result<CompressionStatus, CompressionError> {
        use std::io::Write;

        if self.finished {
            return Err(CompressionError::AlreadyFinished);
        }

        let mut compressed = Vec::new();
        {
            let mut encoder =
                brotli::CompressorWriter::new(&mut compressed, 4096, self.quality, 22);
            encoder
                .write_all(&self.buffer)
                .map_err(|e| CompressionError::Internal(e.to_string()))?;
        }

        let len = compressed.len().min(output.len());
        output[..len].copy_from_slice(&compressed[..len]);
        self.finished = true;
        self.buffer.clear();

        if len < compressed.len() {
            Ok(CompressionStatus::OutputFull {
                consumed: 0,
                produced: len,
            })
        } else {
            Ok(CompressionStatus::Complete {
                consumed: 0,
                produced: len,
            })
        }
    }

    fn reset(&mut self) {
        self.buffer.clear();
        self.finished = false;
    }
}

// ============================================================================
// Zstandard 圧縮器
// ============================================================================

#[cfg(feature = "zstd")]
#[allow(dead_code)]
pub struct ZstdCompressor {
    buffer: Vec<u8>,
    level: i32,
    finished: bool,
}

#[cfg(feature = "zstd")]
impl std::fmt::Debug for ZstdCompressor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ZstdCompressor")
            .field("level", &self.level)
            .field("finished", &self.finished)
            .finish()
    }
}

#[cfg(feature = "zstd")]
#[allow(dead_code)]
impl ZstdCompressor {
    pub fn new() -> Self {
        Self {
            buffer: Vec::new(),
            level: 3, // デフォルトレベル
            finished: false,
        }
    }

    pub fn with_level(level: i32) -> Self {
        Self {
            buffer: Vec::new(),
            level,
            finished: false,
        }
    }
}

#[cfg(feature = "zstd")]
impl Default for ZstdCompressor {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(feature = "zstd")]
impl Compressor for ZstdCompressor {
    fn compress(
        &mut self,
        input: &[u8],
        _output: &mut [u8],
    ) -> Result<CompressionStatus, CompressionError> {
        if self.finished {
            return Err(CompressionError::AlreadyFinished);
        }

        // 入力をバッファに追加
        self.buffer.extend_from_slice(input);

        Ok(CompressionStatus::Continue {
            consumed: input.len(),
            produced: 0,
        })
    }

    fn finish(&mut self, output: &mut [u8]) -> Result<CompressionStatus, CompressionError> {
        if self.finished {
            return Err(CompressionError::AlreadyFinished);
        }

        let compressed = zstd::encode_all(self.buffer.as_slice(), self.level)
            .map_err(|e| CompressionError::Internal(e.to_string()))?;

        let len = compressed.len().min(output.len());
        output[..len].copy_from_slice(&compressed[..len]);
        self.finished = true;
        self.buffer.clear();

        if len < compressed.len() {
            Ok(CompressionStatus::OutputFull {
                consumed: 0,
                produced: len,
            })
        } else {
            Ok(CompressionStatus::Complete {
                consumed: 0,
                produced: len,
            })
        }
    }

    fn reset(&mut self) {
        self.buffer.clear();
        self.finished = false;
    }
}

// ============================================================================
// ユーティリティ関数
// ============================================================================

/// Accept-Encoding ヘッダーから最適な圧縮方式を選択
///
/// 優先順位: zstd > br > gzip > identity
/// 有効な feature のみが選択対象となる。
#[cfg(any(feature = "gzip", feature = "br", feature = "zstd"))]
pub fn select_encoding(accept_encoding: &str) -> Option<&'static str> {
    let encodings: Vec<(&str, f32)> = accept_encoding
        .split(',')
        .filter_map(|part| {
            let part = part.trim();
            let (encoding, quality) = if let Some(pos) = part.find(";q=") {
                let enc = part[..pos].trim();
                let q: f32 = part[pos + 3..].trim().parse().unwrap_or(1.0);
                (enc, q)
            } else {
                (part, 1.0)
            };
            if quality > 0.0 {
                Some((encoding, quality))
            } else {
                None
            }
        })
        .collect();

    // 優先順位でソート（quality が高い順、同じなら zstd > br > gzip の順）
    let mut best: Option<(&str, f32, u8)> = None;

    for (enc, q) in encodings {
        let priority = match enc {
            #[cfg(feature = "zstd")]
            "zstd" => 3,
            #[cfg(feature = "br")]
            "br" => 2,
            #[cfg(feature = "gzip")]
            "gzip" | "x-gzip" => 1,
            #[cfg(feature = "gzip")]
            "*" => 1,
            _ => continue,
        };

        match best {
            None => best = Some((enc, q, priority)),
            Some((_, best_q, best_p)) => {
                if q > best_q || (q == best_q && priority > best_p) {
                    best = Some((enc, q, priority));
                }
            }
        }
    }

    best.map(|(enc, _, _)| match enc {
        #[cfg(feature = "zstd")]
        "zstd" => "zstd",
        #[cfg(feature = "br")]
        "br" => "br",
        #[cfg(feature = "gzip")]
        "gzip" | "x-gzip" | "*" => "gzip",
        _ => {
            // フォールバック: 有効な feature で最初に見つかったものを返す
            #[cfg(feature = "gzip")]
            {
                return "gzip";
            }
            #[cfg(all(feature = "br", not(feature = "gzip")))]
            {
                return "br";
            }
            #[cfg(all(feature = "zstd", not(feature = "gzip"), not(feature = "br")))]
            {
                return "zstd";
            }
            #[allow(unreachable_code)]
            "identity"
        }
    })
}

/// Accept-Encoding ヘッダーから最適な圧縮方式を選択（圧縮機能無効時）
#[cfg(not(any(feature = "gzip", feature = "br", feature = "zstd")))]
pub fn select_encoding(_accept_encoding: &str) -> Option<&'static str> {
    None
}

/// 圧縮方式に対応する Content-Encoding 値
pub fn encoding_header(encoding: &str) -> &'static str {
    match encoding {
        #[cfg(feature = "zstd")]
        "zstd" => "zstd",
        #[cfg(feature = "br")]
        "br" => "br",
        #[cfg(feature = "gzip")]
        "gzip" | "x-gzip" => "gzip",
        _ => "identity",
    }
}

/// ボディを一括圧縮
pub fn compress_body(data: &[u8], encoding: &str) -> Result<Vec<u8>, CompressionError> {
    match encoding {
        #[cfg(feature = "gzip")]
        "gzip" => {
            use std::io::Write;
            let mut encoder =
                flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
            encoder
                .write_all(data)
                .map_err(|e| CompressionError::Internal(e.to_string()))?;
            encoder
                .finish()
                .map_err(|e| CompressionError::Internal(e.to_string()))
        }
        #[cfg(feature = "br")]
        "br" => {
            use std::io::Write;
            let mut compressed = Vec::new();
            {
                let mut encoder = brotli::CompressorWriter::new(&mut compressed, 4096, 4, 22);
                encoder
                    .write_all(data)
                    .map_err(|e| CompressionError::Internal(e.to_string()))?;
            }
            Ok(compressed)
        }
        #[cfg(feature = "zstd")]
        "zstd" => zstd::encode_all(data, 3).map_err(|e| CompressionError::Internal(e.to_string())),
        _ => Ok(data.to_vec()),
    }
}
