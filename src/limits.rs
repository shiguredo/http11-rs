/// デコーダーの制限設定
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DecoderLimits {
    /// 最大バッファサイズ (デフォルト: 64KB)
    pub max_buffer_size: usize,
    /// 最大ヘッダー数 (デフォルト: 100)
    pub max_headers_count: usize,
    /// 最大ヘッダー行長 (デフォルト: 8KB)
    pub max_header_line_size: usize,
    /// 最大ボディサイズ (デフォルト: 10MB)
    pub max_body_size: usize,
    /// 最大チャンクサイズ行長 (デフォルト: 64バイト)
    ///
    /// chunked 転送エンコーディングのチャンクサイズ行の最大長。
    /// チャンクサイズは 16 進数で表現されるため、通常は非常に短い。
    /// 例: "FFFFFFFF\r\n" (4GB) でも 10 バイト程度。
    pub max_chunk_line_size: usize,
}

impl Default for DecoderLimits {
    fn default() -> Self {
        Self {
            max_buffer_size: 64 * 1024, // 64KB
            max_headers_count: 100,
            max_header_line_size: 8 * 1024,  // 8KB
            max_body_size: 10 * 1024 * 1024, // 10MB
            max_chunk_line_size: 64,         // 64 bytes
        }
    }
}

impl DecoderLimits {
    /// 制限なしの設定を作成
    pub fn unlimited() -> Self {
        Self {
            max_buffer_size: usize::MAX,
            max_headers_count: usize::MAX,
            max_header_line_size: usize::MAX,
            max_body_size: usize::MAX,
            max_chunk_line_size: usize::MAX,
        }
    }
}
