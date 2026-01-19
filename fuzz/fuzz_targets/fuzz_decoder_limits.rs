#![no_main]

use arbitrary::Arbitrary;
use libfuzzer_sys::fuzz_target;
use shiguredo_http11::{BodyKind, BodyProgress, DecoderLimits, RequestDecoder, ResponseDecoder};

#[derive(Arbitrary, Debug)]
struct FuzzLimits {
    max_buffer_size: u16,
    max_headers_count: u16,
    max_header_line_size: u16,
    max_body_size: u32,
    data: Vec<u8>,
}

fn build_limits(input: &FuzzLimits) -> DecoderLimits {
    DecoderLimits {
        max_buffer_size: input.max_buffer_size as usize,
        max_headers_count: input.max_headers_count as usize,
        max_header_line_size: input.max_header_line_size as usize,
        max_body_size: input.max_body_size as usize,
    }
}

fuzz_target!(|input: FuzzLimits| {
    let limits = build_limits(&input);

    let mut request_decoder = RequestDecoder::with_limits(limits.clone());
    let _ = request_decoder.feed(&input.data);
    if let Ok(Some((_, body_kind))) = request_decoder.decode_headers() {
        match body_kind {
            BodyKind::ContentLength(_) | BodyKind::Chunked => {
                while let Some(body_data) = request_decoder.peek_body() {
                    let len = body_data.len();
                    match request_decoder.consume_body(len) {
                        Ok(BodyProgress::Complete { .. }) => break,
                        Ok(BodyProgress::Continue) => {}
                        Err(_) => break,
                    }
                }
            }
            BodyKind::None => {}
        }
    }

    let mut response_decoder = ResponseDecoder::with_limits(limits);
    let _ = response_decoder.feed(&input.data);
    if let Ok(Some((_, body_kind))) = response_decoder.decode_headers() {
        match body_kind {
            BodyKind::ContentLength(_) | BodyKind::Chunked => {
                while let Some(body_data) = response_decoder.peek_body() {
                    let len = body_data.len();
                    match response_decoder.consume_body(len) {
                        Ok(BodyProgress::Complete { .. }) => break,
                        Ok(BodyProgress::Continue) => {}
                        Err(_) => break,
                    }
                }
            }
            BodyKind::None => {}
        }
    }
});
