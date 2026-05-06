//! `encode_response` の任意入力に対するパニック / abort 安全性を検証する
//!
//! 容量見積もり (`estimate_response_capacity`) が攻撃者制御のヘッダー値に対して
//! 過小確保 / オーバーフロー / OOM abort を引き起こさないことを担保する。
//! - 任意の version / status_code / reason / ヘッダー / ボディから `Response` を構築し、
//!   `encode_response` がパニック / abort せず必ず `Result` を返すこと
//! - 同じ Response を複数回 encode しても出力がバイト単位で等しいこと (決定性)

#![no_main]

use arbitrary::Arbitrary;
use libfuzzer_sys::fuzz_target;
use shiguredo_http11::{Response, encode_response};

#[derive(Arbitrary, Debug)]
struct FuzzResponse {
    version: String,
    status_code: u16,
    reason_phrase: String,
    headers: Vec<(String, String)>,
    body_present: bool,
    body: Vec<u8>,
    omit_body: bool,
}

fuzz_target!(|input: FuzzResponse| {
    let FuzzResponse {
        version,
        status_code,
        reason_phrase,
        headers,
        body_present,
        body,
        omit_body,
    } = input;
    let mut response = Response::with_version(&version, status_code, &reason_phrase);
    for (name, value) in &headers {
        response.add_header(name, value);
    }
    response.body = if body_present { Some(body) } else { None };
    response.omit_body = omit_body;

    let first = encode_response(&response);
    let second = encode_response(&response);
    assert_eq!(first.is_ok(), second.is_ok());
    if let (Ok(a), Ok(b)) = (&first, &second) {
        assert_eq!(a, b);
    }
});
