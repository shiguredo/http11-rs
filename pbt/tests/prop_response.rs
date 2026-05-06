//! Response 構造体のプロパティテスト

use proptest::prelude::*;
use shiguredo_http11::{EncodeError, HttpHead, Response, ResponseDecoder, StatusClass, StatusCode};

// ========================================
// Strategy 定義
// ========================================

fn http_version() -> impl Strategy<Value = String> {
    prop_oneof![
        Just("HTTP/1.0".to_string()),
        Just("HTTP/1.1".to_string()),
        Just("RTSP/1.0".to_string()),
        Just("RTSP/2.0".to_string()),
    ]
}

fn status_code() -> impl Strategy<Value = u16> {
    prop_oneof![
        100u16..=101, // 1xx
        200u16..=206, // 2xx
        300u16..=308, // 3xx
        400u16..=451, // 4xx
        500u16..=511, // 5xx
    ]
}

// RFC 9110 Section 15 が許容する 100..=599 の全範囲を生成する Strategy
// (IANA 未登録の拡張 / 私的ステータスコードを含む全範囲をカバーする)
fn status_code_full_range() -> impl Strategy<Value = u16> {
    100u16..=599
}

// IANA 登録の StatusCode 定数を全網羅で選択する Strategy
fn iana_status_code() -> impl Strategy<Value = StatusCode> {
    proptest::sample::select(
        [
            // 1xx
            StatusCode::CONTINUE,
            StatusCode::SWITCHING_PROTOCOLS,
            StatusCode::PROCESSING,
            StatusCode::EARLY_HINTS,
            // 2xx
            StatusCode::OK,
            StatusCode::CREATED,
            StatusCode::ACCEPTED,
            StatusCode::NON_AUTHORITATIVE_INFORMATION,
            StatusCode::NO_CONTENT,
            StatusCode::RESET_CONTENT,
            StatusCode::PARTIAL_CONTENT,
            StatusCode::MULTI_STATUS,
            StatusCode::ALREADY_REPORTED,
            StatusCode::IM_USED,
            // 3xx
            StatusCode::MULTIPLE_CHOICES,
            StatusCode::MOVED_PERMANENTLY,
            StatusCode::FOUND,
            StatusCode::SEE_OTHER,
            StatusCode::NOT_MODIFIED,
            StatusCode::USE_PROXY,
            StatusCode::TEMPORARY_REDIRECT,
            StatusCode::PERMANENT_REDIRECT,
            // 4xx
            StatusCode::BAD_REQUEST,
            StatusCode::UNAUTHORIZED,
            StatusCode::PAYMENT_REQUIRED,
            StatusCode::FORBIDDEN,
            StatusCode::NOT_FOUND,
            StatusCode::METHOD_NOT_ALLOWED,
            StatusCode::NOT_ACCEPTABLE,
            StatusCode::PROXY_AUTHENTICATION_REQUIRED,
            StatusCode::REQUEST_TIMEOUT,
            StatusCode::CONFLICT,
            StatusCode::GONE,
            StatusCode::LENGTH_REQUIRED,
            StatusCode::PRECONDITION_FAILED,
            StatusCode::CONTENT_TOO_LARGE,
            StatusCode::URI_TOO_LONG,
            StatusCode::UNSUPPORTED_MEDIA_TYPE,
            StatusCode::RANGE_NOT_SATISFIABLE,
            StatusCode::EXPECTATION_FAILED,
            StatusCode::IM_A_TEAPOT,
            StatusCode::MISDIRECTED_REQUEST,
            StatusCode::UNPROCESSABLE_CONTENT,
            StatusCode::LOCKED,
            StatusCode::FAILED_DEPENDENCY,
            StatusCode::TOO_EARLY,
            StatusCode::UPGRADE_REQUIRED,
            StatusCode::PRECONDITION_REQUIRED,
            StatusCode::TOO_MANY_REQUESTS,
            StatusCode::REQUEST_HEADER_FIELDS_TOO_LARGE,
            StatusCode::UNAVAILABLE_FOR_LEGAL_REASONS,
            // 5xx
            StatusCode::INTERNAL_SERVER_ERROR,
            StatusCode::NOT_IMPLEMENTED,
            StatusCode::BAD_GATEWAY,
            StatusCode::SERVICE_UNAVAILABLE,
            StatusCode::GATEWAY_TIMEOUT,
            StatusCode::HTTP_VERSION_NOT_SUPPORTED,
            StatusCode::VARIANT_ALSO_NEGOTIATES,
            StatusCode::INSUFFICIENT_STORAGE,
            StatusCode::LOOP_DETECTED,
            StatusCode::NOT_EXTENDED,
            StatusCode::NETWORK_AUTHENTICATION_REQUIRED,
        ]
        .as_slice(),
    )
}

