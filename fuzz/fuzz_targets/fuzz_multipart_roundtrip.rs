//! `MultipartBuilder` で構築 → `MultipartParser` でデコード のラウンドトリップ
//! 安全性を検証する
//!
//! 既存 `fuzz_multipart` は decode 側のみ、`fuzz_multipart_boundary` は boundary
//! 任意化のみで、build → parse の往復は未実装。本 target は `MultipartBuilder` の
//! `text_field` / `file_field` / `part` で組み立てた multipart payload を
//! `MultipartParser` に流し、双方の API 契約が壊れないこと、有効ペイロードに
//! 対しては parser がエラーを返さないことを検証する。

#![no_main]

use arbitrary::Arbitrary;
use libfuzzer_sys::fuzz_target;
use shiguredo_http11::multipart::{MultipartBuilder, MultipartParser, Part};

#[derive(Arbitrary, Debug)]
enum FuzzPart {
    Text {
        name: String,
        value: String,
    },
    File {
        name: String,
        filename: String,
        content_type: String,
        body: Vec<u8>,
    },
    Raw {
        name: String,
        body: Vec<u8>,
    },
}

#[derive(Arbitrary, Debug)]
struct FuzzInput {
    /// boundary。`try_with_boundary` を通れば valid path、そうでなければ early return。
    boundary: String,
    parts: Vec<FuzzPart>,
    /// parser feed の分割サイズ (1..=64 で正規化)
    split_hint: u8,
}

/// 入力サイズを制限する (OOM 回避)
fn normalize_parts(parts: Vec<FuzzPart>) -> Vec<FuzzPart> {
    let mut total: usize = 0;
    parts
        .into_iter()
        .take(32)
        .filter_map(|part| {
            let part_size = match &part {
                FuzzPart::Text { name, value } => name.len() + value.len(),
                FuzzPart::File {
                    name,
                    filename,
                    content_type,
                    body,
                } => name.len() + filename.len() + content_type.len() + body.len(),
                FuzzPart::Raw { name, body } => name.len() + body.len(),
            };
            let next = total.checked_add(part_size)?;
            if next > 256 * 1024 {
                return None;
            }
            total = next;
            Some(part)
        })
        .collect()
}

fn build_payload(builder: MultipartBuilder, parts: &[FuzzPart]) -> Vec<u8> {
    let mut builder = builder;
    for part in parts {
        builder = match part {
            FuzzPart::Text { name, value } => builder.text_field(name, value),
            FuzzPart::File {
                name,
                filename,
                content_type,
                body,
            } => builder.file_field(name, filename, content_type, body),
            FuzzPart::Raw { name, body } => builder.part(Part::new(name).with_body(body)),
        };
    }
    builder.build()
}

fn drive_parser(parser: &mut MultipartParser, payload: &[u8], split_size: usize) {
    for chunk in payload.chunks(split_size) {
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
                }
                Ok(None) => break,
                Err(_) => return,
            }
        }
    }
}

fuzz_target!(|input: FuzzInput| {
    let FuzzInput {
        boundary,
        parts,
        split_hint,
    } = input;
    let parts = normalize_parts(parts);
    let split_size = ((split_hint as usize) % 64).max(1);

    // パターン 1: `try_with_boundary` を通った valid path
    if let Ok(builder) = MultipartBuilder::try_with_boundary(&boundary) {
        let payload = build_payload(builder, &parts);
        if let Ok(mut parser) = MultipartParser::try_new(&boundary) {
            drive_parser(&mut parser, &payload, split_size);
        }
    }

    // パターン 2: `with_boundary` (検証なし) で組み立て、`new` で parse する
    // attacker controlled boundary 経路。build 側 / parse 側どちらでも
    // パニックしないことを確認する。
    let builder = MultipartBuilder::with_boundary(&boundary);
    let payload = build_payload(builder, &parts);
    let mut parser = MultipartParser::new(&boundary);
    drive_parser(&mut parser, &payload, split_size);
});
