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
    /// ボディ読み取り中 (CloseDelimited) - 接続が閉じるまで
    BodyCloseDelimited,
    /// トンネルモード (CONNECT 2xx レスポンス用)
    ///
    /// RFC 9112 Section 6.3: CONNECT メソッドへの 2xx レスポンスは
    /// トンネルモードに切り替わる。この状態では decode_headers() / decode() は使えない。
    /// take_remaining() でバッファ残りデータを取り出す必要がある。
    Tunnel,
    /// 完了
    Complete,
}
