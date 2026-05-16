//! 圧縮/展開トレイト (Sans I/O)
//!
//! RFC 9110 Section 8.4 (Content-Encoding) 準拠の圧縮/展開インターフェース。
//! gzip, deflate, br 等の圧縮アルゴリズムを実装する際のトレイト定義を提供する。

use alloc::string::String;
use core::fmt;

/// 圧縮/展開エラー
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum CompressionError {
    /// 出力バッファが小さすぎる
    BufferTooSmall { required: usize, available: usize },
    /// 入力データが不正
    InvalidData(String),
    /// 内部エラー
    Internal(String),
    /// 予期しない EOF
    UnexpectedEof,
    /// 既に完了している
    AlreadyFinished,
}

impl fmt::Display for CompressionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CompressionError::BufferTooSmall {
                required,
                available,
            } => {
                write!(
                    f,
                    "buffer too small: required {} bytes, available {} bytes",
                    required, available
                )
            }
            CompressionError::InvalidData(msg) => write!(f, "invalid data: {}", msg),
            CompressionError::Internal(msg) => write!(f, "internal error: {}", msg),
            CompressionError::UnexpectedEof => write!(f, "unexpected end of input"),
            CompressionError::AlreadyFinished => write!(f, "compression already finished"),
        }
    }
}

impl core::error::Error for CompressionError {}

/// 処理結果
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum CompressionStatus {
    /// 処理継続中
    Continue {
        /// 消費した入力バイト数
        consumed: usize,
        /// 生成した出力バイト数
        produced: usize,
    },
    /// 処理完了
    Complete {
        /// 消費した入力バイト数
        consumed: usize,
        /// 生成した出力バイト数
        produced: usize,
    },
    /// 出力バッファが満杯
    OutputFull {
        /// 消費した入力バイト数
        consumed: usize,
        /// 生成した出力バイト数
        produced: usize,
    },
}

impl CompressionStatus {
    /// 消費した入力バイト数を取得
    #[inline]
    pub fn consumed(&self) -> usize {
        match self {
            CompressionStatus::Continue { consumed, .. } => *consumed,
            CompressionStatus::Complete { consumed, .. } => *consumed,
            CompressionStatus::OutputFull { consumed, .. } => *consumed,
        }
    }

    /// 生成した出力バイト数を取得
    #[inline]
    pub fn produced(&self) -> usize {
        match self {
            CompressionStatus::Continue { produced, .. } => *produced,
            CompressionStatus::Complete { produced, .. } => *produced,
            CompressionStatus::OutputFull { produced, .. } => *produced,
        }
    }

    /// 処理が完了したかどうかを判定
    #[inline]
    pub fn is_complete(&self) -> bool {
        matches!(self, CompressionStatus::Complete { .. })
    }

    /// 出力バッファが満杯かどうかを判定
    #[inline]
    pub fn is_output_full(&self) -> bool {
        matches!(self, CompressionStatus::OutputFull { .. })
    }
}

/// 圧縮トレイト (Sans I/O)
///
/// # 使い方
///
/// ```ignore
/// let mut compressor = GzipCompressor::new();
/// let mut output = vec![0u8; 8192];
///
/// // 入力データを圧縮
/// let status = compressor.compress(input, &mut output)?;
/// // output[..status.produced()] に圧縮データ
///
/// // 圧縮を終了（残りのデータをフラッシュ）
/// let status = compressor.finish(&mut output)?;
/// // output[..status.produced()] に残りの圧縮データ
/// ```
pub trait Compressor {
    /// 入力データを圧縮して出力バッファに書き込む
    ///
    /// # 引数
    /// - `input`: 圧縮する入力データ
    /// - `output`: 圧縮データを書き込む出力バッファ
    ///
    /// # 戻り値
    /// - `Continue`: 処理継続中、入力がすべて消費された
    /// - `OutputFull`: 出力バッファが満杯、再度呼び出す必要あり
    /// - `Complete`: 処理完了（通常 `compress` では返らない）
    fn compress(
        &mut self,
        input: &[u8],
        output: &mut [u8],
    ) -> Result<CompressionStatus, CompressionError>;

