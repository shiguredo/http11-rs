//! `encode_request_headers` / `encode_response_headers` の panic / 決定性
//! 安全性を検証する
//!
//! 既存 `fuzz_encode_request` / `fuzz_encode_response` は `encode_request` /
//! `encode_response` (full encode) のみを叩いており、ボディ無しでヘッダー部分
//! だけをエンコードする `encode_*_headers` の経路は未到達。
//! `Transfer-Encoding: chunked` でストリーミング送信するシナリオではこの
//! 関数群が単独で呼ばれるため、容量見積もり / 決定性 / panic 安全性を独立に
//! 検証する。
//!
//! 検証対象:
//! - 任意 method / uri / version / ヘッダー / status / reason から構築した
//!   `Request` / `Response` に対し `encode_*_headers` が必ず `Result` を返すこと
//! - 同じ入力を 2 回 encode して結果が一致すること (決定性)

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
            // 決定性: 2 回呼んでバイト単位で一致すること
            let again = encode_request_headers(&req).expect("encode succeeded once");
            assert_eq!(encoded, again, "encode_request_headers must be deterministic");
            // ヘッダーセクションは必ず CRLF CRLF で終わる (RFC 9112 Section 2.1)
            assert!(
                encoded.ends_with(b"\r\n\r\n"),
                "encoded request headers must end with CRLF CRLF"
            );
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
            let again = encode_response_headers(&res).expect("encode succeeded once");
            assert_eq!(encoded, again, "encode_response_headers must be deterministic");
            assert!(
                encoded.ends_with(b"\r\n\r\n"),
                "encoded response headers must end with CRLF CRLF"
            );
        }
    }
});
