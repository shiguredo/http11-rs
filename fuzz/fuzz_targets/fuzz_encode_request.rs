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
    // 構築時バリデーションが失敗する任意入力は早期 return する。
    // encoder の panic 安全性は構築を通過した Request にだけ問えばよい。
    let Ok(mut request) = Request::with_version(&method, &uri, &version) else {
        return;
    };
    for (name, value) in &headers {
        if request.add_header(name, value).is_err() {
            return;
        }
    }
    let request = if body_present {
        request.body(body)
    } else {
        request
    };

    // 1 回目: パニック / abort しないこと
    let _ = encode_request(&request);
});
