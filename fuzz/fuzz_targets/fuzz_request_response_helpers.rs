//! Request / Response のヘルパーメソッドの panic 安全性を検証する
//!
//! - 任意の method, uri, version, ヘッダー, ボディから Request を構築し、
//!   get_header, get_headers, has_header, connection, content_length,
//!   is_chunked, is_keep_alive の各メソッドがパニックしないことを確認する
//! - 同様に Response を構築し、上記に加えて status_class() のパニック安全性を検証する

#![no_main]

use arbitrary::Arbitrary;
use libfuzzer_sys::fuzz_target;
use shiguredo_http11::{Request, Response};

#[derive(Arbitrary, Debug)]
struct FuzzRequest {
    method: String,
    uri: String,
    version: String,
    headers: Vec<(String, String)>,
    body: Vec<u8>,
}

#[derive(Arbitrary, Debug)]
struct FuzzResponse {
    version: String,
    status_code: u16,
    reason_phrase: String,
    headers: Vec<(String, String)>,
    body: Vec<u8>,
}

fn exercise_request(input: FuzzRequest) {
    let FuzzRequest {
        method,
        uri,
        version,
        headers,
        body,
    } = input;
    let Ok(mut request) = Request::with_version(&method, &uri, &version) else {
        return;
    };
    for (name, value) in &headers {
        if request.add_header(name, value).is_err() {
            return;
        }
    }
    let request = request.body(body);

    for name in ["Connection", "Content-Length", "Transfer-Encoding"] {
        let _ = request.get_header(name);
        let _ = request.get_headers(name);
        let _ = request.has_header(name);
    }

    let _ = request.connection();
    let _ = request.content_length();
    let _ = request.is_chunked();
    let _ = request.is_keep_alive();
}

fn exercise_response(input: FuzzResponse) {
    let FuzzResponse {
        version,
        status_code,
        reason_phrase,
        headers,
        body,
    } = input;
    let Ok(mut response) = Response::with_version(&version, status_code, &reason_phrase) else {
        return;
    };
    for (name, value) in &headers {
        if response.add_header(name, value).is_err() {
            return;
        }
    }
    let response = response.body(body);

    for name in ["Connection", "Content-Length", "Transfer-Encoding"] {
        let _ = response.get_header(name);
        let _ = response.get_headers(name);
        let _ = response.has_header(name);
    }

    let _ = response.connection();
    let _ = response.content_length();
    let _ = response.is_chunked();
    let _ = response.is_keep_alive();
    let _ = response.status_class();
}

fuzz_target!(|data: (FuzzRequest, FuzzResponse)| {
    let (request, response) = data;
    exercise_request(request);
    exercise_response(response);
});
