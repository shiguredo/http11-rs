//! `encode_request_headers` / `encode_response_headers` の panic 安全性を検証する
//!
//! 検証対象:
//! - 任意 method / uri / version / ヘッダー / status / reason から構築した
//!   `Request` / `Response` に対し `encode_*_headers` が必ず `Result` を返すこと

#![no_main]

use arbitrary::Arbitrary;
use libfuzzer_sys::fuzz_target;
use shiguredo_http11::{
    HeaderName, Method, Request, Response, encode_request_headers, encode_response_headers,
};

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
    let Ok(method) = Method::new(&request.method) else {
        return;
    };
    if let Ok(mut req) =
        Request::with_version(method, request.uri.as_str(), request.version.as_str())
    {
        for (name, value) in &request.headers {
            let Ok(header_name) = HeaderName::new(name) else {
                break;
            };
            if req.add_header(header_name, value.as_str()).is_err() {
                break;
            }
        }
        if let Ok(encoded) = encode_request_headers(&req) {
            let _ = encoded;
        }
    }

    // Response 側
    if let Ok(mut res) = Response::with_version(
        response.version.as_str(),
        response.status_code,
        response.reason_phrase.as_str(),
    ) {
        for (name, value) in &response.headers {
            let Ok(header_name) = HeaderName::new(name) else {
                break;
            };
            if res.add_header(header_name, value.as_str()).is_err() {
                break;
            }
        }
        if let Ok(encoded) = encode_response_headers(&res) {
            let _ = encoded;
        }
    }
});
