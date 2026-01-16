#![no_main]

use arbitrary::Arbitrary;
use libfuzzer_sys::fuzz_target;
use shiguredo_http11::{
    Request, RequestDecoder, Response, ResponseDecoder, encode_chunk, encode_chunks,
    encode_request_headers, encode_response_headers,
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
    for part in encoded.chunks(split_size) {
        if decoder.feed(part).is_err() {
            return None;
        }
        match decoder.decode() {
            Ok(Some(request)) => return Some(request.body),
            Ok(None) => {}
            Err(_) => return None,
        }
    }
    match decoder.decode() {
        Ok(Some(request)) => Some(request.body),
        _ => None,
    }
}

fn decode_response(encoded: &[u8], split_size: usize) -> Option<Vec<u8>> {
    let mut decoder = ResponseDecoder::new();
    for part in encoded.chunks(split_size) {
        if decoder.feed(part).is_err() {
            return None;
        }
        match decoder.decode() {
            Ok(Some(response)) => return Some(response.body),
            Ok(None) => {}
            Err(_) => return None,
        }
    }
    match decoder.decode() {
        Ok(Some(response)) => Some(response.body),
        _ => None,
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
