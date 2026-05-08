//! 圧縮器の実装 (gzip, br, zstd)
//!
//! shiguredo_http11 の Compressor トレイトを実装する。

use shiguredo_http11::compression::{CompressionError, CompressionStatus, Compressor};

// ============================================================================
// gzip 圧縮器
// ============================================================================

#[allow(dead_code)]
pub struct GzipCompressor {
    encoder: noflate::gzip::Encoder,
    finished: bool,
}

impl std::fmt::Debug for GzipCompressor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GzipCompressor")
            .field("finished", &self.finished)
            .finish()
    }
}

#[allow(dead_code)]
impl GzipCompressor {
    pub fn new() -> Self {
        Self {
            encoder: noflate::gzip::Encoder::new(),
            finished: false,
        }
    }
}

impl Default for GzipCompressor {
    fn default() -> Self {
        Self::new()
    }
}

impl Compressor for GzipCompressor {
    fn compress(
        &mut self,
        input: &[u8],
        output: &mut [u8],
    ) -> Result<CompressionStatus, CompressionError> {
        if self.finished {
            return Err(CompressionError::AlreadyFinished);
        }

        self.encoder
            .feed(input)
            .map_err(|e| CompressionError::Internal(e.to_string()))?;

        let available = self.encoder.output();
        let len = available.len().min(output.len());
        output[..len].copy_from_slice(&available[..len]);
        let total = available.len();
        self.encoder.advance(len);

        if len < total {
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

        self.encoder
            .finish()
            .map_err(|e| CompressionError::Internal(e.to_string()))?;

        let available = self.encoder.output();
        let total = available.len();
        let len = total.min(output.len());
        output[..len].copy_from_slice(&available[..len]);
        self.encoder.advance(len);

        if len < total {
            Ok(CompressionStatus::OutputFull {
                consumed: 0,
                produced: len,
            })
        } else {
            self.finished = true;
            Ok(CompressionStatus::Complete {
                consumed: 0,
                produced: len,
            })
        }
    }

    fn reset(&mut self) {
        self.encoder = noflate::gzip::Encoder::new();
        self.finished = false;
    }
}

// ============================================================================
// Brotli 圧縮器
// ============================================================================

#[allow(dead_code)]
pub struct BrotliCompressor {
    buffer: Vec<u8>,
    quality: u32,
    finished: bool,
}

impl std::fmt::Debug for BrotliCompressor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BrotliCompressor")
            .field("quality", &self.quality)
            .field("finished", &self.finished)
            .finish()
    }
}

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

impl Default for BrotliCompressor {
    fn default() -> Self {
        Self::new()
    }
}

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

#[allow(dead_code)]
pub struct ZstdCompressor {
    buffer: Vec<u8>,
    level: i32,
    finished: bool,
}

impl std::fmt::Debug for ZstdCompressor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ZstdCompressor")
            .field("level", &self.level)
            .field("finished", &self.finished)
            .finish()
    }
}

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

impl Default for ZstdCompressor {
    fn default() -> Self {
        Self::new()
    }
}

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
            "zstd" => 3,
            "br" => 2,
            "gzip" | "x-gzip" | "*" => 1,
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
        "zstd" => "zstd",
        "br" => "br",
        _ => "gzip",
    })
}

/// 圧縮方式に対応する Content-Encoding 値
pub fn encoding_header(encoding: &str) -> &'static str {
    match encoding {
        "zstd" => "zstd",
        "br" => "br",
        "gzip" | "x-gzip" => "gzip",
        _ => "identity",
    }
}

/// ボディを一括圧縮
pub fn compress_body(data: &[u8], encoding: &str) -> Result<Vec<u8>, CompressionError> {
    match encoding {
        "gzip" => {
            noflate::gzip::compress(data).map_err(|e| CompressionError::Internal(e.to_string()))
        }
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
        "zstd" => zstd::encode_all(data, 3).map_err(|e| CompressionError::Internal(e.to_string())),
        _ => Ok(data.to_vec()),
    }
}
