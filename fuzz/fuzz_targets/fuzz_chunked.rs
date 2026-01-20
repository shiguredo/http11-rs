#![no_main]

use arbitrary::Arbitrary;
use libfuzzer_sys::fuzz_target;
use shiguredo_http11::{
    encode_chunk, encode_chunks, encode_request_headers, encode_response_headers, BodyKind,
    BodyProgress, Request, RequestDecoder, Response, ResponseDecoder,
};

#[derive(Arbitrary, Debug)]
struct FuzzChunked {
    chunks: Vec<Vec<u8>>,
    split_hint: u8,
}

fn normalize_chunks(mut chunks: Vec<Vec<u8>>) -> Vec<Vec<u8>> {
    chunks.retain(|chunk| !chunk.is_empty());
    if chunks.len() > 64 {
        chunks.truncate(64);
    }
    chunks
}

fn concat_chunks(chunks: &[Vec<u8>]) -> Vec<u8> {
    let mut body = Vec::new();
    for chunk in chunks {
        body.extend_from_slice(chunk);
    }
    body
}

fn decode_request(encoded: &[u8], split_size: usize) -> Option<Vec<u8>> {
    let mut decoder = RequestDecoder::new();
    let mut decoded_body = Vec::new();
    let mut headers_done = false;
    let mut body_kind = BodyKind::None;

    for part in encoded.chunks(split_size) {
        if decoder.feed(part).is_err() {
            return None;
        }

        if !headers_done {
            match decoder.decode_headers() {
                Ok(Some((_, kind))) => {
                    headers_done = true;
                    body_kind = kind;
                }
                Ok(None) => continue,
                Err(_) => return None,
            }
        }

        match body_kind {
            BodyKind::ContentLength(_) | BodyKind::Chunked => loop {
                if let Some(body_data) = decoder.peek_body() {
                    decoded_body.extend_from_slice(body_data);
                    let len = body_data.len();
                    match decoder.consume_body(len) {
                        Ok(BodyProgress::Complete { .. }) => return Some(decoded_body),
                        Ok(BodyProgress::Continue) => {}
                        Err(_) => return None,
                    }
                } else {
                    // peek_body() が None でも consume_body(0) で状態遷移を試みる
                    match decoder.consume_body(0) {
                        Ok(BodyProgress::Complete { .. }) => return Some(decoded_body),
                        Ok(BodyProgress::Continue) => break, // 追加データが必要
                        Err(_) => return None,
                    }
                }
            },
            BodyKind::None => return Some(decoded_body),
        }
    }

    // 最後にもう一度チェック
    if !headers_done {
        match decoder.decode_headers() {
            Ok(Some((_, kind))) => {
                body_kind = kind;
            }
            Ok(None) => return None,
            Err(_) => return None,
        }
    }

    match body_kind {
        BodyKind::ContentLength(_) | BodyKind::Chunked => loop {
            if let Some(body_data) = decoder.peek_body() {
                decoded_body.extend_from_slice(body_data);
                let len = body_data.len();
                match decoder.consume_body(len) {
                    Ok(BodyProgress::Complete { .. }) => return Some(decoded_body),
                    Ok(BodyProgress::Continue) => {}
                    Err(_) => return None,
                }
            } else {
                match decoder.consume_body(0) {
                    Ok(BodyProgress::Complete { .. }) => return Some(decoded_body),
                    Ok(BodyProgress::Continue) => return None, // データ不足で不完全
                    Err(_) => return None,
                }
            }
        },
        BodyKind::None => Some(decoded_body),
    }
}

