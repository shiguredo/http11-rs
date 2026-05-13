//! Request / Response のエンコード → デコード ラウンドトリップを検証する
//!
//! - 任意の method, uri, ヘッダー, ボディから Request を構築し、
//!   encode() → RequestDecoder でデコードした結果が一致することを確認する
//! - 任意の status_code, reason_phrase, ヘッダー, ボディから Response を構築し、
//!   encode() → ResponseDecoder でデコードした結果が一致することを確認する

#![no_main]

use arbitrary::Arbitrary;
use libfuzzer_sys::fuzz_target;
use shiguredo_http11::{
    BodyKind, BodyProgress, Request, RequestDecoder, Response, ResponseDecoder,
};

#[derive(Arbitrary, Debug)]
struct FuzzRequest {
    method: String,
    uri: String,
    headers: Vec<(String, String)>,
    body: Vec<u8>,
}

#[derive(Arbitrary, Debug)]
struct FuzzResponse {
    status_code: u16,
    reason_phrase: String,
    headers: Vec<(String, String)>,
    body: Vec<u8>,
}

fn is_valid_token(s: &str) -> bool {
    !s.is_empty()
        && s.chars()
            .all(|c| c.is_ascii_alphanumeric() || "-_.~!$&'()*+,;=".contains(c))
}

fn is_valid_header_name(s: &str) -> bool {
    !s.is_empty()
        && s.chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

fn is_valid_header_value(s: &str) -> bool {
    s.chars().all(|c| c != '\r' && c != '\n')
}

fuzz_target!(|data: (FuzzRequest, FuzzResponse)| {
    let (fuzz_req, fuzz_resp) = data;

    // Request roundtrip
    if is_valid_token(&fuzz_req.method)
        && !fuzz_req.uri.is_empty()
        && !fuzz_req.uri.contains(' ')
        && !fuzz_req.uri.contains('\r')
        && !fuzz_req.uri.contains('\n')
    {
        let valid_headers: Vec<_> = fuzz_req
            .headers
            .iter()
            .filter(|(n, v)| is_valid_header_name(n) && is_valid_header_value(v))
            .cloned()
            .collect();

        let Ok(mut request) = Request::new(&fuzz_req.method, &fuzz_req.uri) else {
            return;
        };
        for (name, value) in &valid_headers {
            if request.add_header(name, value).is_err() {
                return;
            }
        }

        // ボディがある場合のみ設定
        let request = if !fuzz_req.body.is_empty() {
            request.body(fuzz_req.body.clone())
        } else {
            request
        };

        let encoded = match request.encode() {
            Ok(v) => v,
            Err(_) => return,
        };

        let mut decoder = RequestDecoder::new();
        if decoder.feed(&encoded).is_ok()
            && let Ok(Some((head, body_kind))) = decoder.decode_headers()
        {
            assert_eq!(head.method(), fuzz_req.method);
            assert_eq!(head.uri(), fuzz_req.uri);

            let mut decoded_body = Vec::new();
            match body_kind {
                BodyKind::ContentLength(_) | BodyKind::Chunked | BodyKind::CloseDelimited => {
                    while let Some(body_data) = decoder.peek_body() {
                        decoded_body.extend_from_slice(body_data);
                        let len = body_data.len();
                        match decoder.consume_body(len) {
                            Ok(BodyProgress::Complete { .. }) => break,
                            Ok(BodyProgress::Advanced | BodyProgress::NeedData) => {}
                            Err(_) => break,
                        }
                    }
                }
                BodyKind::None | BodyKind::Tunnel => {}
                _ => {}
            }
            let expected_body: Vec<u8> = request
                .body_bytes()
                .map(<[u8]>::to_vec)
                .unwrap_or_default();
            assert_eq!(decoded_body, expected_body);
        }
    }

    // Response roundtrip
    if fuzz_resp.status_code >= 100
        && fuzz_resp.status_code < 600
        && !fuzz_resp.reason_phrase.contains('\r')
        && !fuzz_resp.reason_phrase.contains('\n')
    {
        let valid_headers: Vec<_> = fuzz_resp
            .headers
            .iter()
            .filter(|(n, v)| is_valid_header_name(n) && is_valid_header_value(v))
            .cloned()
            .collect();

        let mut response = match Response::new(fuzz_resp.status_code, &fuzz_resp.reason_phrase) {
            Ok(r) => r,
            Err(_) => return,
        };
        for (name, value) in &valid_headers {
            if response.add_header(name, value).is_err() {
                return;
            }
        }

        // 1xx, 204, 304 以外の場合のみボディを設定
        let has_body = !((100..200).contains(&fuzz_resp.status_code)
            || fuzz_resp.status_code == 204
            || fuzz_resp.status_code == 304);

        let response_body = fuzz_resp.body.clone();
        if has_body && !response_body.is_empty() {
            response = response.body(response_body.clone());
        }

        let expected_body = if has_body && !response_body.is_empty() {
            response_body
        } else {
            Vec::new()
        };

        let encoded = match response.encode() {
            Ok(v) => v,
            Err(_) => return,
        };

        let mut decoder = ResponseDecoder::new();
        if decoder.feed(&encoded).is_ok()
            && let Ok(Some((head, body_kind))) = decoder.decode_headers()
        {
            assert_eq!(head.status_code(), fuzz_resp.status_code);
            assert_eq!(head.reason_phrase(), fuzz_resp.reason_phrase);

            let mut decoded_body = Vec::new();
            if has_body {
                match body_kind {
                    BodyKind::ContentLength(_) | BodyKind::Chunked | BodyKind::CloseDelimited => {
                        while let Some(body_data) = decoder.peek_body() {
                            decoded_body.extend_from_slice(body_data);
                            let len = body_data.len();
                            match decoder.consume_body(len) {
                                Ok(BodyProgress::Complete { .. }) => break,
                                Ok(BodyProgress::Advanced | BodyProgress::NeedData) => {}
                                Err(_) => break,
                            }
                        }
                    }
                    BodyKind::None | BodyKind::Tunnel => {}
                    _ => {}
                }
                assert_eq!(decoded_body, expected_body);
            }
        }
    }
});
