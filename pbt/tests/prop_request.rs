//! Request 構造体のプロパティテスト (request.rs)

use proptest::prelude::*;
use shiguredo_http11::{BodyKind, BodyProgress, EncodeError, HttpHead, Request, RequestDecoder};

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
//
// 注: 構築時バリデーション (Request::add_header / .header) を通過するため、
// VCHAR + SP + HTAB のみを生成する (obs-text は UTF-8 として扱われ、
// validate.rs の is_valid_field_value では受理されるが、本 strategy は
// ASCII safe な集合に限定する)。
//
// また先頭/末尾の SP は構築時バリデーションを通過するが、
// ヘッダー数の比較を簡単にするため空文字は許容する。
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

/// URI から Host ヘッダーの値を決定する
///
/// absolute-form の場合は URI の authority を返す。
/// それ以外の場合は "localhost" を返す。
fn host_for_uri(uri: &str) -> String {
    if uri.contains("://") {
        let after_scheme = uri.split("://").nth(1).unwrap_or("localhost");
        let end = after_scheme.find('/').unwrap_or(after_scheme.len());
        after_scheme[..end].to_string()
    } else {
        "localhost".to_string()
    }
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
        let mut request = Request::new(&method, &uri).unwrap();
        let host_value = host_for_uri(&uri);
        request.add_header("Host", &host_value).unwrap();
        for (name, value) in &hdrs {
            // Host ヘッダーの重複を避ける
            if !name.eq_ignore_ascii_case("Host") {
                request.add_header(name, value).unwrap();
            }
        }
        if !body_data.is_empty() {
            request = request.body(body_data.clone());
        }

        let encoded = request.encode().unwrap();

        let mut decoder = RequestDecoder::new();
        decoder.feed(&encoded).unwrap();
        let (head, body_kind) = decoder.decode_headers().unwrap().unwrap();

        prop_assert_eq!(head.method(), method.as_str());
        prop_assert_eq!(head.uri(), uri.as_str());

        let mut decoded_body = Vec::new();
        match body_kind {
            BodyKind::ContentLength(_) | BodyKind::Chunked => {
                while let Some(data) = decoder.peek_body() {
                    decoded_body.extend_from_slice(data);
                    let len = data.len();
                    match decoder.consume_body(len).unwrap() {
                        BodyProgress::Complete { .. } => break,
                        BodyProgress::Advanced | BodyProgress::NeedData => {}
                    }
                }
            }
            // リクエストでは CloseDelimited は使われない (RFC 9112)
            // Tunnel はレスポンスのみで発生 (CONNECT 2xx)
            BodyKind::CloseDelimited | BodyKind::None | BodyKind::Tunnel => {}
        }

        let expected_body: Vec<u8> = request
            .body_bytes()
            .map(<[u8]>::to_vec)
            .unwrap_or_default();
        prop_assert_eq!(decoded_body, expected_body);

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
        prop_assert_eq!(head.headers().len(), expected_header_count);
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
        let mut request = Request::new(&method, &uri).unwrap();
        let host_value = host_for_uri(&uri);
        request.add_header("Host", &host_value).unwrap();
        for (name, value) in &hdrs {
            if !name.eq_ignore_ascii_case("Host") {
                request.add_header(name, value).unwrap();
            }
        }

        let encoded = request.encode().unwrap();

        // 1バイトずつ feed
        let mut decoder = RequestDecoder::new();
        for byte in &encoded {
            decoder.feed(std::slice::from_ref(byte)).unwrap();
        }
        let (head, _) = decoder.decode_headers().unwrap().unwrap();

        prop_assert_eq!(head.method(), method.as_str());
        prop_assert_eq!(head.uri(), uri.as_str());
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
        let mut request = Request::new(&method, &uri).unwrap();
        let host_value = host_for_uri(&uri);
        request.add_header("Host", &host_value).unwrap();
        let request = request.body(body_data.clone());
        let encoded = request.encode().unwrap();

        // チャンクサイズで分割して feed し、デコード完了まで繰り返す
        let mut decoder = RequestDecoder::new();
        let mut headers_decoded = false;
        let mut body_kind = BodyKind::None;
        let mut decoded_body = Vec::new();
        let mut decoded_method = String::new();

        for chunk in encoded.chunks(7) {
            decoder.feed(chunk).unwrap();

            if !headers_decoded
                && let Ok(Some((head, kind))) = decoder.decode_headers()
            {
                headers_decoded = true;
                body_kind = kind;
                decoded_method = head.method().to_string();
            }

            if headers_decoded {
                match body_kind {
                    BodyKind::ContentLength(_) | BodyKind::Chunked => {
                        while let Some(data) = decoder.peek_body() {
                            decoded_body.extend_from_slice(data);
                            let len = data.len();
                            match decoder.consume_body(len).unwrap() {
                                BodyProgress::Complete { .. } => break,
                                BodyProgress::Advanced | BodyProgress::NeedData => {}
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
            let mut request = Request::new(&methods[i], &uris[i]).unwrap();
            let host_value = host_for_uri(&uris[i]);
            request.add_header("Host", &host_value).unwrap();
            let encoded = request.encode().unwrap();
            decoder.feed(&encoded).unwrap();
            let (head, _) = decoder.decode_headers().unwrap().unwrap();

            prop_assert_eq!(head.method(), methods[i].as_str());
            prop_assert_eq!(head.uri(), uris[i].as_str());
        }
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
        let mut request = Request::new(&method, &uri).unwrap();
        let host_value = host_for_uri(&uri);
        request.add_header("Host", &host_value).unwrap();
        let encoded = request.encode().unwrap();
        decoder.feed(&encoded).unwrap();
        let (head, _) = decoder.decode_headers().unwrap().unwrap();

        prop_assert_eq!(head.method(), method.as_str());
        prop_assert_eq!(head.uri(), uri.as_str());
    }
}

// ========================================
// Request API テスト
// ========================================

proptest! {
    #[test]
    fn prop_request_new_creates_valid_request(method in http_method(), uri in http_uri()) {
        let request = Request::new(&method, &uri).unwrap();

        prop_assert_eq!(request.method(), &method);
        prop_assert_eq!(request.uri(), &uri);
        prop_assert_eq!(request.version(), "HTTP/1.1");
        prop_assert!(HttpHead::headers(&request).is_empty());
        prop_assert!(request.body_bytes().is_none());
    }
}

proptest! {
    #[test]
    fn prop_request_with_version(method in http_method(), uri in http_uri()) {
        let request10 = Request::with_version(&method, &uri, "HTTP/1.0").unwrap();
        let request11 = Request::with_version(&method, &uri, "HTTP/1.1").unwrap();

        prop_assert_eq!(request10.version(), "HTTP/1.0");
        prop_assert_eq!(request11.version(), "HTTP/1.1");
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
        let request = Request::new(&method, &uri).unwrap().header(&name, &value).unwrap();

        let headers = HttpHead::headers(&request);
        prop_assert_eq!(headers.len(), 1);
        prop_assert_eq!(&headers[0].0, &name);
        prop_assert_eq!(&headers[0].1, &value);
    }
}

proptest! {
    #[test]
    fn prop_request_body_builder_pattern(
        method in http_method(),
        uri in http_uri(),
        body_data in body()
    ) {
        let request = Request::new(&method, &uri).unwrap().body(body_data.clone());

        prop_assert_eq!(request.body_bytes(), Some(body_data.as_slice()));
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
            .unwrap()
            .header("Content-Type", &value)
            .unwrap();

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
            .unwrap()
            .header("X-Custom", &value1)
            .unwrap()
            .header("x-custom", &value2)
            .unwrap();

        let values = request.get_headers("X-CUSTOM");
        prop_assert_eq!(values.len(), 2);
        prop_assert!(values.contains(&value1.as_str()));
        prop_assert!(values.contains(&value2.as_str()));
    }
}

// ========================================
// 構築時バリデーションの PBT
// ========================================

// CRLF を含む method は常に拒否される
proptest! {
    #[test]
    fn prop_request_rejects_method_with_crlf(
        prefix in token_string(8),
        infix in prop_oneof![Just("\r\n"), Just("\r"), Just("\n")],
        suffix in token_string(8),
    ) {
        let method = format!("{prefix}{infix}{suffix}");
        let result = Request::new(&method, "/");
        let is_invalid_method = matches!(result, Err(EncodeError::InvalidMethod { .. }));
        prop_assert!(is_invalid_method);
    }
}

// CRLF を含む URI は常に拒否される
proptest! {
    #[test]
    fn prop_request_rejects_uri_with_crlf(
        prefix in "/[a-zA-Z0-9/_.-]{1,16}",
        infix in prop_oneof![Just("\r\n"), Just("\r"), Just("\n")],
        suffix in "[a-zA-Z0-9/_.-]{1,16}",
    ) {
        let uri = format!("{prefix}{infix}{suffix}");
        let result = Request::new("GET", &uri);
        let is_invalid_target = matches!(result, Err(EncodeError::InvalidRequestTarget { .. }));
        prop_assert!(is_invalid_target);
    }
}

// ヘッダー値に CRLF が含まれていれば構築時に拒否される (smuggling 防御)
proptest! {
    #[test]
    fn prop_request_rejects_header_value_with_crlf(
        prefix in header_value(),
        infix in prop_oneof![Just("\r\n"), Just("\r"), Just("\n")],
        suffix in header_value(),
    ) {
        let req = Request::new("GET", "/").unwrap();
        let value = format!("{prefix}{infix}{suffix}");
        let result = req.header("X-Test", &value);
        let is_invalid_value = matches!(result, Err(EncodeError::InvalidHeaderValue { .. }));
        prop_assert!(is_invalid_value);
    }
}

// set_header のアトミック性: 不正な値で失敗しても既存ヘッダーは残る
proptest! {
    #[test]
    fn prop_request_set_header_atomicity_on_invalid_value(
        old_value in header_value(),
        new_value in header_value(),
    ) {
        let mut req = Request::new("GET", "/").unwrap();
        req.add_header("X-Test", &old_value).unwrap();
        // 不正な値で set_header 失敗
        let invalid = format!("{new_value}\r\nEvil: x");
        let result = req.set_header("X-Test", &invalid);
        let is_invalid_value = matches!(result, Err(EncodeError::InvalidHeaderValue { .. }));
        prop_assert!(is_invalid_value);
        // 既存ヘッダーが消えていない
        prop_assert_eq!(req.get_header("X-Test"), Some(old_value.as_str()));
    }
}
