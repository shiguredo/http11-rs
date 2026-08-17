//! Response 構造体のプロパティテスト

use shiguredo_http11::{
    EncodeError, HeaderName, HttpHead, Response, ResponseDecoder, StatusClass, StatusCode,
};

// ========================================
// ジェネレータ定義
// ========================================

fn http_version(ctx: &mut noprop::TestCaseContext) -> String {
    match noprop::sample_usize_in(ctx, 0..4) {
        0 => "HTTP/1.0".to_string(),
        1 => "HTTP/1.1".to_string(),
        2 => "RTSP/1.0".to_string(),
        _ => "RTSP/2.0".to_string(),
    }
}

fn status_code(ctx: &mut noprop::TestCaseContext) -> u16 {
    match noprop::sample_usize_in(ctx, 0..5) {
        0 => noprop::sample_usize_in(ctx, 100..=101) as u16, // 1xx
        1 => noprop::sample_usize_in(ctx, 200..=206) as u16, // 2xx
        2 => noprop::sample_usize_in(ctx, 300..=308) as u16, // 3xx
        3 => noprop::sample_usize_in(ctx, 400..=451) as u16, // 4xx
        _ => noprop::sample_usize_in(ctx, 500..=511) as u16, // 5xx
    }
}

// RFC 9110 Section 15 が許容する 100..=599 の全範囲を生成する Strategy
// (IANA 未登録の拡張 / 私的ステータスコードを含む全範囲をカバーする)
fn status_code_full_range(ctx: &mut noprop::TestCaseContext) -> u16 {
    noprop::sample_usize_in(ctx, 100..=599) as u16
}

// IANA 登録の StatusCode 定数を全網羅で選択するジェネレータ
fn iana_status_code(ctx: &mut noprop::TestCaseContext) -> StatusCode {
    noprop::sample_choice(
        ctx,
        &[
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
        ],
    )
}

fn reason_phrase(ctx: &mut noprop::TestCaseContext) -> String {
    match noprop::sample_usize_in(ctx, 0..5) {
        0 => "OK".to_string(),
        1 => "Not Found".to_string(),
        2 => "Internal Server Error".to_string(),
        3 => "Bad Request".to_string(),
        _ => {
            let len = noprop::sample_usize_in(ctx, 1..=32);
            let mut s = String::new();
            for _ in 0..len {
                s.push(noprop::sample_choice(
                    ctx,
                    b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz ",
                ) as char);
            }
            s
        }
    }
}

fn header_name(ctx: &mut noprop::TestCaseContext) -> HeaderName {
    let mut s = String::new();
    s.push(
        noprop::sample_choice(ctx, b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz") as char,
    );
    let rest_len = noprop::sample_usize_in(ctx, 0..=31);
    for _ in 0..rest_len {
        s.push(noprop::sample_choice(
            ctx,
            b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-",
        ) as char);
    }
    HeaderName::new(s.as_bytes()).expect("生成したヘッダー名は有効なはず (実装バグ)")
}

// HTTP ヘッダー値 (RFC 9110 Section 5.5)
// field-vchar = VCHAR / obs-text
// obs-text を含む共通ジェネレータは pbt::field_vchar / pbt::header_value を使用する。

// ========================================
// コンストラクタのテスト
// ========================================

// with_status() はすべての IANA 登録 StatusCode 定数で infallible に Response を構築できる
/// with_status() が IANA 登録 StatusCode 定数で infallible に構築でき、各 getter が一致する
#[test]
fn prop_response_with_status_constructs_infallibly() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let status = iana_status_code(ctx);
        let response = Response::with_status(status);

        // version は HTTP/1.1 固定
        assert_eq!(HttpHead::version(&response), "HTTP/1.1");
        // status_code() は StatusCode::code() と一致
        assert_eq!(response.status_code(), status.code());
        // reason_phrase() は StatusCode::canonical_reason() と一致
        assert_eq!(response.reason_phrase(), status.canonical_reason());
        // 初期状態はヘッダーなし、ボディなし
        assert!(HttpHead::headers(&response).is_empty());
        assert!(response.body_bytes().is_none());
        assert!(!response.is_body_omitted());
        Ok(())
    })?;

    // ジェネレータは valid-by-construction であり、ケース棄却が発生しないことの検証
    assert_eq!(
        runner.stats().rejected_cases,
        0,
        "ジェネレータが valid-by-construction であること\n{runner}"
    );
    Ok(())
}

