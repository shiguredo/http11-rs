//! 固定した上限のもとで任意入力をデコードし、デコード成功時に以下の不変条件が
//! 守られることを検証する:
//!
//! - 保持ヘッダー数は `max_headers_count` 以下
//! - 各ヘッダーの name+value バイト数は `max_header_line_size` 以下
//! - トレーラーも同様に `max_headers_count` 以下
//!
//! 上記が破れる入力が存在すればクラッシュとして検出される。

#![no_main]

use libfuzzer_sys::fuzz_target;
use shiguredo_http11::{
    BodyKind, BodyProgress, DecoderLimits, HeaderName, RequestDecoder, ResponseDecoder,
};

// 固定した小さい上限
const MAX_HEADERS_COUNT: usize = 32;
const MAX_HEADER_LINE_SIZE: usize = 256;

fn limits() -> DecoderLimits {
    DecoderLimits {
        max_buffer_size: 64 * 1024,
        max_headers_count: MAX_HEADERS_COUNT,
        max_header_line_size: MAX_HEADER_LINE_SIZE,
        max_body_size: 64 * 1024,
        max_chunk_line_size: 64,
    }
}

// デコード済みヘッダー (またはトレーラー) が上限内に収まることを検証する。
fn assert_headers_bounded(headers: &[(HeaderName, String)]) {
    assert!(
        headers.len() <= MAX_HEADERS_COUNT,
        "header count {} exceeds limit {}",
        headers.len(),
        MAX_HEADERS_COUNT
    );
    for (name, value) in headers {
        let size = name.as_str().len() + value.len();
        assert!(
            size <= MAX_HEADER_LINE_SIZE,
            "header line size {} exceeds limit {}",
            size,
            MAX_HEADER_LINE_SIZE
        );
    }
}

fn has_body(body_kind: &BodyKind) -> bool {
    matches!(
        body_kind,
        BodyKind::ContentLength(_) | BodyKind::Chunked | BodyKind::CloseDelimited
    )
}

fuzz_target!(|data: &[u8]| {
    // リクエストデコーダー
    let mut request = RequestDecoder::with_limits(limits());
    if request.feed(data).is_ok()
        && let Ok(Some((head, body_kind))) = request.decode_headers()
    {
        assert_headers_bounded(head.headers());
        if has_body(&body_kind) {
            loop {
                // peek の借用は `len` 取得直後に終わるため consume の可変借用と衝突しない
                let progress = if let Some(body_data) = request.peek_body() {
                    let len = body_data.len();
                    request.consume_body(len)
                } else {
                    request.progress()
                };
                match progress {
                    Ok(BodyProgress::Complete { trailers }) => {
                        assert_headers_bounded(&trailers);
                        break;
                    }
                    Ok(BodyProgress::Advanced) => {}
                    Ok(BodyProgress::NeedData) => break,
                    Err(_) => break,
                }
            }
        }
    }

    // レスポンスデコーダー
    let mut response = ResponseDecoder::with_limits(limits());
    if response.feed(data).is_ok()
        && let Ok(Some((head, body_kind))) = response.decode_headers()
    {
        assert_headers_bounded(head.headers());
        if has_body(&body_kind) {
            loop {
                let progress = if let Some(body_data) = response.peek_body() {
                    let len = body_data.len();
                    response.consume_body(len)
                } else {
                    response.progress()
                };
                match progress {
                    Ok(BodyProgress::Complete { trailers }) => {
                        assert_headers_bounded(&trailers);
                        break;
                    }
                    Ok(BodyProgress::Advanced) => {}
                    Ok(BodyProgress::NeedData) => break,
                    Err(_) => break,
                }
            }
        }
    }
});
