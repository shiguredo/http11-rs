//! デコード状態の定義

/// デコード状態
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum DecodePhase {
    /// スタートライン待ち
    StartLine,
    /// ヘッダー待ち
    Headers,
    /// ボディ読み取り中 (Content-Length)
    BodyContentLength { remaining: usize },
    /// ボディ読み取り中 (Chunked) - チャンクサイズ待ち
    BodyChunkedSize,
    /// ボディ読み取り中 (Chunked) - チャンクデータ待ち
    BodyChunkedData { remaining: usize },
    /// ボディ読み取り中 (Chunked) - チャンクデータ後の CRLF 待ち
    BodyChunkedDataCrlf,
    /// トレーラーヘッダー待ち
    ChunkedTrailer,
    /// 完了
    Complete,
}