// with_status() で構築した Response は encode → decode のラウンドトリップで
// status_code / reason_phrase / version が保存される
/// with_status() で構築した Response の encode → decode ラウンドトリップで各フィールドが保存される
#[test]
fn prop_response_with_status_roundtrip() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let status = iana_status_code(ctx);
        let response = Response::with_status(status);
        let bytes = response
            .encode()
            .expect("レスポンスのパース / 構築は成功するはず (実装バグ)");

        let mut decoder = ResponseDecoder::new();
        decoder
            .feed(&bytes)
            .expect("レスポンスのパース / 構築は成功するはず (実装バグ)");
        let (head, _body_kind) = decoder
            .decode_headers()
            .expect("レスポンスのパース / 構築は成功するはず (実装バグ)")
            .expect("ヘッダーが揃っているべき");

        assert_eq!(head.status_code(), status.code());
        assert_eq!(head.reason_phrase(), status.canonical_reason());
        assert_eq!(head.version(), "HTTP/1.1");
        Ok(())
    })?;

    // ジェネレータは valid-by-construction であり、ケース棄却が発生しないことの検証
    assert_eq!(
        runner.stats().rejected_cases,
        0,
        "ジェネレータが valid-by-construction であること\n{runner}"
    );
    Ok(())
}

// 任意の status_code (100..=599) で構築した Response の status_class() が
// StatusClass::from_status_code(status_code) と一致する
/// 任意の status_code (100..=599) で構築した Response の status_class() が分類と一致する
#[test]
fn prop_response_status_class() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let code = status_code_full_range(ctx);
        let response =
            Response::new(code, "OK").expect("レスポンスのパース / 構築は成功するはず (実装バグ)");
        let expected =
            StatusClass::from_status_code(code).expect("100..=599 は必ず分類されるはず (実装バグ)");
        assert_eq!(response.status_class(), expected);
        Ok(())
    })?;

    // ジェネレータは valid-by-construction であり、ケース棄却が発生しないことの検証
    assert_eq!(
        runner.stats().rejected_cases,
        0,
        "ジェネレータが valid-by-construction であること\n{runner}"
    );
    Ok(())
}

// new() はデフォルトで HTTP/1.1
/// Response::new() のデフォルト version が HTTP/1.1 で、status_code / reason_phrase が保存される
#[test]
fn prop_response_new_default_version() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let code = status_code(ctx);
        let phrase = reason_phrase(ctx);
        let response = Response::new(code, &phrase)
            .expect("レスポンスのパース / 構築は成功するはず (実装バグ)");

        assert_eq!(HttpHead::version(&response), "HTTP/1.1");
        assert_eq!(response.status_code(), code);
        assert_eq!(response.reason_phrase(), &phrase);
        assert!(HttpHead::headers(&response).is_empty());
        assert!(response.body_bytes().is_none());
        assert!(!response.is_body_omitted());
        Ok(())
    })?;

    // ジェネレータは valid-by-construction であり、ケース棄却が発生しないことの検証
    assert_eq!(
        runner.stats().rejected_cases,
        0,
        "ジェネレータが valid-by-construction であること\n{runner}"
    );
    Ok(())
}

// with_version() でカスタムバージョン
/// with_version() で指定した version が保存され、各 getter が一致する
#[test]
fn prop_response_with_version() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let version = http_version(ctx);
        let code = status_code(ctx);
        let phrase = reason_phrase(ctx);
        let response = Response::with_version(&version, code, &phrase)
            .expect("レスポンスのパース / 構築は成功するはず (実装バグ)");

        assert_eq!(HttpHead::version(&response), &version);
        assert_eq!(response.status_code(), code);
        assert_eq!(response.reason_phrase(), &phrase);
        assert!(HttpHead::headers(&response).is_empty());
        assert!(response.body_bytes().is_none());
        assert!(!response.is_body_omitted());
        Ok(())
    })?;

    // ジェネレータは valid-by-construction であり、ケース棄却が発生しないことの検証
    assert_eq!(
        runner.stats().rejected_cases,
        0,
        "ジェネレータが valid-by-construction であること\n{runner}"
    );
    Ok(())
}

