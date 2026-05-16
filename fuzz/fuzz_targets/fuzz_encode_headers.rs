//! `encode_request_headers` / `encode_response_headers` の panic 安全性を検証する
//!
//! 検証対象:
//! - 任意 method / uri / version / ヘッダー / status / reason から構築した
//!   `Request` / `Response` に対し `encode_*_headers` が必ず `Result` を返すこと

#![no_main]

use arbitrary::Arbitrary;
use libfuzzer_sys::fuzz_target;
use shiguredo_http11::{Request, Response, encode_request_headers, encode_response_headers};

#[derive(Arbitrary, Debug)]
struct FuzzRequest {
    method: String,
    uri: String,
    version: String,
    headers: Vec<(String, String)>,
}

#[derive(Arbitrary, Debug)]
struct FuzzResponse {
    version: String,
    status_code: u16,
    reason_phrase: String,
    headers: Vec<(String, String)>,
}

#[derive(Arbitrary, Debug)]
struct FuzzInput {
    request: FuzzRequest,
    response: FuzzResponse,
}

fuzz_target!(|input: FuzzInput| {
    let FuzzInput { request, response } = input;

    // Request 側
    if let Ok(mut req) = Request::with_version(&request.method, &request.uri, &request.version) {
        for (name, value) in &request.headers {
            if req.add_header(name, value).is_err() {
                break;
            }
        }
        if let Ok(encoded) = encode_request_headers(&req) {
            let _ = encoded;
        }
    }

    // Response 側
    if let Ok(mut res) = Response::with_version(
        &response.version,
        response.status_code,
        &response.reason_phrase,
    ) {
        for (name, value) in &response.headers {
            if res.add_header(name, value).is_err() {
                break;
            }
        }
        if let Ok(encoded) = encode_response_headers(&res) {
            let _ = encoded;
        }
    }
});
