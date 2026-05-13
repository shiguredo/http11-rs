//! Keep-Alive / パイプライン化されたメッセージ列のデコード安全性を検証する
//!
//! HTTP/1.1 の Keep-Alive 接続では複数のリクエスト / レスポンスが
//! 単一のバイトストリームに連続して流れる。`reset()` でデコーダーを再利用し、
//! `take_remaining()` で次メッセージのデータを引き継ぐシナリオで、
//! 任意入力でパニックしないこと、連続デコードでも内部状態が壊れないことを確認する。
//!
//! 検証対象:
//! - `RequestDecoder` / `ResponseDecoder` の `reset()` + `take_remaining()` 経路
//! - 連続したメッセージ間で body 消費 → 次ヘッダー解析の遷移
//! - feed 分割 + パイプライン化の組み合わせ

#![no_main]

use arbitrary::Arbitrary;
use libfuzzer_sys::fuzz_target;
use shiguredo_http11::{BodyKind, BodyProgress, RequestDecoder, ResponseDecoder};

#[derive(Arbitrary, Debug)]
struct FuzzPipelined {
    /// 連続するメッセージのバイト列 (それぞれ別の HTTP メッセージ想定)
    messages: Vec<Vec<u8>>,
    /// feed 単位の分割サイズ (1..=64 で正規化)
    split_hint: u8,
}

fn drain_body_request(decoder: &mut RequestDecoder) {
    // peek_body と progress を交互に呼び、ボディを完全消費する。
    // パニック安全性が目的なので、エラーは early return せずに break する。
    loop {
        if let Some(data) = decoder.peek_body() {
            let len = data.len();
            match decoder.consume_body(len) {
                Ok(BodyProgress::Complete { .. }) => break,
                Ok(BodyProgress::Advanced | BodyProgress::NeedData) => continue,
                Err(_) => break,
            }
        }
        match decoder.progress() {
            Ok(BodyProgress::Complete { .. }) => break,
            Ok(BodyProgress::Advanced) => continue,
            Ok(BodyProgress::NeedData) | Err(_) => break,
        }
    }
}

fn drain_body_response(decoder: &mut ResponseDecoder) {
    loop {
        if let Some(data) = decoder.peek_body() {
            let len = data.len();
            match decoder.consume_body(len) {
                Ok(BodyProgress::Complete { .. }) => break,
                Ok(BodyProgress::Advanced | BodyProgress::NeedData) => continue,
                Err(_) => break,
            }
        }
        match decoder.progress() {
            Ok(BodyProgress::Complete { .. }) => break,
            Ok(BodyProgress::Advanced) => continue,
            Ok(BodyProgress::NeedData) | Err(_) => break,
        }
    }
}

fuzz_target!(|input: FuzzPipelined| {
    let FuzzPipelined {
        messages,
        split_hint,
    } = input;

    if messages.is_empty() {
        return;
    }
    let split_size = ((split_hint as usize) % 64).max(1);

    // パイプライン化リクエスト
    let mut request_decoder = RequestDecoder::new();
    for message in &messages {
        // 前回の残りデータ + 今回のメッセージを連結 (take_remaining 経路)
        let mut combined = request_decoder.take_remaining();
        combined.extend_from_slice(message);
        request_decoder.reset();

        for part in combined.chunks(split_size) {
            if request_decoder.feed(part).is_err() {
                break;
            }
        }
        // ヘッダー → ボディ消費
        if let Ok(Some((_, body_kind))) = request_decoder.decode_headers() {
            match body_kind {
                BodyKind::ContentLength(_) | BodyKind::Chunked => {
                    drain_body_request(&mut request_decoder);
                }
                BodyKind::None | BodyKind::Tunnel | BodyKind::CloseDelimited => {}
                _ => {}
            }
        }
    }

    // パイプライン化レスポンス
    let mut response_decoder = ResponseDecoder::new();
    for (i, message) in messages.iter().enumerate() {
        let mut combined = response_decoder.take_remaining();
        combined.extend_from_slice(message);
        response_decoder.reset();
        // 奇数番目は HEAD レスポンス想定にしておき set_request_method の経路もカバー
        if i % 2 == 1 {
            response_decoder.set_request_method("HEAD");
        }

        for part in combined.chunks(split_size) {
            if response_decoder.feed(part).is_err() {
                break;
            }
        }
        if let Ok(Some((_, body_kind))) = response_decoder.decode_headers() {
            match body_kind {
                BodyKind::ContentLength(_) | BodyKind::Chunked => {
                    drain_body_response(&mut response_decoder);
                }
                BodyKind::None | BodyKind::Tunnel | BodyKind::CloseDelimited => {}
                _ => {}
            }
        }
    }
});