fn decode_response(encoded: &[u8], split_size: usize) -> Option<Vec<u8>> {
    let mut decoder = ResponseDecoder::new();
    let mut decoded_body = Vec::new();
    let mut headers_done = false;
    let mut body_kind = BodyKind::None;

    for part in encoded.chunks(split_size) {
        if decoder.feed(part).is_err() {
            return None;
        }

        if !headers_done {
            match decoder.decode_headers() {
                Ok(Some((_, kind))) => {
                    headers_done = true;
                    body_kind = kind;
                }
                Ok(None) => continue,
                Err(_) => return None,
            }
        }

        match body_kind {
            BodyKind::ContentLength(_) | BodyKind::Chunked => loop {
                if let Some(body_data) = decoder.peek_body() {
                    decoded_body.extend_from_slice(body_data);
                    let len = body_data.len();
                    match decoder.consume_body(len) {
                        Ok(BodyProgress::Complete { .. }) => return Some(decoded_body),
                        Ok(BodyProgress::Continue) => {}
                        Err(_) => return None,
                    }
                } else {
                    // peek_body() が None でも consume_body(0) で状態遷移を試みる
                    match decoder.consume_body(0) {
                        Ok(BodyProgress::Complete { .. }) => return Some(decoded_body),
                        Ok(BodyProgress::Continue) => break, // 追加データが必要
                        Err(_) => return None,
                    }
                }
            },
            BodyKind::None => return Some(decoded_body),
        }
    }

    // 最後にもう一度チェック
    if !headers_done {
        match decoder.decode_headers() {
            Ok(Some((_, kind))) => {
                body_kind = kind;
            }
            Ok(None) => return None,
            Err(_) => return None,
        }
    }

    match body_kind {
        BodyKind::ContentLength(_) | BodyKind::Chunked => loop {
            if let Some(body_data) = decoder.peek_body() {
                decoded_body.extend_from_slice(body_data);
                let len = body_data.len();
                match decoder.consume_body(len) {
                    Ok(BodyProgress::Complete { .. }) => return Some(decoded_body),
                    Ok(BodyProgress::Continue) => {}
                    Err(_) => return None,
                }
            } else {
                match decoder.consume_body(0) {
                    Ok(BodyProgress::Complete { .. }) => return Some(decoded_body),
                    Ok(BodyProgress::Continue) => return None, // データ不足で不完全
                    Err(_) => return None,
                }
            }
        },
        BodyKind::None => Some(decoded_body),
    }
}

fn exercise_request(body: &[u8], expected: &[u8], split_size: usize) {
    let mut request = Request::new("POST", "/");
    request.add_header("Transfer-Encoding", "chunked");
    let mut encoded = encode_request_headers(&request);
    encoded.extend_from_slice(body);

    if let Some(decoded_body) = decode_request(&encoded, split_size) {
        assert_eq!(decoded_body, expected);
    }
}

fn exercise_response(body: &[u8], expected: &[u8], split_size: usize) {
    let mut response = Response::new(200, "OK");
    response.add_header("Transfer-Encoding", "chunked");
    let mut encoded = encode_response_headers(&response);
    encoded.extend_from_slice(body);

    if let Some(decoded_body) = decode_response(&encoded, split_size) {
        assert_eq!(decoded_body, expected);
    }
}

fuzz_target!(|input: FuzzChunked| {
    let chunks = normalize_chunks(input.chunks);
    let expected = concat_chunks(&chunks);
    let split_size = (input.split_hint as usize % 32) + 1;

    let chunk_refs: Vec<&[u8]> = chunks.iter().map(|chunk| chunk.as_slice()).collect();
    let body_from_chunks = encode_chunks(&chunk_refs);

    let body_from_single = if chunks.is_empty() {
        encode_chunk(&[])
    } else {
        let mut body = Vec::new();
        for chunk in &chunks {
            body.extend_from_slice(&encode_chunk(chunk));
        }
        body.extend_from_slice(&encode_chunk(&[]));
        body
    };

    exercise_request(&body_from_chunks, &expected, split_size);
    exercise_request(&body_from_single, &expected, split_size);
    exercise_response(&body_from_chunks, &expected, split_size);
    exercise_response(&body_from_single, &expected, split_size);
});