// ========================================
// ビルダーパターンのテスト
// ========================================

// header() ビルダー
/// header() ビルダーで追加したヘッダーがそのまま残る
#[test]
fn prop_response_header_builder() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let code = status_code(ctx);
        let name = header_name(ctx);
        let value = pbt::header_value(ctx);
        let response = Response::new(code, "OK")
            .expect("レスポンスのパース / 構築は成功するはず (実装バグ)")
            .header(name.clone(), &value)
            .expect("レスポンスのパース / 構築は成功するはず (実装バグ)");

        assert_eq!(HttpHead::headers(&response).len(), 1);
        assert_eq!(&HttpHead::headers(&response)[0].0, &name);
        assert_eq!(&HttpHead::headers(&response)[0].1, &value);
        Ok(())
    })?;

    // ジェネレータは valid-by-construction であり、ケース棄却が発生しないことの検証
    assert_eq!(
        runner.stats().rejected_cases,
        0,
        "ジェネレータが valid-by-construction であること\n{runner}"
    );
    Ok(())
}

// 複数の header() チェーン
/// 複数の header() チェーンでヘッダーが順序通り追加される
#[test]
fn prop_response_header_builder_chain() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let code = status_code(ctx);
        let header_len = noprop::sample_usize_in(ctx, 1..=4);
        let headers: Vec<(HeaderName, String)> = (0..header_len)
            .map(|_| (header_name(ctx), pbt::header_value(ctx)))
            .collect();
        let mut response =
            Response::new(code, "OK").expect("レスポンスのパース / 構築は成功するはず (実装バグ)");
        for (name, value) in &headers {
            response = response
                .header(name.clone(), value)
                .expect("レスポンスのパース / 構築は成功するはず (実装バグ)");
        }

        assert_eq!(HttpHead::headers(&response).len(), headers.len());
        for (i, (name, value)) in headers.iter().enumerate() {
            assert_eq!(&HttpHead::headers(&response)[i].0, name);
            assert_eq!(&HttpHead::headers(&response)[i].1, value);
        }
        Ok(())
    })?;

    // ジェネレータは valid-by-construction であり、ケース棄却が発生しないことの検証
    assert_eq!(
        runner.stats().rejected_cases,
        0,
        "ジェネレータが valid-by-construction であること\n{runner}"
    );
    Ok(())
}

// body() ビルダー
/// body() ビルダーで設定したボディが body_bytes() で取得できる
#[test]
fn prop_response_body_builder() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let code = status_code(ctx);
        let body_len = noprop::sample_usize_in(ctx, 0..=256);
        let body_data = noprop::sample_bytes_vec(ctx, body_len);
        let response = Response::new(code, "OK")
            .expect("レスポンスのパース / 構築は成功するはず (実装バグ)")
            .body(body_data.clone());

        assert_eq!(response.body_bytes(), Some(body_data.as_slice()));
        Ok(())
    })?;

    // ジェネレータは valid-by-construction であり、ケース棄却が発生しないことの検証
    assert_eq!(
        runner.stats().rejected_cases,
        0,
        "ジェネレータが valid-by-construction であること\n{runner}"
    );
    Ok(())
}

// omit_body() ビルダー
/// omit_body() ビルダーの値が is_body_omitted() で取得できる
#[test]
fn prop_response_omit_body_builder() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let code = status_code(ctx);
        let omit = noprop::sample_bool(ctx);
        let response = Response::new(code, "OK")
            .expect("レスポンスのパース / 構築は成功するはず (実装バグ)")
            .omit_body(omit);
        assert_eq!(response.is_body_omitted(), omit);
        Ok(())
    })?;

    // ジェネレータは valid-by-construction であり、ケース棄却が発生しないことの検証
    assert_eq!(
        runner.stats().rejected_cases,
        0,
        "ジェネレータが valid-by-construction であること\n{runner}"
    );
    Ok(())
}

