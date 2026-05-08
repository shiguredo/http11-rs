//! 展開器の実装 (gzip, br, zstd)
//!
//! shiguredo_http11 の `Decompressor` トレイトを gzip / brotli / zstd に対し実装する。
//! 各実装は対応する crate のストリーミング API を直接駆動するため、
//! 1 GiB のレスポンスボディでもメモリ常駐させずに 8 KiB 単位で展開できる。
//!
//! `AnyDecompressor` は Content-Encoding ヘッダーを受信してから展開器の
//! 種別を確定させるユースケース向けの enum ラッパー。
//! Content-Encoding ごとに別の concrete type を持つ `Decompressor` 実装を
//! 動的に切り替えるため、enum で variant をまとめて trait を impl している。

use brotli::{BrotliDecompressStream, BrotliResult, BrotliState, HeapAlloc, HuffmanCode};
use shiguredo_http11::compression::{
    CompressionError, CompressionStatus, Decompressor, NoCompression,
};

// ============================================================================
// gzip 展開器 (noflate::gzip::Decoder のラップ)
// ============================================================================

/// gzip ストリーミング展開器
///
/// `noflate::gzip::Decoder` は `feed` で input を全消費して内部 output buffer に
/// 展開済みバイトを蓄積する API スタイル。`Decompressor` トレイトは
/// `(input, output) -> (consumed, produced)` の back-pressure 型なので、両者を
/// 橋渡しするため input を `FEED_CHUNK` (= 4 KiB) ずつ feed しては output に
/// drain する形で適用する。これにより:
///
/// - `consumed` は呼び出し側 output 容量内に収まる範囲を反映する
/// - 内部 output buffer の蓄積が `FEED_CHUNK * 圧縮率` 程度に抑えられる
/// - back-pressure を呼び出し側が認識し、未消費 input を次回呼び出しで再度
///   受け取れる (peek_body の戻り値が consume_body されないため)
///
/// 末尾の極端ケース (最後の数 byte が極端に展開され output 容量を超える) では
/// 内部 buffer に leftover が残るが、それは次回 `decompress(&[], output)`
/// (例: `ResponseDecoder::peek_body_decompressed` がボディ枯渇後に行う drain)
/// で回収される。
pub struct GzipDecompressor {
    decoder: noflate::gzip::Decoder,
}

/// 1 回の `feed` で投入する input の最大バイト数
///
/// 小さいほど内部 buffer の蓄積を抑えて back-pressure が綺麗になるが、
/// `feed` の呼び出し回数が増えるためトレードオフ。
/// 4 KiB はテキスト系の典型圧縮率 (3〜10x) で 8 KiB 出力バッファに数回の
/// 反復で展開しきれる程度の値として選択。
const GZIP_FEED_CHUNK: usize = 4096;

impl GzipDecompressor {
    pub fn new() -> Self {
        Self {
            decoder: noflate::gzip::Decoder::new(),
        }
    }

    /// 内部 output buffer から `output` の先頭 `output.len()` バイト分まで drain する
    ///
    /// 戻り値は実際に書き込んだバイト数。
    fn drain_into(&mut self, output: &mut [u8]) -> usize {
        let avail = self.decoder.output();
        let n = avail.len().min(output.len());
        if n == 0 {
            return 0;
        }
        output[..n].copy_from_slice(&avail[..n]);
        self.decoder.advance(n);
        n
    }
}

impl Default for GzipDecompressor {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Debug for GzipDecompressor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GzipDecompressor")
            .field("buffered_output_len", &self.decoder.output().len())
            .field("finished", &self.decoder.is_finished())
            .finish()
    }
}

