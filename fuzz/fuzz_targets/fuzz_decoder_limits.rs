#![no_main]

use arbitrary::Arbitrary;
use libfuzzer_sys::fuzz_target;
use shiguredo_http11::{DecoderLimits, RequestDecoder, ResponseDecoder};

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
    let _ = request_decoder.decode();

    let mut response_decoder = ResponseDecoder::with_limits(limits);
    let _ = response_decoder.feed(&input.data);
    let _ = response_decoder.decode();
});