// ========================================
// ヘッダー操作のテスト
// ========================================

// get_header() は大文字小文字を区別しない
/// get_header() は大文字小文字を区別しない
#[test]
fn prop_response_get_header_case_insensitive() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let code = status_code(ctx);
        let value = pbt::header_value(ctx);
        let response = Response::new(code, "OK")
            .expect("レスポンスのパース / 構築は成功するはず (実装バグ)")
            .header(HeaderName::from_static(b"Content-Type"), &value)
            .expect("レスポンスのパース / 構築は成功するはず (実装バグ)");

        assert_eq!(response.get_header("Content-Type"), Some(value.as_str()));
        assert_eq!(response.get_header("content-type"), Some(value.as_str()));
        assert_eq!(response.get_header("CONTENT-TYPE"), Some(value.as_str()));
        Ok(())
    })?;

    // ジェネレータは valid-by-construction であり、ケース棄却が発生しないことの検証
    assert_eq!(
        runner.stats().rejected_cases,
        0,
        "ジェネレータが valid-by-construction であること\n{runner}"
    );
    Ok(())
}

// get_headers() は複数の同名ヘッダーをすべて取得
/// get_headers() は複数の同名ヘッダーをすべて取得する
#[test]
fn prop_response_get_headers_multiple() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let code = status_code(ctx);
        let value_count = noprop::sample_usize_in(ctx, 1..=4);
        let values: Vec<String> = (0..value_count).map(|_| pbt::header_value(ctx)).collect();
        let mut response =
            Response::new(code, "OK").expect("レスポンスのパース / 構築は成功するはず (実装バグ)");
        for value in &values {
            response = response
                .header(HeaderName::from_static(b"Set-Cookie"), value)
                .expect("レスポンスのパース / 構築は成功するはず (実装バグ)");
        }
        let headers = response.get_headers("set-cookie");
        assert_eq!(headers.len(), values.len());
        for (i, value) in values.iter().enumerate() {
            assert_eq!(headers[i], value.as_str());
        }
        Ok(())
    })?;

    // ジェネレータは valid-by-construction であり、ケース棄却が発生しないことの検証
    assert_eq!(
        runner.stats().rejected_cases,
        0,
        "ジェネレータが valid-by-construction であること\n{runner}"
    );
    Ok(())
}

// get_headers() は大文字小文字を区別しない
/// get_headers() は大文字小文字を区別しない
#[test]
fn prop_response_get_headers_case_insensitive() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let code = status_code(ctx);
        let value = pbt::header_value(ctx);
        let response = Response::new(code, "OK")
            .expect("レスポンスのパース / 構築は成功するはず (実装バグ)")
            .header(HeaderName::from_static(b"Set-Cookie"), &value)
            .expect("レスポンスのパース / 構築は成功するはず (実装バグ)");

        assert_eq!(response.get_headers("set-cookie").len(), 1);
        assert_eq!(response.get_headers("SET-COOKIE").len(), 1);
        Ok(())
    })?;

    // ジェネレータは valid-by-construction であり、ケース棄却が発生しないことの検証
    assert_eq!(
        runner.stats().rejected_cases,
        0,
        "ジェネレータが valid-by-construction であること\n{runner}"
    );
    Ok(())
}

// has_header() の動作確認
/// has_header() は大文字小文字を区別せず、存在しないヘッダーには false を返す
#[test]
fn prop_response_has_header() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let code = status_code(ctx);
        let name = header_name(ctx);
        let value = pbt::header_value(ctx);
        let response = Response::new(code, "OK")
            .expect("レスポンスのパース / 構築は成功するはず (実装バグ)")
            .header(name.clone(), &value)
            .expect("レスポンスのパース / 構築は成功するはず (実装バグ)");

        assert!(response.has_header(name.as_str()));
        assert!(response.has_header(&name.as_str().to_lowercase()));
        assert!(response.has_header(&name.as_str().to_uppercase()));
        assert!(!response.has_header("X-Not-Exists"));
        Ok(())
    })?;

    // ジェネレータは valid-by-construction であり、ケース棄却が発生しないことの検証
    assert_eq!(
        runner.stats().rejected_cases,
        0,
        "ジェネレータが valid-by-construction であること\n{runner}"
    );
    Ok(())
}