impl Decompressor for GzipDecompressor {
    fn decompress(
        &mut self,
        input: &[u8],
        output: &mut [u8],
    ) -> Result<CompressionStatus, CompressionError> {
        let mut consumed = 0usize;
        let mut produced = 0usize;

        // 1. 前回の呼び出しで残った内部 buffer を先に drain する
        produced += self.drain_into(&mut output[produced..]);
        if !self.decoder.output().is_empty() {
            // output が満杯になったが内部 buffer に leftover あり: caller に back-pressure を返す
            return Ok(CompressionStatus::OutputFull { consumed, produced });
        }

        // 2. input を GZIP_FEED_CHUNK ずつ feed → drain の繰り返しで処理する
        while consumed < input.len() && produced < output.len() {
            let feed_size = (input.len() - consumed).min(GZIP_FEED_CHUNK);
            self.decoder
                .feed(&input[consumed..consumed + feed_size])
                .map_err(|e| CompressionError::InvalidData(e.to_string()))?;
            consumed += feed_size;

            produced += self.drain_into(&mut output[produced..]);
            if !self.decoder.output().is_empty() {
                // この feed の出力が output 容量を超えた: leftover を残して return
                return Ok(CompressionStatus::OutputFull { consumed, produced });
            }
        }

        // 3. 終端到達判定
        if self.decoder.is_finished() {
            return Ok(CompressionStatus::Complete { consumed, produced });
        }
        Ok(CompressionStatus::Continue { consumed, produced })
    }

    fn reset(&mut self) {
        self.decoder = noflate::gzip::Decoder::new();
    }
}

// ============================================================================
// Brotli 展開器 (BrotliDecompressStream のラップ)
// ============================================================================

type BrotliStateAlias = BrotliState<HeapAlloc<u8>, HeapAlloc<u32>, HeapAlloc<HuffmanCode>>;

/// Brotli ストリーミング展開器
///
/// `BrotliDecompressStream` の入出力 offset / available 形式を `Decompressor`
/// トレイトの `consumed` / `produced` 表現に変換する。
/// `total_out` は brotli 内部の累計出力カウンタで、状態保持のため struct に持たせる。
pub struct BrotliDecompressor {
    state: BrotliStateAlias,
    total_out: usize,
}

impl BrotliDecompressor {
    pub fn new() -> Self {
        Self {
            state: BrotliState::new(
                HeapAlloc::<u8>::new(0),
                HeapAlloc::<u32>::new(0),
                HeapAlloc::<HuffmanCode>::new(HuffmanCode::default()),
            ),
            total_out: 0,
        }
    }
}

impl Default for BrotliDecompressor {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Debug for BrotliDecompressor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BrotliDecompressor")
            .field("total_out", &self.total_out)
            .finish()
    }
}

impl Decompressor for BrotliDecompressor {
    fn decompress(
        &mut self,
        input: &[u8],
        output: &mut [u8],
    ) -> Result<CompressionStatus, CompressionError> {
        let mut available_in = input.len();
        let mut input_offset = 0usize;
        let mut available_out = output.len();
        let mut output_offset = 0usize;
        let mut total_out = self.total_out;

        let result = BrotliDecompressStream(
            &mut available_in,
            &mut input_offset,
            input,
            &mut available_out,
            &mut output_offset,
            output,
            &mut total_out,
            &mut self.state,
        );

        self.total_out = total_out;
        let consumed = input_offset;
        let produced = output_offset;

        match result {
            BrotliResult::ResultSuccess => Ok(CompressionStatus::Complete { consumed, produced }),
            BrotliResult::NeedsMoreInput => Ok(CompressionStatus::Continue { consumed, produced }),
            BrotliResult::NeedsMoreOutput => {
                Ok(CompressionStatus::OutputFull { consumed, produced })
            }
            BrotliResult::ResultFailure => Err(CompressionError::InvalidData(
                "brotli decoder reported failure".to_string(),
            )),
        }
    }

    fn reset(&mut self) {
        self.state = BrotliState::new(
            HeapAlloc::<u8>::new(0),
            HeapAlloc::<u32>::new(0),
            HeapAlloc::<HuffmanCode>::new(HuffmanCode::default()),
        );
        self.total_out = 0;
    }
}

// ============================================================================
// Zstandard 展開器 (zstd::stream::raw::Decoder のラップ)
// ============================================================================

/// Zstandard ストリーミング展開器
///
/// `zstd::stream::raw::Decoder::run_on_buffers` は (bytes_read, bytes_written,
/// remaining) を返す。`remaining == 0` がフレーム終端の合図。
pub struct ZstdDecompressor {
    decoder: zstd::stream::raw::Decoder<'static>,
}

impl ZstdDecompressor {
    pub fn new() -> Result<Self, CompressionError> {
        let decoder = zstd::stream::raw::Decoder::new()
            .map_err(|e| CompressionError::Internal(e.to_string()))?;
        Ok(Self { decoder })
    }
}

impl std::fmt::Debug for ZstdDecompressor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ZstdDecompressor").finish()
    }
}

