//! 直接書き込み API (mut_buf / advance_buf / available_buf) のテスト
//!
//! `mut_buf(n)` でデコーダー内部バッファに直接書き込み、`advance_buf(written)` で
//! 書き込んだバイト数を確定する経路を検証する。
//! - 0 バイト要求 / 部分確定 / 二重 `mut_buf` の挙動
//! - バッファサイズ上限を超える `mut_buf` の安全性 (既存状態を保つ)
//! - pending 状態での `feed` 呼び出しの debug-assert
//! - `reset` による pending 解除
//! - `available_buf` の正確性 (max - 既存 - pending)
//! - `mut_buf` + `advance_buf` 経路で書き込んだヘッダーが `decode` でパースされること

use shiguredo_http11::{DecoderLimits, Error, RequestDecoder, ResponseDecoder};

#[test]
fn response_mut_buf_zero_returns_empty_slice() {
    let mut decoder = ResponseDecoder::new();
    let buf = decoder.mut_buf(0).unwrap();
    assert!(buf.is_empty());
    decoder.advance_buf(0);
    assert_eq!(decoder.remaining().len(), 0);
}

#[test]
fn request_mut_buf_zero_returns_empty_slice() {
    let mut decoder = RequestDecoder::new();
    let buf = decoder.mut_buf(0).unwrap();
    assert!(buf.is_empty());
    decoder.advance_buf(0);
    assert_eq!(decoder.remaining().len(), 0);
}

#[test]
fn response_advance_zero_drops_pending() {
    let mut decoder = ResponseDecoder::new();
    let _ = decoder.mut_buf(64).unwrap();
    decoder.advance_buf(0);
    // pending が破棄されてバッファは空のまま
    assert_eq!(decoder.remaining().len(), 0);
}

#[test]
fn request_advance_zero_drops_pending() {
    let mut decoder = RequestDecoder::new();
    let _ = decoder.mut_buf(64).unwrap();
    decoder.advance_buf(0);
    assert_eq!(decoder.remaining().len(), 0);
}

#[test]
fn response_advance_partial_drops_remainder() {
    let mut decoder = ResponseDecoder::new();
    let buf = decoder.mut_buf(16).unwrap();
    buf[..4].copy_from_slice(b"abcd");
    decoder.advance_buf(4);
    assert_eq!(decoder.remaining(), b"abcd");
}

#[test]
fn request_advance_partial_drops_remainder() {
    let mut decoder = RequestDecoder::new();
    let buf = decoder.mut_buf(16).unwrap();
    buf[..4].copy_from_slice(b"abcd");
    decoder.advance_buf(4);
    assert_eq!(decoder.remaining(), b"abcd");
}

#[test]
fn response_mut_buf_overflow_preserves_state() {
    let limits = DecoderLimits {
        max_buffer_size: 16,
        ..Default::default()
    };
    let mut decoder = ResponseDecoder::with_limits(limits);
    decoder.feed(b"hello").unwrap();
    let prev = decoder.remaining().to_vec();

    match decoder.mut_buf(100) {
        Err(Error::BufferOverflow { size, limit }) => {
            assert_eq!(size, 105);
            assert_eq!(limit, 16);
        }
        other => panic!("BufferOverflow を期待したが {:?} だった", other.is_ok()),
    }
    assert_eq!(decoder.remaining(), prev.as_slice());
}

#[test]
fn request_mut_buf_overflow_preserves_state() {
    let limits = DecoderLimits {
        max_buffer_size: 16,
        ..Default::default()
    };
    let mut decoder = RequestDecoder::with_limits(limits);
    decoder.feed(b"hello").unwrap();
    let prev = decoder.remaining().to_vec();

    match decoder.mut_buf(100) {
        Err(Error::BufferOverflow { size, limit }) => {
            assert_eq!(size, 105);
            assert_eq!(limit, 16);
        }
        other => panic!("BufferOverflow を期待したが {:?} だった", other.is_ok()),
    }
    assert_eq!(decoder.remaining(), prev.as_slice());
}

#[test]
fn response_consecutive_mut_buf_drops_previous_pending() {
    let mut decoder = ResponseDecoder::new();
    let buf = decoder.mut_buf(32).unwrap();
    buf[..3].copy_from_slice(b"foo");
    // advance_buf を呼ばずに 2 回目の mut_buf
    let buf2 = decoder.mut_buf(8).unwrap();
    assert_eq!(buf2.len(), 8);
    decoder.advance_buf(3);
    // 1 回目の pending は破棄される
    assert_eq!(decoder.remaining().len(), 3);
}

#[test]
fn request_consecutive_mut_buf_drops_previous_pending() {
    let mut decoder = RequestDecoder::new();
    let buf = decoder.mut_buf(32).unwrap();
    buf[..3].copy_from_slice(b"foo");
    let buf2 = decoder.mut_buf(8).unwrap();
    assert_eq!(buf2.len(), 8);
    decoder.advance_buf(3);
    assert_eq!(decoder.remaining().len(), 3);
}

#[test]
#[cfg(debug_assertions)]
#[should_panic(expected = "feed called with pending mut_buf")]
fn response_feed_with_pending_panics_in_debug() {
    let mut decoder = ResponseDecoder::new();
    let _ = decoder.mut_buf(8).unwrap();
    let _ = decoder.feed(b"x");
}

