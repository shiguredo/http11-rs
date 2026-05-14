//! Request / Response のヘルパーメソッドの整合性を検証する
//!
//! - 任意の method, uri, version, ヘッダー, ボディから Request を構築し、
//!   get_header, get_headers, has_header, connection, content_length,
//!   is_chunked, is_keep_alive の各メソッドが期待値と一致することを確認する
//! - 同様に Response を構築し、上記に加えて status_class() による
//!   ステータスコード分類の整合性を検証する

#![no_main]

use arbitrary::Arbitrary;
use libfuzzer_sys::fuzz_target;
use shiguredo_http11::{Request, Response, StatusClass};

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

fn header_value<'a>(headers: &'a [(String, String)], name: &str) -> Option<&'a str> {
    headers
        .iter()
        .find(|(n, _)| n.eq_ignore_ascii_case(name))
        .map(|(_, v)| v.as_str())
}

fn header_count(headers: &[(String, String)], name: &str) -> usize {
    headers
        .iter()
        .filter(|(n, _)| n.eq_ignore_ascii_case(name))
        .count()
}

/// OWS (Optional Whitespace) を前後から除去 (RFC 9110 Section 5.6.3)
///
/// OWS = *( SP / HTAB )。`src/validate.rs::trim_ows` の参照実装。
/// `str::trim()` は NBSP (U+00A0) 等の Unicode 空白も除去するため、
/// HTTP Request Smuggling (CWE-444) 経路を避ける目的で SP / HTAB のみを除去する。
fn trim_ows(s: &str) -> &str {
    s.trim_matches(|c: char| c == ' ' || c == '\t')
}

/// `Request::content_length` / `Response::content_length` の参照実装。
///
/// decoder の `parse_content_length` (`src/decoder/body.rs`) と同じ厳格パース
/// (OWS / カンマリスト / 複数行 / mismatched 値の reject) を再現する。
fn expected_content_length(headers: &[(String, String)]) -> Result<Option<u64>, ()> {
    fn parse_strict_digits(s: &str) -> Result<u64, ()> {
        let trimmed = trim_ows(s);
        if trimmed.is_empty() || !trimmed.bytes().all(|b| b.is_ascii_digit()) {
            return Err(());
        }
        trimmed.parse::<u64>().map_err(|_| ())
    }
    fn parse_value(value: &str) -> Result<Option<u64>, ()> {
        let mut merged: Option<u64> = None;
        for elem in value.split(',') {
            let elem = trim_ows(elem);
            if elem.is_empty() {
                continue;
            }
            let n = parse_strict_digits(elem)?;
            match merged {
                None => merged = Some(n),
                Some(prev) if prev != n => return Err(()),
                Some(_) => {}
            }
        }
        Ok(merged)
    }

    let mut result: Option<u64> = None;
    for (name, value) in headers {
        if !name.eq_ignore_ascii_case("Content-Length") {
            continue;
        }
        let parsed = parse_value(value)?;
        if let Some(n) = parsed {
            match result {
                None => result = Some(n),
                Some(prev) if prev != n => return Err(()),
                Some(_) => {}
            }
        }
    }
    Ok(result)
}

fn expected_chunked(headers: &[(String, String)]) -> bool {
    let mut last_token: Option<&str> = None;
    for (name, value) in headers {
        if name.eq_ignore_ascii_case("Transfer-Encoding") {
            for token in value.split(',') {
                let token = trim_ows(token);
                if !token.is_empty() {
                    last_token = Some(token);
                }
            }
        }
    }
    last_token.is_some_and(|t| t.eq_ignore_ascii_case("chunked"))
}

fn expected_keep_alive(version: &str, headers: &[(String, String)]) -> bool {
    let mut has_keep_alive = false;
    for (name, value) in headers {
        if name.eq_ignore_ascii_case("Connection") {
            for token in value.split(',') {
                let token = trim_ows(token);
                if token.eq_ignore_ascii_case("close") {
                    return false;
                }
                if token.eq_ignore_ascii_case("keep-alive") {
                    has_keep_alive = true;
                }
            }
        }
    }
    if has_keep_alive {
        return true;
    }
    // HttpHead::is_keep_alive は HTTP/1.1 完全一致のみ persistent をデフォルトとする
    // (`RTSP/1.1` 等で誤判定を避けるため厳格化されている、src/decoder/head.rs)
    version == "HTTP/1.1"
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
        assert_eq!(request.get_header(name), header_value(&headers, name));
        assert_eq!(
            request.get_headers(name).len(),
            header_count(&headers, name)
        );
        assert_eq!(request.has_header(name), header_count(&headers, name) > 0);
    }

    assert_eq!(request.connection(), header_value(&headers, "Connection"));
    assert_eq!(
        request.content_length().map_err(|_| ()),
        expected_content_length(&headers)
    );
    assert_eq!(request.is_chunked(), expected_chunked(&headers));
    assert_eq!(
        request.is_keep_alive(),
        expected_keep_alive(&version, &headers)
    );
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
        assert_eq!(response.get_header(name), header_value(&headers, name));
        assert_eq!(
            response.get_headers(name).len(),
            header_count(&headers, name)
        );
        assert_eq!(response.has_header(name), header_count(&headers, name) > 0);
    }

    assert_eq!(response.connection(), header_value(&headers, "Connection"));
    assert_eq!(
        response.content_length().map_err(|_| ()),
        expected_content_length(&headers)
    );
    assert_eq!(response.is_chunked(), expected_chunked(&headers));
    assert_eq!(
        response.is_keep_alive(),
        expected_keep_alive(&version, &headers)
    );

    // Response::with_version はバリデーションを通すため、
    // 到達した時点で status_code は 100..=599 に閉じ込められている。
    assert_eq!(
        response.status_class() == StatusClass::Successful,
        (200..300).contains(&status_code)
    );
    assert_eq!(
        response.status_class() == StatusClass::Redirection,
        (300..400).contains(&status_code)
    );
    assert_eq!(
        response.status_class() == StatusClass::ClientError,
        (400..500).contains(&status_code)
    );
    assert_eq!(
        response.status_class() == StatusClass::ServerError,
        (500..600).contains(&status_code)
    );
    assert_eq!(
        response.status_class() == StatusClass::Informational,
        (100..200).contains(&status_code)
    );
}

fuzz_target!(|data: (FuzzRequest, FuzzResponse)| {
    let (request, response) = data;
    exercise_request(request);
    exercise_response(response);
});