impl Decompressor for ZstdDecompressor {
    fn decompress(
        &mut self,
        input: &[u8],
        output: &mut [u8],
    ) -> Result<CompressionStatus, CompressionError> {
        use zstd::stream::raw::Operation;

        let status = self
            .decoder
            .run_on_buffers(input, output)
            .map_err(|e| CompressionError::InvalidData(e.to_string()))?;
        let consumed = status.bytes_read;
        let produced = status.bytes_written;

        if status.remaining == 0 {
            return Ok(CompressionStatus::Complete { consumed, produced });
        }
        if produced == output.len() && consumed < input.len() {
            return Ok(CompressionStatus::OutputFull { consumed, produced });
        }
        Ok(CompressionStatus::Continue { consumed, produced })
    }

    fn reset(&mut self) {
        // zstd_safe::DCtx の初期化は実用上失敗しない (ヒープ確保失敗時のみ)。
        // Decompressor::reset は無戻り値の trait なので、失敗時は panic で
        // 異常状態を顕在化させ、サンプルのお手本としても意図を明示する。
        self.decoder = zstd::stream::raw::Decoder::new().expect("zstd decoder init must not fail");
    }
}

// ============================================================================
// AnyDecompressor: Content-Encoding に応じた切り替え
// ============================================================================

/// 受信時の Content-Encoding を見てから展開器を決定する用途向けの enum ラッパー
///
/// `None` variant は `shiguredo_http11::compression::NoCompression` をそのまま
/// 包んでおり identity (展開なし) として動作する。
///
/// `BrotliDecompressor` の内部 state (BrotliState) が ~2.5 KiB と大きいため、
/// variant 間サイズ差を抑える目的で `Box` でヒープに逃がしている。
#[derive(Debug)]
pub enum AnyDecompressor {
    /// 展開なし (identity / Content-Encoding ヘッダーなし)
    None(NoCompression),
    Gzip(Box<GzipDecompressor>),
    Brotli(Box<BrotliDecompressor>),
    Zstd(Box<ZstdDecompressor>),
}

impl AnyDecompressor {
    /// Content-Encoding 文字列から展開器を生成する
    ///
    /// `""` / `"identity"` は `None` (= `NoCompression`) を返す。
    /// 未知のエンコーディング (chained encoding `"gzip, br"` 等を含む) は
    /// `CompressionError::InvalidData` を返す。
    /// chained encoding に意味的に対応したい場合は呼び出し側でカンマ分割し
    /// 各 encoding について本関数を順に呼ぶこと。
    pub fn for_encoding(encoding: &str) -> Result<Self, CompressionError> {
        match encoding.trim().to_ascii_lowercase().as_str() {
            "" | "identity" => Ok(AnyDecompressor::None(NoCompression::new())),
            "gzip" | "x-gzip" => Ok(AnyDecompressor::Gzip(Box::default())),
            "br" => Ok(AnyDecompressor::Brotli(Box::default())),
            "zstd" => Ok(AnyDecompressor::Zstd(Box::new(ZstdDecompressor::new()?))),
            other => Err(CompressionError::InvalidData(format!(
                "unsupported Content-Encoding: {}",
                other
            ))),
        }
    }
}

impl Decompressor for AnyDecompressor {
    fn decompress(
        &mut self,
        input: &[u8],
        output: &mut [u8],
    ) -> Result<CompressionStatus, CompressionError> {
        match self {
            AnyDecompressor::None(d) => d.decompress(input, output),
            AnyDecompressor::Gzip(d) => d.decompress(input, output),
            AnyDecompressor::Brotli(d) => d.decompress(input, output),
            AnyDecompressor::Zstd(d) => d.decompress(input, output),
        }
    }

    fn reset(&mut self) {
        match self {
            AnyDecompressor::None(d) => Decompressor::reset(d),
            AnyDecompressor::Gzip(d) => d.reset(),
            AnyDecompressor::Brotli(d) => d.reset(),
            AnyDecompressor::Zstd(d) => d.reset(),
        }
    }
}

// ============================================================================
// Accept-Encoding ヘッダー値
// ============================================================================

/// クライアントがサポートしている展開形式を Accept-Encoding に載せる文字列で返す
pub fn supported_encodings() -> &'static str {
    "gzip, br, zstd"
}
