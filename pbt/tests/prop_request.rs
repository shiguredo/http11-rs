//! Request 構造体のプロパティテスト (request.rs)

use proptest::prelude::*;
use shiguredo_http11::{BodyKind, BodyProgress, Request, RequestDecoder};

// ========================================
// Strategy 定義
// ========================================

// HTTP トークン文字 (RFC 7230)
fn token_char() -> impl Strategy<Value = char> {
    prop_oneof![
        prop::char::range('a', 'z'),
        prop::char::range('A', 'Z'),
        prop::char::range('0', '9'),
        Just('-'),
        Just('_'),
        Just('.'),
    ]
}

fn token_string(max_len: usize) -> impl Strategy<Value = String> {
    proptest::collection::vec(token_char(), 1..=max_len)
        .prop_map(|chars| chars.into_iter().collect())
}

// HTTP ヘッダー名
fn header_name() -> impl Strategy<Value = String> {
    token_string(32)
}

// HTTP ヘッダー値 (RFC 9110 Section 5.5)
// field-vchar = VCHAR / obs-text
// VCHAR = %x21-7E, obs-text = %x80-FF, SP = 0x20, HTAB = 0x09
fn header_value_char() -> impl Strategy<Value = char> {
    prop_oneof![
        prop::char::range('!', '~'), // VCHAR: 0x21-0x7E
        Just(' '),                   // SP: 0x20
        Just('\t'),                  // HTAB: 0x09
    ]
}

fn header_value() -> impl Strategy<Value = String> {
    proptest::collection::vec(header_value_char(), 1..=64)
        .prop_map(|chars| chars.into_iter().collect())
}

// HTTP メソッド
fn http_method() -> impl Strategy<Value = String> {
    prop_oneof![
        Just("GET".to_string()),
        Just("POST".to_string()),
        Just("PUT".to_string()),
        Just("DELETE".to_string()),
        Just("HEAD".to_string()),
        Just("OPTIONS".to_string()),
        Just("PATCH".to_string()),
        // RTSP メソッド
        Just("DESCRIBE".to_string()),
        Just("SETUP".to_string()),
        Just("PLAY".to_string()),
        Just("PAUSE".to_string()),
        Just("TEARDOWN".to_string()),
    ]
}

// URI (スペースや CRLF を含まない)
fn http_uri() -> impl Strategy<Value = String> {
    prop_oneof![
        Just("/".to_string()),
        "/[a-zA-Z0-9/_.-]{1,64}".prop_map(|s| s),
        "rtsp://[a-z.]{1,32}/[a-z/]{1,32}".prop_map(|s| s),
    ]
}

// ヘッダーのリスト
fn headers() -> impl Strategy<Value = Vec<(String, String)>> {
    proptest::collection::vec((header_name(), header_value()), 0..10)
}

// ボディ
fn body() -> impl Strategy<Value = Vec<u8>> {
    proptest::collection::vec(any::<u8>(), 0..256)
}

// ========================================
// Request ラウンドトリップテスト
// ========================================

proptest! {
    #[test]
    fn prop_request_roundtrip(
        method in http_method(),
        uri in http_uri(),
        hdrs in headers(),
        body_data in body()
    ) {
        let mut request = Request::new(&method, &uri);
        request.add_header("Host", "localhost");
        for (name, value) in &hdrs {
            request.add_header(name, value);
        }
        if !body_data.is_empty() {
            request.body = body_data.clone();
        }

        let encoded = request.encode();

        let mut decoder = RequestDecoder::new();
        decoder.feed(&encoded).unwrap();
        let (head, body_kind) = decoder.decode_headers().unwrap().unwrap();

        prop_assert_eq!(&head.method, &method);
        prop_assert_eq!(&head.uri, &uri);

        let mut decoded_body = Vec::new();
        match body_kind {
            BodyKind::ContentLength(_) | BodyKind::Chunked => {
                while let Some(data) = decoder.peek_body() {
                    decoded_body.extend_from_slice(data);
                    let len = data.len();
                    match decoder.consume_body(len).unwrap() {
                        BodyProgress::Complete { .. } => break,
                        BodyProgress::Continue => {}
                    }
                }
            }
            // リクエストでは CloseDelimited は使われない (RFC 9112)
            // Tunnel はレスポンスのみで発生 (CONNECT 2xx)
            BodyKind::CloseDelimited | BodyKind::None | BodyKind::Tunnel => {}
        }

        prop_assert_eq!(&decoded_body, &request.body);

        // ヘッダー数は同じ (Content-Length が自動追加される可能性、Host は +1)
        let expected_header_count = if !body_data.is_empty()
            && !hdrs
                .iter()
                .any(|(n, _)| n.eq_ignore_ascii_case("Content-Length"))
        {
            hdrs.len() + 2  // Host + Content-Length
        } else {
            hdrs.len() + 1  // Host
        };
        prop_assert_eq!(head.headers.len(), expected_header_count);
    }
}