#[test]
#[cfg(debug_assertions)]
#[should_panic(expected = "feed called with pending mut_buf")]
fn request_feed_with_pending_panics_in_debug() {
    let mut decoder = RequestDecoder::new();
    let _ = decoder.mut_buf(8).unwrap();
    let _ = decoder.feed(b"x");
}

#[test]
fn response_reset_clears_pending() {
    let mut decoder = ResponseDecoder::new();
    let _ = decoder.mut_buf(64).unwrap();
    decoder.reset();
    // 再度 mut_buf を呼んでも問題なし
    let buf = decoder.mut_buf(8).unwrap();
    assert_eq!(buf.len(), 8);
    decoder.advance_buf(0);
}

#[test]
fn request_reset_clears_pending() {
    let mut decoder = RequestDecoder::new();
    let _ = decoder.mut_buf(64).unwrap();
    decoder.reset();
    let buf = decoder.mut_buf(8).unwrap();
    assert_eq!(buf.len(), 8);
    decoder.advance_buf(0);
}

#[test]
fn response_available_buf_default_is_max() {
    let decoder = ResponseDecoder::new();
    let max = decoder.limits().max_buffer_size;
    assert_eq!(decoder.available_buf(), max);
}

#[test]
fn request_available_buf_default_is_max() {
    let decoder = RequestDecoder::new();
    let max = decoder.limits().max_buffer_size;
    assert_eq!(decoder.available_buf(), max);
}

#[test]
fn response_available_buf_after_feed() {
    let mut decoder = ResponseDecoder::new();
    let max = decoder.limits().max_buffer_size;
    decoder.feed(b"hello").unwrap();
    assert_eq!(decoder.available_buf(), max - 5);
}

#[test]
fn request_available_buf_after_feed() {
    let mut decoder = RequestDecoder::new();
    let max = decoder.limits().max_buffer_size;
    decoder.feed(b"hello").unwrap();
    assert_eq!(decoder.available_buf(), max - 5);
}

#[test]
fn response_available_buf_after_mut_buf() {
    let mut decoder = ResponseDecoder::new();
    let max = decoder.limits().max_buffer_size;
    decoder.feed(b"abc").unwrap();
    let _ = decoder.mut_buf(7).unwrap();
    // pending を含めて差し引かれる (3 + 7 = 10)
    assert_eq!(decoder.available_buf(), max - 10);
    decoder.advance_buf(2);
}

#[test]
fn request_available_buf_after_mut_buf() {
    let mut decoder = RequestDecoder::new();
    let max = decoder.limits().max_buffer_size;
    decoder.feed(b"abc").unwrap();
    let _ = decoder.mut_buf(7).unwrap();
    assert_eq!(decoder.available_buf(), max - 10);
    decoder.advance_buf(2);
}

#[test]
fn response_available_buf_after_advance_partial() {
    let mut decoder = ResponseDecoder::new();
    let max = decoder.limits().max_buffer_size;
    decoder.feed(b"abc").unwrap();
    let _ = decoder.mut_buf(7).unwrap();
    decoder.advance_buf(4);
    // 3 + 4 = 7 が確定
    assert_eq!(decoder.available_buf(), max - 7);
}

#[test]
fn request_available_buf_after_advance_partial() {
    let mut decoder = RequestDecoder::new();
    let max = decoder.limits().max_buffer_size;
    decoder.feed(b"abc").unwrap();
    let _ = decoder.mut_buf(7).unwrap();
    decoder.advance_buf(4);
    assert_eq!(decoder.available_buf(), max - 7);
}

#[test]
fn response_available_buf_after_reset() {
    let mut decoder = ResponseDecoder::new();
    let max = decoder.limits().max_buffer_size;
    decoder.feed(b"abc").unwrap();
    decoder.reset();
    assert_eq!(decoder.available_buf(), max);
}

#[test]
fn request_available_buf_after_reset() {
    let mut decoder = RequestDecoder::new();
    let max = decoder.limits().max_buffer_size;
    decoder.feed(b"abc").unwrap();
    decoder.reset();
    assert_eq!(decoder.available_buf(), max);
}

/// `mut_buf` + `advance_buf` で送り込んだデータを `decode_headers` で
/// 正常にパースできることを確認する。
#[test]
fn response_mut_buf_decodes_headers() {
    let mut decoder = ResponseDecoder::new();
    let data = b"HTTP/1.1 200 OK\r\nContent-Length: 5\r\n\r\nhello";
    let buf = decoder.mut_buf(data.len()).unwrap();
    buf.copy_from_slice(data);
    decoder.advance_buf(data.len());
    let response = decoder
        .decode()
        .unwrap()
        .expect("response がデコードされるべき");
    assert_eq!(response.status_code(), 200);
    assert_eq!(response.body_bytes(), Some(b"hello".as_slice()));
}

#[test]
fn request_mut_buf_decodes_headers() {
    let mut decoder = RequestDecoder::new();
    let data = b"POST / HTTP/1.1\r\nHost: example.com\r\nContent-Length: 5\r\n\r\nhello";
    let buf = decoder.mut_buf(data.len()).unwrap();
    buf.copy_from_slice(data);
    decoder.advance_buf(data.len());
    let request = decoder
        .decode()
        .unwrap()
        .expect("request がデコードされるべき");
    assert_eq!(request.method(), "POST");
    assert_eq!(request.body_bytes(), Some(b"hello".as_slice()));
}
