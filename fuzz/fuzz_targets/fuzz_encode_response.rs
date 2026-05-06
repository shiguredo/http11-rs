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
    // バリデーション失敗は早期 return (fuzzer は次の入力に進める)
    let Ok(mut response) = Response::with_version(&version, status_code, &reason_phrase) else {
        return;
    };
    for (name, value) in &headers {
        if response.add_header(name, value).is_err() {
            return;
        }
    }
    // body_present=false のときは body() builder を呼ばず、Response の body を None のまま残す。
    // これにより fuzz は body=None / body=Some(...) の両パスをカバーする。
    let response = if body_present {
        response.body(body)
    } else {
        response
    };
    let response = response.omit_body(omit_body);

    let first = encode_response(&response);
    let second = encode_response(&response);
    assert_eq!(first.is_ok(), second.is_ok());
    if let (Ok(a), Ok(b)) = (&first, &second) {
        assert_eq!(a, b);
    }
});