    /// 圧縮を終了して残りのデータをフラッシュ
    ///
    /// # 引数
    /// - `output`: 残りの圧縮データを書き込む出力バッファ
    ///
    /// # 戻り値
    /// - `Complete`: 圧縮完了
    /// - `OutputFull`: 出力バッファが満杯、再度呼び出す必要あり
    fn finish(&mut self, output: &mut [u8]) -> Result<CompressionStatus, CompressionError>;

    /// 圧縮器をリセットして再利用可能にする
    fn reset(&mut self);
}

/// 展開トレイト (Sans I/O)
///
/// # 使い方
///
/// ```ignore
/// let mut decompressor = GzipDecompressor::new();
/// let mut output = vec![0u8; 8192];
/// let mut input = compressed.as_slice();
///
/// loop {
///     let status = decompressor.decompress(input, &mut output)?;
///     // output[..status.produced()] に展開データ
///     process(&output[..status.produced()]);
///     input = &input[status.consumed()..];
///
///     if status.is_complete() {
///         break;
///     }
///     // OutputFull なら output が小さい / Continue で input 空なら更なる入力が必要
/// }
/// ```
pub trait Decompressor {
    /// 圧縮データを展開して出力バッファに書き込む
    ///
    /// # 引数
    /// - `input`: 展開する圧縮データ
    /// - `output`: 展開データを書き込む出力バッファ
    ///
    /// # 戻り値
    /// - `Continue`: 処理継続中、さらに入力が必要
    /// - `OutputFull`: 出力バッファが満杯、再度呼び出す必要あり
    /// - `Complete`: 展開完了
    ///
    /// # 実装者向け要件
    ///
    /// `input` が空でもエラーを返してはならない。
    /// 空入力は内部 buffer を `output` へ drain したり、ストリーム終端を
    /// 報告したりする目的で呼び出されるためであり、`Continue` /
    /// `OutputFull` / `Complete` の何れか (`consumed = 0`) を返すこと。
    /// `ResponseDecoder::peek_body_decompressed` 等が body 枯渇後の
    /// drain のために空入力で本メソッドを呼び出す。
    fn decompress(
        &mut self,
        input: &[u8],
        output: &mut [u8],
    ) -> Result<CompressionStatus, CompressionError>;

    /// 展開器をリセットして再利用可能にする
    fn reset(&mut self);
}

/// 圧縮なし（デフォルト実装）
///
/// 入出力をそのままコピーする。Content-Encoding がない場合や
/// "identity" エンコーディングの場合に使用。
#[derive(Debug, Clone, Default)]
pub struct NoCompression {
    finished: bool,
}

impl NoCompression {
    /// 新しい NoCompression を作成
    pub fn new() -> Self {
        Self { finished: false }
    }
}

impl Compressor for NoCompression {
    fn compress(
        &mut self,
        input: &[u8],
        output: &mut [u8],
    ) -> Result<CompressionStatus, CompressionError> {
        if self.finished {
            return Err(CompressionError::AlreadyFinished);
        }

        let len = input.len().min(output.len());
        output[..len].copy_from_slice(&input[..len]);

        if len < input.len() {
            Ok(CompressionStatus::OutputFull {
                consumed: len,
                produced: len,
            })
        } else {
            Ok(CompressionStatus::Continue {
                consumed: len,
                produced: len,
            })
        }
    }

    fn finish(&mut self, _output: &mut [u8]) -> Result<CompressionStatus, CompressionError> {
        if self.finished {
            return Err(CompressionError::AlreadyFinished);
        }
        self.finished = true;
        Ok(CompressionStatus::Complete {
            consumed: 0,
            produced: 0,
        })
    }

    fn reset(&mut self) {
        self.finished = false;
    }
}

impl Decompressor for NoCompression {
    fn decompress(
        &mut self,
        input: &[u8],
        output: &mut [u8],
    ) -> Result<CompressionStatus, CompressionError> {
        let len = input.len().min(output.len());
        output[..len].copy_from_slice(&input[..len]);

        if len < input.len() {
            Ok(CompressionStatus::OutputFull {
                consumed: len,
                produced: len,
            })
        } else if input.is_empty() {
            Ok(CompressionStatus::Complete {
                consumed: 0,
                produced: 0,
            })
        } else {
            Ok(CompressionStatus::Continue {
                consumed: len,
                produced: len,
            })
        }
    }

    fn reset(&mut self) {
        self.finished = false;
    }
}