// ========================================
// ストリーミングデコードテスト
// ========================================

proptest! {
    #[test]
    fn prop_streaming_decode_request(
        method in http_method(),
        uri in http_uri(),
        hdrs in headers()
    ) {
        let mut request = Request::new(&method, &uri);
        request.add_header("Host", "localhost");
        for (name, value) in &hdrs {
            request.add_header(name, value);
        }

        let encoded = request.encode();

        // 1バイトずつ feed
        let mut decoder = RequestDecoder::new();
        for byte in &encoded {
            decoder.feed(std::slice::from_ref(byte)).unwrap();
        }
        let (head, _) = decoder.decode_headers().unwrap().unwrap();

        prop_assert_eq!(&head.method, &method);
        prop_assert_eq!(&head.uri, &uri);
    }
}

// ボディ付きリクエストのストリーミングデコード
proptest! {
    #[test]
    fn prop_streaming_decode_request_with_body(
        method in http_method(),
        uri in http_uri(),
        body_data in proptest::collection::vec(any::<u8>(), 1..128)
    ) {
        let mut request = Request::new(&method, &uri);
        request.add_header("Host", "localhost");
        request.body = body_data.clone();
        let encoded = request.encode();

        // チャンクサイズで分割して feed し、デコード完了まで繰り返す
        let mut decoder = RequestDecoder::new();
        let mut headers_decoded = false;
        let mut body_kind = BodyKind::None;
        let mut decoded_body = Vec::new();
        let mut decoded_method = String::new();

        for chunk in encoded.chunks(7) {
            decoder.feed(chunk).unwrap();

            if !headers_decoded {
                if let Ok(Some((head, kind))) = decoder.decode_headers() {
                    headers_decoded = true;
                    body_kind = kind;
                    decoded_method = head.method;
                }
            }

            if headers_decoded {
                match body_kind {
                    BodyKind::ContentLength(_) | BodyKind::Chunked => {
                        while let Some(data) = decoder.peek_body() {
                            decoded_body.extend_from_slice(data);
                            let len = data.len();
                            match decoder.consume_body(len).unwrap() {
                                BodyProgress::Complete { .. } => break,
                                BodyProgress::Continue => {}
                            }
                        }
                    }
                    // リクエストでは CloseDelimited は使われない (RFC 9112)
                    // Tunnel はレスポンスのみで発生 (CONNECT 2xx)
                    BodyKind::CloseDelimited | BodyKind::None | BodyKind::Tunnel => {}
                }
            }
        }

        prop_assert!(headers_decoded, "should decode headers");
        prop_assert_eq!(&decoded_method, &method);
        prop_assert_eq!(&decoded_body, &body_data);
    }
}

// ========================================
// Keep-Alive テスト
// ========================================

proptest! {
    #[test]
    fn prop_keep_alive_http11_default(method in http_method(), uri in http_uri()) {
        let request = Request::new(&method, &uri);
        prop_assert!(
            request.is_keep_alive(),
            "HTTP/1.1 should default to keep-alive"
        );

        let request_close = Request::new(&method, &uri).header("Connection", "close");
        prop_assert!(
            !request_close.is_keep_alive(),
            "Connection: close should disable keep-alive"
        );
    }
}

proptest! {
    #[test]
    fn prop_keep_alive_http10_default(method in http_method(), uri in http_uri()) {
        let request = Request::with_version(&method, &uri, "HTTP/1.0");
        prop_assert!(
            !request.is_keep_alive(),
            "HTTP/1.0 should default to close"
        );

        let request_keep =
            Request::with_version(&method, &uri, "HTTP/1.0").header("Connection", "keep-alive");
        prop_assert!(
            request_keep.is_keep_alive(),
            "Connection: keep-alive should enable keep-alive"
        );
    }
}

// ========================================
// 複数リクエストのデコードテスト
// ========================================

proptest! {
    #[test]
    fn prop_multiple_requests_same_decoder(
        methods in proptest::collection::vec(http_method(), 2..5),
        uris in proptest::collection::vec(http_uri(), 2..5)
    ) {
        let count = methods.len().min(uris.len());
        let mut decoder = RequestDecoder::new();

        for i in 0..count {
            if i > 0 {
                decoder.reset();
            }
            let mut request = Request::new(&methods[i], &uris[i]);
            request.add_header("Host", "localhost");
            let encoded = request.encode();
            decoder.feed(&encoded).unwrap();
            let (head, _) = decoder.decode_headers().unwrap().unwrap();

            prop_assert_eq!(&head.method, &methods[i]);
            prop_assert_eq!(&head.uri, &uris[i]);
        }
    }
}

// ========================================
// パニック安全性テスト
// ========================================

