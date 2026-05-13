//! ResponseDecoder の close-delimited ボディと mark_eof の挙動を検証する
//!
//! `BodyKind::CloseDelimited` は RFC 9112 で「接続が閉じるまでがボディ」と
//! 定義されており、`mark_eof()` 呼び出しで Complete に遷移する。
//! 任意のバイト列で以下の経路がパニックしないことを確認する:
//! - feed → decode_headers → ボディ消費 → mark_eof → 再度 peek_body
//! - feed の途中で mark_eof を呼ぶ (close-delimited 以外の状態)
//! - mark_eof 後にさらに feed を呼ぶ (pending=0 を満たす経路のみ)
//! - is_close_delimited() の状態確認

#![no_main]

use arbitrary::Arbitrary;
use libfuzzer_sys::fuzz_target;
use shiguredo_http11::{BodyKind, BodyProgress, ResponseDecoder};

#[derive(Arbitrary, Debug)]
struct FuzzInput<'a> {
    /// ヘッダー部とボディ部に分けて投入するバイト列
    header_bytes: &'a [u8],
    body_bytes: &'a [u8],
    /// body の分割サイズ (1..=32 で正規化)
    split_hint: u8,
}

fuzz_target!(|input: FuzzInput| {
    let FuzzInput {
        header_bytes,
        body_bytes,
        split_hint,
    } = input;
    let split_size = ((split_hint as usize) % 32).max(1);

    // パターン 1: ヘッダー投入 → decode → ボディ分割投入 → mark_eof
    let mut decoder = ResponseDecoder::new();
    if decoder.feed(header_bytes).is_err() {
        return;
    }
    let body_kind = match decoder.decode_headers() {
        Ok(Some((_, kind))) => kind,
        _ => {
            // ヘッダーがデコードできない場合でも mark_eof が安全であることだけ確認
            decoder.mark_eof();
            let _ = decoder.is_close_delimited();
            return;
        }
    };

    // close-delimited 以外の状態でも mark_eof が no-op として安全であることを確認
    if !matches!(body_kind, BodyKind::CloseDelimited) {
        decoder.mark_eof();
    }

    match body_kind {
        BodyKind::CloseDelimited => {
            // close-delimited: ボディを分割 feed しながら逐次消費
            for chunk in body_bytes.chunks(split_size) {
                if decoder.feed(chunk).is_err() {
                    break;
                }
                while let Some(data) = decoder.peek_body() {
                    let len = data.len();
                    if decoder.consume_body(len).is_err() {
                        break;
                    }
                }
            }
            // is_close_delimited は EOF 前は true を返す想定
            let _ = decoder.is_close_delimited();
            // 接続終了を通知
            decoder.mark_eof();
            // EOF 後の状態確認
            let _ = decoder.is_close_delimited();
            // EOF 後に peek_body を呼んでもパニックしないこと
            let _ = decoder.peek_body();
            let _ = decoder.progress();
        }
        BodyKind::ContentLength(_) | BodyKind::Chunked => {
            // close-delimited 以外でも分割 feed + ボディ消費でパニックしないこと
            for chunk in body_bytes.chunks(split_size) {
                if decoder.feed(chunk).is_err() {
                    break;
                }
                while let Some(data) = decoder.peek_body() {
                    let len = data.len();
                    match decoder.consume_body(len) {
                        Ok(BodyProgress::Complete { .. }) => break,
                        Ok(BodyProgress::Advanced | BodyProgress::NeedData) => continue,
                        Err(_) => break,
                    }
                }
            }
        }
        BodyKind::None | BodyKind::Tunnel => {}
        _ => {}
    }
});