// ========================================
// Connection と Keep-Alive のテスト
// ========================================

// connection() はヘッダー値を返す
/// connection() は Connection ヘッダーの値をそのまま返す
#[test]
fn prop_response_connection_header() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let code = status_code(ctx);
        let conn_value =
            noprop::sample_choice(ctx, &["keep-alive", "close", "Keep-Alive", "Close"]);
        let response = Response::new(code, "OK")
            .expect("レスポンスのパース / 構築は成功するはず (実装バグ)")
            .header(HeaderName::from_static(b"Connection"), conn_value)
            .expect("レスポンスのパース / 構築は成功するはず (実装バグ)");

        assert_eq!(response.connection(), Some(conn_value));
        Ok(())
    })?;

    // ジェネレータは valid-by-construction であり、ケース棄却が発生しないことの検証
    assert_eq!(
        runner.stats().rejected_cases,
        0,
        "ジェネレータが valid-by-construction であること\n{runner}"
    );
    Ok(())
}

// ========================================
// Content-Length と Transfer-Encoding のテスト
// ========================================

// content_length() は数値を返す
/// content_length() は Content-Length ヘッダーの数値を返す
#[test]
fn prop_response_content_length() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let code = status_code(ctx);
        let len = noprop::sample_usize_in(ctx, 0..1_000_000);
        let response = Response::new(code, "OK")
            .expect("レスポンスのパース / 構築は成功するはず (実装バグ)")
            .header(HeaderName::from_static(b"Content-Length"), len.to_string())
            .expect("レスポンスのパース / 構築は成功するはず (実装バグ)");

        assert_eq!(
            response
                .content_length()
                .expect("レスポンスのパース / 構築は成功するはず (実装バグ)"),
            Some(len as u64)
        );
        Ok(())
    })?;

    // ジェネレータは valid-by-construction であり、ケース棄却が発生しないことの検証
    assert_eq!(
        runner.stats().rejected_cases,
        0,
        "ジェネレータが valid-by-construction であること\n{runner}"
    );
    Ok(())
}

// ========================================
// バリデーションのテスト
// ========================================

// 不正な status_code は構築時に拒否される
/// 100 未満または 599 より大きい status_code は構築時に拒否される
#[test]
fn prop_response_invalid_status_code() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let code = match noprop::sample_usize_in(ctx, 0..2) {
            0 => noprop::sample_usize_in(ctx, 0..100) as u16,
            _ => noprop::sample_usize_in(ctx, 600..=u16::MAX as usize) as u16,
        };
        let phrase = reason_phrase(ctx);
        let result = Response::new(code, &phrase);
        let is_invalid_status = matches!(result, Err(EncodeError::InvalidStatusCode { .. }));
        assert!(is_invalid_status);
        Ok(())
    })?;

    // ジェネレータは valid-by-construction であり、ケース棄却が発生しないことの検証
    assert_eq!(
        runner.stats().rejected_cases,
        0,
        "ジェネレータが valid-by-construction であること\n{runner}"
    );
    Ok(())
}

// 制御文字を含む reason_phrase は構築時に拒否される
fn invalid_reason_phrase_char(ctx: &mut noprop::TestCaseContext) -> char {
    match noprop::sample_usize_in(ctx, 0..3) {
        // 制御文字 0x00-0x08, 0x0A-0x1F, 0x7F
        0 => char::from_u32(noprop::sample_usize_in(ctx, 0x00..=0x08) as u32)
            .expect("制御文字 0x00-0x08 は Unicode scalar として有効なはず (実装バグ)"),
        1 => char::from_u32(noprop::sample_usize_in(ctx, 0x0A..=0x1F) as u32)
            .expect("制御文字 0x0A-0x1F は Unicode scalar として有効なはず (実装バグ)"),
        _ => '\u{7F}',
    }
}

