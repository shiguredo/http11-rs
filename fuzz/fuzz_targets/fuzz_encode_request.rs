//! `encode_request` の任意入力に対するパニック / abort 安全性を検証する
//!
//! 容量見積もり (`estimate_request_capacity`) が攻撃者制御のヘッダー値に対して
//! 過小確保 / オーバーフロー / OOM abort を引き起こさないことを担保する。
//! - 任意の method / uri / version / ヘッダー / ボディから `Request` を構築し、
//!   `encode_request` がパニック / abort せず必ず `Result` を返すこと
//! - 同じ Request を複数回 encode しても出力がバイト単位で等しいこと (決定性)

#![no_main]

use arbitrary::Arbitrary;
use libfuzzer_sys::fuzz_target;
use shiguredo_http11::{Request, encode_request};

#[derive(Arbitrary, Debug)]
struct FuzzRequest {
    method: String,
    uri: String,
    version: String,
    headers: Vec<(String, String)>,
    body_present: bool,
    body: Vec<u8>,
}

fuzz_target!(|input: FuzzRequest| {
    let FuzzRequest {
        method,
        uri,
        version,
        headers,
        body_present,
        body,
    } = input;
    let mut request = Request::with_version(&method, &uri, &version);
    for (name, value) in &headers {
        request.add_header(name, value);
    }
    request.body = if body_present { Some(body) } else { None };

    // 1 回目: パニック / abort しないこと
    let first = encode_request(&request);
    // 2 回目: 決定性 (同じ入力で同じ出力)
    let second = encode_request(&request);
    assert_eq!(first.is_ok(), second.is_ok());
    if let (Ok(a), Ok(b)) = (&first, &second) {
        assert_eq!(a, b);
    }
});
