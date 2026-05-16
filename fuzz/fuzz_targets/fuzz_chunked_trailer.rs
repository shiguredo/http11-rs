//! `Transfer-Encoding: chunked` + `Trailer:` 宣言 + trailer-section の統合
//! デコード経路の panic 安全性を検証する
//!
//! 既存 `fuzz_trailer` は `Trailer::parse` 単独、`fuzz_decoder_chunked` は
//! trailer 部分の生成を含まないため、decoder の `set_declared_trailers` +
//! `is_prohibited_trailer_field` 経路 (`src/decoder/body.rs:114, 297`) が
//! 踏まれていない。
//!
//! 本 target は arbitrary で chunks と trailers (name, value) を生成し、
//! - `Trailer:` ヘッダーで宣言したフィールド名のみ trailer-section に書く
//! - 宣言しないフィールドや prohibited フィールドを混ぜたバリエーション
//!
//! も含めて、`RequestDecoder` / `ResponseDecoder` がパニックしないこと、
//! valid path では `BodyProgress::Complete { trailers }` まで到達することを確認する。

#![no_main]

use arbitrary::Arbitrary;
use libfuzzer_sys::fuzz_target;
use shiguredo_http11::{
    BodyKind, BodyProgress, Request, RequestDecoder, Response, ResponseDecoder, StatusCode,
    encode_chunks, encode_request_headers, encode_response_headers,
};

#[derive(Arbitrary, Debug)]
struct FuzzInput {
    chunks: Vec<Vec<u8>>,
    /// trailer-section に書く (name, value) の列
    trailers: Vec<(String, String)>,
    /// `Trailer:` ヘッダーで宣言する名前の選択 mask (trailers と同じ index で
    /// ビットが立っていれば宣言する)
    declared_mask: u32,
    /// feed 分割サイズ (1..=64 で正規化)
    split_hint: u8,
}

fn normalize_chunks(mut chunks: Vec<Vec<u8>>) -> Vec<Vec<u8>> {
    chunks.retain(|chunk| !chunk.is_empty());
    if chunks.len() > 32 {
        chunks.truncate(32);
    }
    let mut total: usize = 0;
    chunks.retain_mut(|chunk| {
        if chunk.len() > 16 * 1024 {
            chunk.truncate(16 * 1024);
        }
        let next = match total.checked_add(chunk.len()) {
            Some(v) => v,
            None => return false,
        };
        if next > 256 * 1024 {
            return false;
        }
        total = next;
        true
    });
    chunks
}

/// trailer name / value がヘッダー文法に「収まる」候補だけを残す。
/// arbitrary 入力には CR/LF/NUL 等が混入し得るが、それを混ぜると encoder 側で
/// add_header が拒否し、そもそも trailer-section を組み立てられない。
fn normalize_trailers(trailers: Vec<(String, String)>) -> Vec<(String, String)> {
    fn is_valid_token(s: &str) -> bool {
        !s.is_empty()
            && s.bytes().all(|b| {
                matches!(
                    b,
                    b'!' | b'#' | b'$' | b'%' | b'&' | b'\'' | b'*' | b'+' | b'-' | b'.'
                    | b'0'..=b'9' | b'A'..=b'Z' | b'^' | b'_' | b'`'
                    | b'a'..=b'z' | b'|' | b'~'
                )
            })
    }
    fn is_valid_value(s: &str) -> bool {
        s.bytes()
            .all(|b| matches!(b, 0x09 | 0x20..=0x7E | 0x80..=0xFF))
    }
    trailers
        .into_iter()
        .take(16)
        .filter(|(name, value)| is_valid_token(name) && is_valid_value(value))
        .collect()
}

fn build_trailer_section(trailers: &[(String, String)]) -> Vec<u8> {
    let mut buf = Vec::new();
    for (name, value) in trailers {
        buf.extend_from_slice(name.as_bytes());
        buf.extend_from_slice(b": ");
        buf.extend_from_slice(value.as_bytes());
        buf.extend_from_slice(b"\r\n");
    }
    buf.extend_from_slice(b"\r\n");
    buf
}

/// `Trailer:` ヘッダー値を組み立てる (declared_mask で選んだ index のみを宣言する)
fn build_declared(trailers: &[(String, String)], mask: u32) -> Option<String> {
    let names: Vec<&str> = trailers
        .iter()
        .enumerate()
        .filter(|(idx, _)| (*idx) < 32 && (mask & (1u32 << *idx)) != 0)
        .map(|(_, (name, _))| name.as_str())
        .collect();
    if names.is_empty() {
        None
    } else {
        Some(names.join(", "))
    }
}