/// 制御文字を含む reason_phrase は構築時に拒否される
#[test]
fn prop_response_invalid_reason_phrase() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let code = status_code(ctx);
        let bad_char = invalid_reason_phrase_char(ctx);
        let phrase = format!("OK{bad_char}bad");
        let result = Response::new(code, &phrase);
        let is_invalid = matches!(result, Err(EncodeError::InvalidReasonPhrase { .. }));
        assert!(is_invalid);
        Ok(())
    })?;

    // ジェネレータは valid-by-construction であり、ケース棄却が発生しないことの検証
    assert_eq!(
        runner.stats().rejected_cases,
        0,
        "ジェネレータが valid-by-construction であること\n{runner}"
    );
    Ok(())
}

// 空の reason_phrase も構築時に拒否される
/// 空の reason_phrase は構築時に拒否される
#[test]
fn prop_response_empty_reason_phrase_rejected() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let code = status_code(ctx);
        let result = Response::new(code, "");
        let is_invalid = matches!(result, Err(EncodeError::InvalidReasonPhrase { .. }));
        assert!(is_invalid);
        Ok(())
    })?;

    // ジェネレータは valid-by-construction であり、ケース棄却が発生しないことの検証
    assert_eq!(
        runner.stats().rejected_cases,
        0,
        "ジェネレータが valid-by-construction であること\n{runner}"
    );
    Ok(())
}

// 不正な version は構築時に拒否される
fn invalid_version(ctx: &mut noprop::TestCaseContext) -> String {
    noprop::sample_choice(
        ctx,
        &[
            String::new(),
            "garbage".to_string(),
            "HTTP /1.1".to_string(),
            "HTTP/1.1\r\nX: y".to_string(),
            "HTTP/abc.def".to_string(),
        ],
    )
}

/// 不正な version は構築時に拒否される
#[test]
fn prop_response_invalid_version() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let version = invalid_version(ctx);
        let code = status_code(ctx);
        let phrase = reason_phrase(ctx);
        let result = Response::with_version(&version, code, &phrase);
        let is_invalid = matches!(result, Err(EncodeError::InvalidVersion { .. }));
        assert!(is_invalid);
        Ok(())
    })?;

    // ジェネレータは valid-by-construction であり、ケース棄却が発生しないことの検証
    assert_eq!(
        runner.stats().rejected_cases,
        0,
        "ジェネレータが valid-by-construction であること\n{runner}"
    );
    Ok(())
}

// 不正なヘッダー名は add_header / header で拒否される
fn invalid_header_name(ctx: &mut noprop::TestCaseContext) -> String {
    noprop::sample_choice(
        ctx,
        &[
            String::new(),
            "Bad Name".to_string(),
            "Bad\r\nName".to_string(),
            "Bad\0Name".to_string(),
            "Bad:Name".to_string(),
        ],
    )
}

/// 不正なヘッダー名は HeaderName::new が Err を返す
#[test]
fn prop_response_invalid_header_name() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let name = invalid_header_name(ctx);
        let result = HeaderName::new(name.as_bytes());
        assert!(result.is_err());
        Ok(())
    })?;

    // ジェネレータは valid-by-construction であり、ケース棄却が発生しないことの検証
    assert_eq!(
        runner.stats().rejected_cases,
        0,
        "ジェネレータが valid-by-construction であること\n{runner}"
    );
    Ok(())
}

// 不正なヘッダー値は add_header / header で拒否される
fn invalid_header_value_char(ctx: &mut noprop::TestCaseContext) -> char {
    match noprop::sample_usize_in(ctx, 0..3) {
        0 => char::from_u32(noprop::sample_usize_in(ctx, 0x00..=0x08) as u32)
            .expect("制御文字 0x00-0x08 は Unicode scalar として有効なはず (実装バグ)"),
        1 => char::from_u32(noprop::sample_usize_in(ctx, 0x0A..=0x1F) as u32)
            .expect("制御文字 0x0A-0x1F は Unicode scalar として有効なはず (実装バグ)"),
        _ => '\u{7F}',
    }
}

