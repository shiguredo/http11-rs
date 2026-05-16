//! `MultipartParser` の任意 boundary に対するパニック安全性を検証する
//!
//! 既存の `fuzz_multipart` は boundary が 4 種固定 (`"boundary"` 等) のため、
//! 攻撃者が `Content-Type: multipart/form-data; boundary=...` で制御し得る
//! boundary 文字列の経路 (`MultipartParser::try_new` の `InvalidBoundary` 判定、
//! delimiter 構築時の境界長 / 特殊文字、body と boundary の偶発衝突) を
//! 踏めていない。
//!
//! 本 target は boundary 文字列も任意化し、以下を検証する:
//! - `MultipartParser::new` / `try_new` が任意 boundary でパニックしないこと
//! - `with_max_buffer_size` 経路でも `feed` が制御範囲内で `BufferOverflow` を
//!   返し、パニックしないこと
//! - `next_part` の巡回 + アクセサ呼び出しがパニックしないこと

#![no_main]

use arbitrary::Arbitrary;
use libfuzzer_sys::fuzz_target;
use shiguredo_http11::multipart::MultipartParser;

#[derive(Arbitrary, Debug)]
struct FuzzInput {
    /// 攻撃者制御を想定した任意 boundary 文字列
    boundary: String,
    /// `with_max_buffer_size` に渡す上限値の元値 (OOM 回避のため 1MB で clamp する)
    max_buffer_size: u16,
    /// `feed` に投入するボディ
    data: Vec<u8>,
    /// データ分割サイズ (1..=64 で正規化)
    split_hint: u8,
}

fn drive(parser: &mut MultipartParser, data: &[u8], split_size: usize) {
    for chunk in data.chunks(split_size) {
        if parser.feed(chunk).is_err() {
            return;
        }
        loop {
            match parser.next_part() {
                Ok(Some(part)) => {
                    let _ = part.name();
                    let _ = part.filename();
                    let _ = part.content_type();
                    let _ = part.body();
                    let _ = part.body_str();
                    let _ = part.is_file();
                    let _ = part.headers();
                    let _ = part.content_disposition();
                }
                Ok(None) => break,
                Err(_) => return,
            }
        }
    }
    let _ = parser.is_finished();
}

fuzz_target!(|input: FuzzInput| {
    let FuzzInput {
        boundary,
        max_buffer_size,
        data,
        split_hint,
    } = input;
    let split_size = ((split_hint as usize) % 64).max(1);
    // OOM を避けるため上限を 1MB に clamp する
    let max_buffer_size = (max_buffer_size as usize).min(1024 * 1024);

    // パターン 1: `try_new` 経路 (RFC 2046 Section 5.1.1 検証あり)
    if let Ok(mut parser) = MultipartParser::try_new(&boundary) {
        parser = parser.with_max_buffer_size(max_buffer_size);
        drive(&mut parser, &data, split_size);
    }

    // パターン 2: `new` 経路 (検証なし、攻撃者が boundary に bare CTL や
    // RFC 2046 違反文字を埋め込む経路を再現する)
    let mut parser = MultipartParser::new(&boundary).with_max_buffer_size(max_buffer_size);
    drive(&mut parser, &data, split_size);
});