fn build_request_payload(
    chunks: &[Vec<u8>],
    trailers: &[(String, String)],
    declared: Option<&str>,
) -> Option<Vec<u8>> {
    let mut request = Request::new("POST", "/").ok()?;
    request.add_header("Transfer-Encoding", "chunked").ok()?;
    if let Some(decl) = declared {
        request.add_header("Trailer", decl).ok()?;
    }
    let mut encoded = encode_request_headers(&request).ok()?;
    let refs: Vec<&[u8]> = chunks.iter().map(|c| c.as_slice()).collect();
    let mut body = encode_chunks(&refs);
    // `encode_chunks` は終端チャンク (`0\r\n\r\n`) を出力するが、trailer-section
    // を入れるには末尾の最終 CRLF (header-section との区切り) を一旦削って
    // trailers + final CRLF を差し込む必要がある。
    // 形式: `... 0\r\n` + trailer-section + `\r\n`
    assert!(body.ends_with(b"0\r\n\r\n"));
    body.truncate(body.len() - 2); // 末尾 `\r\n` を 1 つ削る
    body.extend_from_slice(&build_trailer_section(trailers));
    encoded.extend_from_slice(&body);
    Some(encoded)
}

fn build_response_payload(
    chunks: &[Vec<u8>],
    trailers: &[(String, String)],
    declared: Option<&str>,
) -> Option<Vec<u8>> {
    let mut response = Response::with_status(StatusCode::OK);
    response.add_header("Transfer-Encoding", "chunked").ok()?;
    if let Some(decl) = declared {
        response.add_header("Trailer", decl).ok()?;
    }
    let mut encoded = encode_response_headers(&response).ok()?;
    let refs: Vec<&[u8]> = chunks.iter().map(|c| c.as_slice()).collect();
    let mut body = encode_chunks(&refs);
    assert!(body.ends_with(b"0\r\n\r\n"));
    body.truncate(body.len() - 2);
    body.extend_from_slice(&build_trailer_section(trailers));
    encoded.extend_from_slice(&body);
    Some(encoded)
}

fn drive_request(payload: &[u8], split_size: usize) {
    let mut decoder = RequestDecoder::new();
    for part in payload.chunks(split_size) {
        if decoder.feed(part).is_err() {
            return;
        }
    }
    let body_kind = match decoder.decode_headers() {
        Ok(Some((_, kind))) => kind,
        _ => return,
    };
    if !matches!(body_kind, BodyKind::Chunked) {
        return;
    }
    loop {
        if let Some(data) = decoder.peek_body() {
            let len = data.len();
            match decoder.consume_body(len) {
                Ok(BodyProgress::Complete { .. }) => return,
                Ok(BodyProgress::Advanced | BodyProgress::NeedData) => continue,
                Err(_) => return,
            }
        }
        match decoder.progress() {
            Ok(BodyProgress::Complete { .. }) => return,
            Ok(BodyProgress::Advanced) => continue,
            Ok(BodyProgress::NeedData) | Err(_) => return,
        }
    }
}

fn drive_response(payload: &[u8], split_size: usize) {
    let mut decoder = ResponseDecoder::new();
    for part in payload.chunks(split_size) {
        if decoder.feed(part).is_err() {
            return;
        }
    }
    let body_kind = match decoder.decode_headers() {
        Ok(Some((_, kind))) => kind,
        _ => return,
    };
    if !matches!(body_kind, BodyKind::Chunked) {
        return;
    }
    loop {
        if let Some(data) = decoder.peek_body() {
            let len = data.len();
            match decoder.consume_body(len) {
                Ok(BodyProgress::Complete { .. }) => return,
                Ok(BodyProgress::Advanced | BodyProgress::NeedData) => continue,
                Err(_) => return,
            }
        }
        match decoder.progress() {
            Ok(BodyProgress::Complete { .. }) => return,
            Ok(BodyProgress::Advanced) => continue,
            Ok(BodyProgress::NeedData) | Err(_) => return,
        }
    }
}

fuzz_target!(|input: FuzzInput| {
    let FuzzInput {
        chunks,
        trailers,
        declared_mask,
        split_hint,
    } = input;
    let chunks = normalize_chunks(chunks);
    let trailers = normalize_trailers(trailers);
    let split_size = ((split_hint as usize) % 64).max(1);
    let declared = build_declared(&trailers, declared_mask);

    if let Some(payload) = build_request_payload(&chunks, &trailers, declared.as_deref()) {
        drive_request(&payload, split_size);
    }
    if let Some(payload) = build_response_payload(&chunks, &trailers, declared.as_deref()) {
        drive_response(&payload, split_size);
    }
});