/// 制御文字を含むヘッダー値は add_header が Err を返す
#[test]
fn prop_response_invalid_header_value() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let code = status_code(ctx);
        let name = header_name(ctx);
        let bad_char = invalid_header_value_char(ctx);
        let value = format!("good{bad_char}bad");
        let mut response =
            Response::new(code, "OK").expect("レスポンスのパース / 構築は成功するはず (実装バグ)");
        let result = response.add_header(name.clone(), &value);
        let is_invalid = matches!(result, Err(EncodeError::InvalidHeaderValue { .. }));
        assert!(is_invalid);
        Ok(())
    })?;

    // ジェネレータは valid-by-construction であり、ケース棄却が発生しないことの検証
    assert_eq!(
        runner.stats().rejected_cases,
        0,
        "ジェネレータが valid-by-construction であること\n{runner}"
    );
    Ok(())
}

// ========================================
// mutator (set_body / clear_body / without_body / set_omit_body / チェイン) の PBT
// ========================================

// set_body → body_bytes() のラウンドトリップ
/// set_body で設定したボディが body_bytes() で取得できる
#[test]
fn prop_response_set_body_roundtrip() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let code = status_code(ctx);
        let body_len = noprop::sample_usize_in(ctx, 0..=256);
        let body_data = noprop::sample_bytes_vec(ctx, body_len);
        let mut response =
            Response::new(code, "OK").expect("レスポンスのパース / 構築は成功するはず (実装バグ)");
        response.set_body(body_data.clone());
        assert_eq!(response.body_bytes(), Some(body_data.as_slice()));
        Ok(())
    })?;

    // ジェネレータは valid-by-construction であり、ケース棄却が発生しないことの検証
    assert_eq!(
        runner.stats().rejected_cases,
        0,
        "ジェネレータが valid-by-construction であること\n{runner}"
    );
    Ok(())
}

// set_body → clear_body で body が None になる
/// set_body の後に clear_body すると body_bytes() が None になる
#[test]
fn prop_response_set_then_clear_body() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let code = status_code(ctx);
        let body_len = noprop::sample_usize_in(ctx, 0..=256);
        let body_data = noprop::sample_bytes_vec(ctx, body_len);
        let mut response =
            Response::new(code, "OK").expect("レスポンスのパース / 構築は成功するはず (実装バグ)");
        response.set_body(body_data);
        response.clear_body();
        assert!(response.body_bytes().is_none());
        Ok(())
    })?;

    // ジェネレータは valid-by-construction であり、ケース棄却が発生しないことの検証
    assert_eq!(
        runner.stats().rejected_cases,
        0,
        "ジェネレータが valid-by-construction であること\n{runner}"
    );
    Ok(())
}

// without_body ビルダーで body が None になる
/// body() の後に without_body() すると body_bytes() が None になる
#[test]
fn prop_response_without_body_builder() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let code = status_code(ctx);
        let body_len = noprop::sample_usize_in(ctx, 0..=256);
        let body_data = noprop::sample_bytes_vec(ctx, body_len);
        let response = Response::new(code, "OK")
            .expect("レスポンスのパース / 構築は成功するはず (実装バグ)")
            .body(body_data)
            .without_body();
        assert!(response.body_bytes().is_none());
        Ok(())
    })?;

    // ジェネレータは valid-by-construction であり、ケース棄却が発生しないことの検証
    assert_eq!(
        runner.stats().rejected_cases,
        0,
        "ジェネレータが valid-by-construction であること\n{runner}"
    );
    Ok(())
}

// set_omit_body の値が is_body_omitted() で取得できる
/// set_omit_body で設定した値が is_body_omitted() で取得できる
#[test]
fn prop_response_set_omit_body() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let code = status_code(ctx);
        let omit = noprop::sample_bool(ctx);
        let mut response =
            Response::new(code, "OK").expect("レスポンスのパース / 構築は成功するはず (実装バグ)");
        response.set_omit_body(omit);
        assert_eq!(response.is_body_omitted(), omit);
        Ok(())
    })?;

    // ジェネレータは valid-by-construction であり、ケース棄却が発生しないことの検証
    assert_eq!(
        runner.stats().rejected_cases,
        0,
        "ジェネレータが valid-by-construction であること\n{runner}"
    );
    Ok(())
}

