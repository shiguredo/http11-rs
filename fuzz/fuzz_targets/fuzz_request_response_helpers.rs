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

fn expected_content_length(headers: &[(String, String)]) -> Option<usize> {
    header_value(headers, "Content-Length").and_then(|v| v.parse::<usize>().ok())
}

fn expected_chunked(headers: &[(String, String)]) -> bool {
    header_value(headers, "Transfer-Encoding")
        .is_some_and(|v| v.eq_ignore_ascii_case("chunked"))
}

fn expected_keep_alive(version: &str, connection: Option<&str>) -> bool {
    if let Some(conn) = connection {
        if conn.eq_ignore_ascii_case("close") {
            return false;
        }
        if conn.eq_ignore_ascii_case("keep-alive") {
            return true;
        }
    }
    version.ends_with("/1.1")
}

fn exercise_request(input: FuzzRequest) {
    let FuzzRequest {
        method,
        uri,
        version,
        headers,
        body,
    } = input;
    let mut request = Request::with_version(&method, &uri, &version);
    for (name, value) in &headers {
        request.add_header(name, value);
    }
    request.body = body;

    for name in ["Connection", "Content-Length", "Transfer-Encoding"] {
        assert_eq!(request.get_header(name), header_value(&headers, name));
        assert_eq!(
            request.get_headers(name).len(),
            header_count(&headers, name)
        );
        assert_eq!(request.has_header(name), header_count(&headers, name) > 0);
    }

    let connection = header_value(&headers, "Connection");
    assert_eq!(request.connection(), connection);
    assert_eq!(request.content_length(), expected_content_length(&headers));
    assert_eq!(request.is_chunked(), expected_chunked(&headers));
    assert_eq!(
        request.is_keep_alive(),
        expected_keep_alive(&version, connection)
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
    let mut response = Response::with_version(&version, status_code, &reason_phrase);
    for (name, value) in &headers {
        response.add_header(name, value);
    }
    response.body = body;

    for name in ["Connection", "Content-Length", "Transfer-Encoding"] {
        assert_eq!(response.get_header(name), header_value(&headers, name));
        assert_eq!(
            response.get_headers(name).len(),
            header_count(&headers, name)
        );
        assert_eq!(response.has_header(name), header_count(&headers, name) > 0);
    }

    let connection = header_value(&headers, "Connection");
    assert_eq!(response.connection(), connection);
    assert_eq!(response.content_length(), expected_content_length(&headers));
    assert_eq!(response.is_chunked(), expected_chunked(&headers));
    assert_eq!(
        response.is_keep_alive(),
        expected_keep_alive(&version, connection)
    );

    assert_eq!(response.is_success(), (200..300).contains(&status_code));
    assert_eq!(response.is_redirect(), (300..400).contains(&status_code));
    assert_eq!(response.is_client_error(), (400..500).contains(&status_code));
    assert_eq!(response.is_server_error(), (500..600).contains(&status_code));
    assert_eq!(
        response.is_informational(),
        (100..200).contains(&status_code)
    );
}

fuzz_target!(|data: (FuzzRequest, FuzzResponse)| {
    let (request, response) = data;
    exercise_request(request);
    exercise_response(response);
});