fn reason_phrase() -> impl Strategy<Value = String> {
    prop_oneof![
        Just("OK".to_string()),
        Just("Not Found".to_string()),
        Just("Internal Server Error".to_string()),
        Just("Bad Request".to_string()),
        "[A-Za-z ]{1,32}".prop_map(|s| s),
    ]
}

fn header_name() -> impl Strategy<Value = String> {
    "[A-Za-z][A-Za-z0-9-]{0,31}".prop_map(|s| s)
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

// ========================================
// コンストラクタのテスト
// ========================================

// with_status() はすべての IANA 登録 StatusCode 定数で infallible に Response を構築できる
proptest! {
    #[test]
    fn prop_response_with_status_constructs_infallibly(status in iana_status_code()) {
        let response = Response::with_status(status);

        // version は HTTP/1.1 固定
        prop_assert_eq!(HttpHead::version(&response), "HTTP/1.1");
        // status_code() は StatusCode::code() と一致
        prop_assert_eq!(response.status_code(), status.code());
        // reason_phrase() は StatusCode::canonical_reason() と一致
        prop_assert_eq!(response.reason_phrase(), status.canonical_reason());
        // 初期状態はヘッダーなし、ボディなし
        prop_assert!(HttpHead::headers(&response).is_empty());
        prop_assert!(response.body_bytes().is_none());
        prop_assert!(!response.is_body_omitted());
    }
}

// with_status() で構築した Response は encode → decode のラウンドトリップで
// status_code / reason_phrase / version が保存される
proptest! {
    #[test]
    fn prop_response_with_status_roundtrip(status in iana_status_code()) {
        let response = Response::with_status(status);
        let bytes = response.try_encode().unwrap();

        let mut decoder = ResponseDecoder::new();
        decoder.feed(&bytes).unwrap();
        let (head, _body_kind) = decoder
            .decode_headers()
            .unwrap()
            .expect("headers should be ready");

        prop_assert_eq!(head.status_code, status.code());
        prop_assert_eq!(&head.reason_phrase, status.canonical_reason());
        prop_assert_eq!(&head.version, "HTTP/1.1");
    }
}

// 任意の status_code (100..=599) で構築した Response の status_class() が
// StatusClass::from_status_code(status_code) と一致する
proptest! {
    #[test]
    fn prop_response_status_class(code in status_code_full_range()) {
        let response = Response::new(code, "OK").unwrap();
        let expected = StatusClass::from_status_code(code).expect("100..=599 always classified");
        prop_assert_eq!(response.status_class(), expected);
    }
}

// new() はデフォルトで HTTP/1.1
proptest! {
    #[test]
    fn prop_response_new_default_version(code in status_code(), phrase in reason_phrase()) {
        let response = Response::new(code, &phrase).unwrap();

        prop_assert_eq!(HttpHead::version(&response), "HTTP/1.1");
        prop_assert_eq!(response.status_code(), code);
        prop_assert_eq!(response.reason_phrase(), &phrase);
        prop_assert!(HttpHead::headers(&response).is_empty());
        prop_assert!(response.body_bytes().is_none());
        prop_assert!(!response.is_body_omitted());
    }
}

// with_version() でカスタムバージョン
proptest! {
    #[test]
    fn prop_response_with_version(version in http_version(), code in status_code(), phrase in reason_phrase()) {
        let response = Response::with_version(&version, code, &phrase).unwrap();

        prop_assert_eq!(HttpHead::version(&response), &version);
        prop_assert_eq!(response.status_code(), code);
        prop_assert_eq!(response.reason_phrase(), &phrase);
        prop_assert!(HttpHead::headers(&response).is_empty());
        prop_assert!(response.body_bytes().is_none());
        prop_assert!(!response.is_body_omitted());
    }
}

// ========================================
// ビルダーパターンのテスト
// ========================================

// header() ビルダー
proptest! {
    #[test]
    fn prop_response_header_builder(code in status_code(), name in header_name(), value in header_value()) {
        let response = Response::new(code, "OK").unwrap().header(&name, &value).unwrap();

        prop_assert_eq!(HttpHead::headers(&response).len(), 1);
        prop_assert_eq!(&HttpHead::headers(&response)[0].0, &name);
        prop_assert_eq!(&HttpHead::headers(&response)[0].1, &value);
    }
}

// 複数の header() チェーン
proptest! {
    #[test]
    fn prop_response_header_builder_chain(code in status_code(), headers in proptest::collection::vec((header_name(), header_value()), 1..5)) {
        let mut response = Response::new(code, "OK").unwrap();
        for (name, value) in &headers {
            response = response.header(name, value).unwrap();
        }

        prop_assert_eq!(HttpHead::headers(&response).len(), headers.len());
        for (i, (name, value)) in headers.iter().enumerate() {
            prop_assert_eq!(&HttpHead::headers(&response)[i].0, name);
            prop_assert_eq!(&HttpHead::headers(&response)[i].1, value);
        }
    }
}

// body() ビルダー
proptest! {
    #[test]
    fn prop_response_body_builder(code in status_code(), body_data in proptest::collection::vec(any::<u8>(), 0..256)) {
        let response = Response::new(code, "OK").unwrap().body(body_data.clone());

        prop_assert_eq!(response.body_bytes(), Some(body_data.as_slice()));
    }
}

// omit_body() ビルダー
proptest! {
    #[test]
    fn prop_response_omit_body_builder(code in status_code(), omit in any::<bool>()) {
        let response = Response::new(code, "OK").unwrap().omit_body(omit);
        prop_assert_eq!(response.is_body_omitted(), omit);
    }
}

// ========================================
// ヘッダー操作のテスト
// ========================================

// get_header() は大文字小文字を区別しない
proptest! {
    #[test]
    fn prop_response_get_header_case_insensitive(code in status_code(), value in header_value()) {
        let response = Response::new(code, "OK").unwrap()
            .header("Content-Type", &value).unwrap();

        prop_assert_eq!(response.get_header("Content-Type"), Some(value.as_str()));
        prop_assert_eq!(response.get_header("content-type"), Some(value.as_str()));
        prop_assert_eq!(response.get_header("CONTENT-TYPE"), Some(value.as_str()));
    }
}

// get_headers() は複数の同名ヘッダーをすべて取得
proptest! {
    #[test]
    fn prop_response_get_headers_multiple(code in status_code(), values in proptest::collection::vec(header_value(), 1..5)) {
        let mut response = Response::new(code, "OK").unwrap();
        for value in &values {
            response = response.header("Set-Cookie", value).unwrap();
        }

        let headers = response.get_headers("Set-Cookie");
        prop_assert_eq!(headers.len(), values.len());
        for (i, value) in values.iter().enumerate() {
            prop_assert_eq!(headers[i], value.as_str());
        }
    }
}

// get_headers() は大文字小文字を区別しない
proptest! {
    #[test]
    fn prop_response_get_headers_case_insensitive(code in status_code(), value in header_value()) {
        let response = Response::new(code, "OK").unwrap()
            .header("Set-Cookie", &value).unwrap();

        prop_assert_eq!(response.get_headers("set-cookie").len(), 1);
        prop_assert_eq!(response.get_headers("SET-COOKIE").len(), 1);
    }
}

// has_header() の動作確認
proptest! {
    #[test]
    fn prop_response_has_header(code in status_code(), name in header_name(), value in header_value()) {
        let response = Response::new(code, "OK").unwrap().header(&name, &value).unwrap();

        prop_assert!(response.has_header(&name));
        prop_assert!(response.has_header(&name.to_lowercase()));
        prop_assert!(response.has_header(&name.to_uppercase()));
        prop_assert!(!response.has_header("X-Not-Exists"));
    }
}

// ========================================
// Connection と Keep-Alive のテスト
// ========================================

// connection() はヘッダー値を返す
proptest! {
    #[test]
    fn prop_response_connection_header(code in status_code(), conn_value in prop_oneof![Just("keep-alive"), Just("close"), Just("Keep-Alive"), Just("Close")]) {
        let response = Response::new(code, "OK").unwrap().header("Connection", conn_value).unwrap();

        prop_assert_eq!(response.connection(), Some(conn_value));
    }
}

// ========================================
// Content-Length と Transfer-Encoding のテスト
// ========================================

// content_length() は数値を返す
proptest! {
    #[test]
    fn prop_response_content_length(code in status_code(), len in 0usize..1_000_000) {
        let response = Response::new(code, "OK").unwrap()
            .header("Content-Length", len.to_string()).unwrap();

        prop_assert_eq!(response.content_length(), Some(len as u64));
    }
}

// ========================================
// バリデーションのテスト
// ========================================

// 不正な status_code は構築時に拒否される
proptest! {
    #[test]
    fn prop_response_invalid_status_code(
        code in prop_oneof![0u16..100, 600u16..=u16::MAX],
        phrase in reason_phrase(),
    ) {
        let result = Response::new(code, &phrase);
        let is_invalid_status = matches!(result, Err(EncodeError::InvalidStatusCode { .. }));
        prop_assert!(is_invalid_status);
    }
}

// 制御文字を含む reason_phrase は構築時に拒否される
fn invalid_reason_phrase_char() -> impl Strategy<Value = char> {
    prop_oneof![
        // 制御文字 0x00-0x08, 0x0A-0x1F, 0x7F
        prop::char::range('\u{0}', '\u{8}'),
        prop::char::range('\u{A}', '\u{1F}'),
        Just('\u{7F}'),
    ]
}

proptest! {
    #[test]
    fn prop_response_invalid_reason_phrase(
        code in status_code(),
        bad_char in invalid_reason_phrase_char(),
    ) {
        let phrase = format!("OK{bad_char}bad");
        let result = Response::new(code, &phrase);
        let is_invalid = matches!(result, Err(EncodeError::InvalidReasonPhrase { .. }));
        prop_assert!(is_invalid);
    }
}

// 空の reason_phrase も構築時に拒否される
proptest! {
    #[test]
    fn prop_response_empty_reason_phrase_rejected(code in status_code()) {
        let result = Response::new(code, "");
        let is_invalid = matches!(result, Err(EncodeError::InvalidReasonPhrase { .. }));
        prop_assert!(is_invalid);
    }
}

// 不正な version は構築時に拒否される
fn invalid_version() -> impl Strategy<Value = String> {
    prop_oneof![
        Just(String::new()),
        Just("garbage".to_string()),
        Just("HTTP /1.1".to_string()),
        Just("HTTP/1.1\r\nX: y".to_string()),
        Just("HTTP/abc.def".to_string()),
    ]
}

proptest! {
    #[test]
    fn prop_response_invalid_version(
        version in invalid_version(),
        code in status_code(),
        phrase in reason_phrase(),
    ) {
        let result = Response::with_version(&version, code, &phrase);
        let is_invalid = matches!(result, Err(EncodeError::InvalidVersion { .. }));
        prop_assert!(is_invalid);
    }
}

// 不正なヘッダー名は add_header / header で拒否される
fn invalid_header_name() -> impl Strategy<Value = String> {
    prop_oneof![
        Just(String::new()),
        Just("Bad Name".to_string()),
        Just("Bad\r\nName".to_string()),
        Just("Bad\0Name".to_string()),
        Just("Bad:Name".to_string()),
    ]
}

proptest! {
    #[test]
    fn prop_response_invalid_header_name(
        code in status_code(),
        name in invalid_header_name(),
        value in header_value(),
    ) {
        let mut response = Response::new(code, "OK").unwrap();
        let result = response.add_header(&name, &value);
        let is_invalid = matches!(result, Err(EncodeError::InvalidHeaderName { .. }));
        prop_assert!(is_invalid);
    }
}

// 不正なヘッダー値は add_header / header で拒否される
fn invalid_header_value_char() -> impl Strategy<Value = char> {
    prop_oneof![
        prop::char::range('\u{0}', '\u{8}'),
        prop::char::range('\u{A}', '\u{1F}'),
        Just('\u{7F}'),
    ]
}

proptest! {
    #[test]
    fn prop_response_invalid_header_value(
        code in status_code(),
        name in header_name(),
        bad_char in invalid_header_value_char(),
    ) {
        let value = format!("good{bad_char}bad");
        let mut response = Response::new(code, "OK").unwrap();
        let result = response.add_header(&name, &value);
        let is_invalid = matches!(result, Err(EncodeError::InvalidHeaderValue { .. }));
        prop_assert!(is_invalid);
    }
}

// ========================================
// 0021: mutator (set_body / clear_body / without_body / set_omit_body / チェイン) の PBT
// ========================================

// set_body → body_bytes() のラウンドトリップ
proptest! {
    #[test]
    fn prop_response_set_body_roundtrip(
        code in status_code(),
        body_data in proptest::collection::vec(any::<u8>(), 0..256),
    ) {
        let mut response = Response::new(code, "OK").unwrap();
        response.set_body(body_data.clone());
        prop_assert_eq!(response.body_bytes(), Some(body_data.as_slice()));
    }
}

// set_body → clear_body で body が None になる
proptest! {
    #[test]
    fn prop_response_set_then_clear_body(
        code in status_code(),
        body_data in proptest::collection::vec(any::<u8>(), 0..256),
    ) {
        let mut response = Response::new(code, "OK").unwrap();
        response.set_body(body_data);
        response.clear_body();
        prop_assert!(response.body_bytes().is_none());
    }
}

// without_body ビルダーで body が None になる
proptest! {
    #[test]
    fn prop_response_without_body_builder(
        code in status_code(),
        body_data in proptest::collection::vec(any::<u8>(), 0..256),
    ) {
        let response = Response::new(code, "OK").unwrap()
            .body(body_data)
            .without_body();
        prop_assert!(response.body_bytes().is_none());
    }
}

// set_omit_body の値が is_body_omitted() で取得できる
proptest! {
    #[test]
    fn prop_response_set_omit_body(
        code in status_code(),
        omit in any::<bool>(),
    ) {
        let mut response = Response::new(code, "OK").unwrap();
        response.set_omit_body(omit);
        prop_assert_eq!(response.is_body_omitted(), omit);
    }
}

// add_header のチェイン呼び出しで複数ヘッダーが順序通り追加される
proptest! {
    #[test]
    fn prop_response_add_header_chain(
        code in status_code(),
        headers in proptest::collection::vec((header_name(), header_value()), 1..5),
    ) {
        let mut response = Response::new(code, "OK").unwrap();
        // 1 つ目だけは add_header(..)?... のチェイン形式で呼べないので unwrap で受ける
        // ここでは for ループで unwrap するが、内部的には Result<&mut Self, _> を消費している。
        for (name, value) in &headers {
            response.add_header(name.as_str(), value.as_str()).unwrap();
        }
        prop_assert_eq!(HttpHead::headers(&response).len(), headers.len());
        for (i, (name, value)) in headers.iter().enumerate() {
            prop_assert_eq!(&HttpHead::headers(&response)[i].0, name);
            prop_assert_eq!(&HttpHead::headers(&response)[i].1, value);
        }
    }
}

// add_header は impl Into<String> として &str / String の両方を受け取る
proptest! {
    #[test]
    fn prop_response_add_header_accepts_str_and_string(
        code in status_code(),
        name in header_name(),
        value in header_value(),
    ) {
        // &str
        let mut r1 = Response::new(code, "OK").unwrap();
        r1.add_header(name.as_str(), value.as_str()).unwrap();
        // String (ムーブ)
        let mut r2 = Response::new(code, "OK").unwrap();
        r2.add_header(name.clone(), value.clone()).unwrap();
        prop_assert_eq!(r1.get_header(&name), Some(value.as_str()));
        prop_assert_eq!(r2.get_header(&name), Some(value.as_str()));
    }
}

// body は impl Into<Vec<u8>> として Vec<u8> を受け取る
proptest! {
    #[test]
    fn prop_response_body_accepts_vec(
        code in status_code(),
        body_data in proptest::collection::vec(any::<u8>(), 0..256),
    ) {
        let response = Response::new(code, "OK").unwrap().body(body_data.clone());
        prop_assert_eq!(response.body_bytes(), Some(body_data.as_slice()));
    }
}