// add_header のチェイン呼び出しで複数ヘッダーが順序通り追加される
/// add_header の連続呼び出しで複数ヘッダーが順序通り追加される
#[test]
fn prop_response_add_header_chain() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let code = status_code(ctx);
        let header_len = noprop::sample_usize_in(ctx, 1..=4);
        let headers: Vec<(HeaderName, String)> = (0..header_len)
            .map(|_| (header_name(ctx), pbt::header_value(ctx)))
            .collect();
        let mut response =
            Response::new(code, "OK").expect("レスポンスのパース / 構築は成功するはず (実装バグ)");
        // 1 つ目だけは add_header(..)?... のチェイン形式で呼べないので unwrap で受ける
        // ここでは for ループで unwrap するが、内部的には Result<&mut Self, _> を消費している。
        for (name, value) in &headers {
            response
                .add_header(name.clone(), value.as_str())
                .expect("レスポンスのパース / 構築は成功するはず (実装バグ)");
        }
        assert_eq!(HttpHead::headers(&response).len(), headers.len());
        for (i, (name, value)) in headers.iter().enumerate() {
            assert_eq!(&HttpHead::headers(&response)[i].0, name);
            assert_eq!(&HttpHead::headers(&response)[i].1, value);
        }
        Ok(())
    })?;

    // ジェネレータは valid-by-construction であり、ケース棄却が発生しないことの検証
    assert_eq!(
        runner.stats().rejected_cases,
        0,
        "ジェネレータが valid-by-construction であること\n{runner}"
    );
    Ok(())
}

// add_header は impl Into<String> として &str / String の両方を受け取る
/// add_header が &str と String の両方の値を受け取れる
#[test]
fn prop_response_add_header_accepts_str_and_string() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let code = status_code(ctx);
        let name = header_name(ctx);
        let value = pbt::header_value(ctx);
        // &str
        let mut r1 =
            Response::new(code, "OK").expect("レスポンスのパース / 構築は成功するはず (実装バグ)");
        r1.add_header(name.clone(), value.as_str())
            .expect("レスポンスのパース / 構築は成功するはず (実装バグ)");
        // String (ムーブ)
        let mut r2 =
            Response::new(code, "OK").expect("レスポンスのパース / 構築は成功するはず (実装バグ)");
        r2.add_header(name.clone(), value.clone())
            .expect("レスポンスのパース / 構築は成功するはず (実装バグ)");
        assert_eq!(r1.get_header(name.as_str()), Some(value.as_str()));
        assert_eq!(r2.get_header(name.as_str()), Some(value.as_str()));
        Ok(())
    })?;

    // ジェネレータは valid-by-construction であり、ケース棄却が発生しないことの検証
    assert_eq!(
        runner.stats().rejected_cases,
        0,
        "ジェネレータが valid-by-construction であること\n{runner}"
    );
    Ok(())
}

// body は impl Into<Vec<u8>> として Vec<u8> を受け取る
/// body() が Vec<u8> を受け取り、body_bytes() で取得できる
#[test]
fn prop_response_body_accepts_vec() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let code = status_code(ctx);
        let body_len = noprop::sample_usize_in(ctx, 0..=256);
        let body_data = noprop::sample_bytes_vec(ctx, body_len);
        let response = Response::new(code, "OK")
            .expect("レスポンスのパース / 構築は成功するはず (実装バグ)")
            .body(body_data.clone());
        assert_eq!(response.body_bytes(), Some(body_data.as_slice()));
        Ok(())
    })?;

    // ジェネレータは valid-by-construction であり、ケース棄却が発生しないことの検証
    assert_eq!(
        runner.stats().rejected_cases,
        0,
        "ジェネレータが valid-by-construction であること\n{runner}"
    );
    Ok(())
}