// 任意のバイト列を feed してもパニックしない
proptest! {
    #[test]
    fn prop_request_decoder_no_panic(data in proptest::collection::vec(any::<u8>(), 0..512)) {
        let mut decoder = RequestDecoder::new();
        let _ = decoder.feed(&data);
        let _ = decoder.decode_headers();
        // エラー後も再利用可能
        let _ = decoder.decode_headers();
    }
}

// エラー後にデコーダーをリセットして再利用できる
proptest! {
    #[test]
    fn prop_decoder_reuse_after_error(
        garbage in proptest::collection::vec(any::<u8>(), 1..64),
        method in http_method(),
        uri in http_uri()
    ) {
        let mut decoder = RequestDecoder::new();

        // 不正データを feed してエラーを発生させる
        let _ = decoder.feed(&garbage);
        let _ = decoder.decode_headers();

        // リセットして正常なリクエストをデコード
        decoder.reset();
        let mut request = Request::new(&method, &uri);
        request.add_header("Host", "localhost");
        let encoded = request.encode();
        decoder.feed(&encoded).unwrap();
        let (head, _) = decoder.decode_headers().unwrap().unwrap();

        prop_assert_eq!(&head.method, &method);
        prop_assert_eq!(&head.uri, &uri);
    }
}

// ========================================
// Request API テスト
// ========================================

proptest! {
    #[test]
    fn prop_request_new_creates_valid_request(method in http_method(), uri in http_uri()) {
        let request = Request::new(&method, &uri);

        prop_assert_eq!(&request.method, &method);
        prop_assert_eq!(&request.uri, &uri);
        prop_assert_eq!(&request.version, "HTTP/1.1");
        prop_assert!(request.headers.is_empty());
        prop_assert!(request.body.is_empty());
    }
}

proptest! {
    #[test]
    fn prop_request_with_version(method in http_method(), uri in http_uri()) {
        let request10 = Request::with_version(&method, &uri, "HTTP/1.0");
        let request11 = Request::with_version(&method, &uri, "HTTP/1.1");

        prop_assert_eq!(&request10.version, "HTTP/1.0");
        prop_assert_eq!(&request11.version, "HTTP/1.1");
    }
}

proptest! {
    #[test]
    fn prop_request_header_builder_pattern(
        method in http_method(),
        uri in http_uri(),
        name in header_name(),
        value in header_value()
    ) {
        let request = Request::new(&method, &uri).header(&name, &value);

        prop_assert_eq!(request.headers.len(), 1);
        prop_assert_eq!(&request.headers[0].0, &name);
        prop_assert_eq!(&request.headers[0].1, &value);
    }
}

proptest! {
    #[test]
    fn prop_request_body_builder_pattern(
        method in http_method(),
        uri in http_uri(),
        body_data in body()
    ) {
        let request = Request::new(&method, &uri).body(body_data.clone());

        prop_assert_eq!(&request.body, &body_data);
    }
}

proptest! {
    #[test]
    fn prop_request_get_header_case_insensitive(
        method in http_method(),
        uri in http_uri(),
        value in header_value()
    ) {
        let request = Request::new(&method, &uri)
            .header("Content-Type", &value);

        prop_assert_eq!(request.get_header("content-type"), Some(value.as_str()));
        prop_assert_eq!(request.get_header("CONTENT-TYPE"), Some(value.as_str()));
        prop_assert_eq!(request.get_header("Content-Type"), Some(value.as_str()));
    }
}

proptest! {
    #[test]
    fn prop_request_get_headers_case_insensitive_multiple(
        method in http_method(),
        uri in http_uri(),
        value1 in header_value(),
        value2 in header_value()
    ) {
        let request = Request::new(&method, &uri)
            .header("X-Custom", &value1)
            .header("x-custom", &value2);

        let values = request.get_headers("X-CUSTOM");
        prop_assert_eq!(values.len(), 2);
        prop_assert!(values.contains(&value1.as_str()));
        prop_assert!(values.contains(&value2.as_str()));
    }
}

// ========================================
// Clone と Debug テスト
// ========================================

proptest! {
    #[test]
    fn prop_request_clone_eq(
        method in http_method(),
        uri in http_uri(),
        hdrs in headers(),
        body_data in body()
    ) {
        let mut request = Request::new(&method, &uri);
        for (name, value) in &hdrs {
            request.add_header(name, value);
        }
        request.body = body_data;

        let cloned = request.clone();

        prop_assert_eq!(request.method, cloned.method);
        prop_assert_eq!(request.uri, cloned.uri);
        prop_assert_eq!(request.version, cloned.version);
        prop_assert_eq!(request.headers, cloned.headers);
        prop_assert_eq!(request.body, cloned.body);
    }
}

#[test]
fn prop_request_debug() {
    let request = Request::new("GET", "/test").header("Host", "example.com");
    let debug_str = format!("{:?}", request);

    assert!(debug_str.contains("GET"));
    assert!(debug_str.contains("/test"));
}
