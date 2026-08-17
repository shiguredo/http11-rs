//! Request 構造体のプロパティテスト (request.rs)

use shiguredo_http11::{
    BodyKind, BodyProgress, EncodeError, HeaderName, HttpHead, Method, Request, RequestDecoder,
};

// ========================================
// ジェネレータ定義
// ========================================

// HTTP トークン文字 (RFC 9110 Section 5.6.2)
fn token_char(ctx: &mut noprop::TestCaseContext) -> char {
    match noprop::sample_usize_in(ctx, 0..6) {
        0 => char::from(b'a' + noprop::sample_usize_in(ctx, 0..26) as u8),
        1 => char::from(b'A' + noprop::sample_usize_in(ctx, 0..26) as u8),
        2 => char::from(b'0' + noprop::sample_usize_in(ctx, 0..10) as u8),
        3 => '-',
        4 => '_',
        _ => '.',
    }
}

fn token_string(ctx: &mut noprop::TestCaseContext, max_len: usize) -> String {
    let len = noprop::sample_usize_in(ctx, 1..=max_len);
    let mut s = String::new();
    for _ in 0..len {
        s.push(token_char(ctx));
    }
    s
}

// HTTP ヘッダー名
fn header_name(ctx: &mut noprop::TestCaseContext) -> HeaderName {
    match noprop::sample_usize_in(ctx, 0..5) {
        0 => HeaderName::from_static(b"Content-Type"),
        1 => HeaderName::from_static(b"Accept"),
        2 => HeaderName::from_static(b"User-Agent"),
        3 => HeaderName::from_static(b"Cache-Control"),
        _ => {
            let s = token_string(ctx, 32);
            HeaderName::new(s.as_bytes())
                .expect("トークン文字列はヘッダー名として有効なはず (実装バグ)")
        }
    }
}

// HTTP ヘッダー値 (RFC 9110 Section 5.5)
// field-vchar = VCHAR / obs-text
// VCHAR = %x21-7E, obs-text = %x80-FF
//
// obs-text を含む共通ジェネレータは pbt::field_vchar / pbt::header_value を使用する。

// HTTP メソッド
fn http_method(ctx: &mut noprop::TestCaseContext) -> Method {
    match noprop::sample_usize_in(ctx, 0..13) {
        0 => Method::GET,
        1 => Method::POST,
        2 => Method::PUT,
        3 => Method::DELETE,
        4 => Method::HEAD,
        5 => Method::OPTIONS,
        6 => Method::PATCH,
        7 => Method::QUERY,
        // RTSP メソッド
        8 => Method::new(b"DESCRIBE").expect("DESCRIBE は有効なトークンのはず (実装バグ)"),
        9 => Method::new(b"SETUP").expect("SETUP は有効なトークンのはず (実装バグ)"),
        10 => Method::new(b"PLAY").expect("PLAY は有効なトークンのはず (実装バグ)"),
        11 => Method::new(b"PAUSE").expect("PAUSE は有効なトークンのはず (実装バグ)"),
        _ => Method::new(b"TEARDOWN").expect("TEARDOWN は有効なトークンのはず (実装バグ)"),
    }
}

// URI (スペースや CRLF を含まない)
fn http_uri(ctx: &mut noprop::TestCaseContext) -> String {
    match noprop::sample_usize_in(ctx, 0..3) {
        0 => "/".to_string(),
        1 => {
            let len = noprop::sample_usize_in(ctx, 1..=64);
            let mut s = "/".to_string();
            for _ in 0..len {
                s.push(noprop::sample_choice(
                    ctx,
                    b"abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789/_.-",
                ) as char);
            }
            s
        }
        _ => {
            let mut s = "rtsp://".to_string();
            let host_len = noprop::sample_usize_in(ctx, 1..=32);
            for _ in 0..host_len {
                s.push(noprop::sample_choice(ctx, b"abcdefghijklmnopqrstuvwxyz.") as char);
            }
            s.push('/');
            let path_len = noprop::sample_usize_in(ctx, 1..=32);
            for _ in 0..path_len {
                s.push(noprop::sample_choice(ctx, b"abcdefghijklmnopqrstuvwxyz/") as char);
            }
            s
        }
    }
}

// ヘッダーのリスト
fn headers(ctx: &mut noprop::TestCaseContext) -> Vec<(HeaderName, String)> {
    let len = noprop::sample_usize_in(ctx, 0..10);
    let mut v = Vec::new();
    for _ in 0..len {
        v.push((header_name(ctx), pbt::header_value(ctx)));
    }
    v
}

// ボディ
fn body(ctx: &mut noprop::TestCaseContext) -> Vec<u8> {
    let len = noprop::sample_usize_in(ctx, 0..256);
    noprop::sample_bytes_vec(ctx, len)
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

/// リクエストの encode → decode ラウンドトリップで method / uri / ヘッダー数 / ボディが保存される
#[test]
fn prop_request_roundtrip() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);
    // ボディデコード分岐 (ContentLength / Chunked) に到達したケースがあるかを記録する網羅ゲート。
    // ボディが非空になる確率 p = 1 - 1/257 ≈ 0.9961 で、256 ケース全てが空になる確率は
    // 無視できるほど小さいが、万一空振りしてボディデコード経路が一度も実行されない事態を防ぐ。
    let body_decoded = std::cell::Cell::new(false);

    runner.run(256, |ctx| {
        let method = http_method(ctx);
        let uri = http_uri(ctx);
        let hdrs = headers(ctx);
        let body_data = body(ctx);

        let mut request = Request::new(method.clone(), &uri)
            .expect("リクエストのパース / 構築は成功するはず (実装バグ)");
        let host_value = host_for_uri(&uri);
        request
            .add_header(HeaderName::from_static(b"Host"), &host_value)
            .expect("リクエストのパース / 構築は成功するはず (実装バグ)");
        for (name, value) in &hdrs {
            // Host ヘッダーの重複を避ける
            if name != "Host" {
                request
                    .add_header(name.clone(), value)
                    .expect("リクエストのパース / 構築は成功するはず (実装バグ)");
            }
        }
        if !body_data.is_empty() {
            request = request.body(body_data.clone());
        }

        let encoded = request
            .encode()
            .expect("リクエストのパース / 構築は成功するはず (実装バグ)");

        let mut decoder = RequestDecoder::new();
        decoder
            .feed(&encoded)
            .expect("リクエストのパース / 構築は成功するはず (実装バグ)");
        let (head, body_kind) = decoder
            .decode_headers()
            .expect("結果は存在するはず (実装バグ)")
            .expect("リクエストのパース / 構築は成功するはず (実装バグ)");

        assert_eq!(head.method(), method.as_str());
        assert_eq!(head.uri(), uri.as_str());

        let mut decoded_body = Vec::new();
        match body_kind {
            BodyKind::ContentLength(_) | BodyKind::Chunked => {
                body_decoded.set(true);
                while let Some(data) = decoder.peek_body() {
                    decoded_body.extend_from_slice(data);
                    let len = data.len();
                    match decoder
                        .consume_body(len)
                        .expect("リクエストのパース / 構築は成功するはず (実装バグ)")
                    {
                        BodyProgress::Complete { .. } => break,
                        BodyProgress::Advanced | BodyProgress::NeedData => {}
                    }
                }
            }
            // リクエストでは CloseDelimited は使われない (RFC 9112)
            // Tunnel はレスポンスのみで発生 (CONNECT 2xx)
            BodyKind::CloseDelimited | BodyKind::None | BodyKind::Tunnel => {}
        }

        let expected_body: Vec<u8> = request.body_bytes().map(<[u8]>::to_vec).unwrap_or_default();
        assert_eq!(decoded_body, expected_body);

        // ヘッダー数は同じ (Content-Length が自動追加される可能性、Host は +1)
        let expected_header_count =
            if !body_data.is_empty() && !hdrs.iter().any(|(n, _)| n == "Content-Length") {
                hdrs.len() + 2 // Host + Content-Length
            } else {
                hdrs.len() + 1 // Host
            };
        assert_eq!(head.headers().len(), expected_header_count);
        Ok(())
    })?;

    // ジェネレータは valid-by-construction であり、ケース棄却が発生しないことの検証
    assert_eq!(
        runner.stats().rejected_cases,
        0,
        "ジェネレータが valid-by-construction であること\n{runner}"
    );
    // ボディデコード分岐の網羅ゲート
    // ボディが非空になる確率 p = 1 - 1/257 ≈ 0.9961 で、256 ケース中 1 回以上は
    // ほぼ確実に到達する (全滅確率 ≈ (1/257)^256 ≈ 0)
    assert!(
        body_decoded.get(),
        "ボディデコード分岐に到達したケースが存在すること\n{runner}"
    );
    Ok(())
}

// ========================================
// ストリーミングデコードテスト
// ========================================

/// 1 バイトずつ feed してもヘッダーが正しくデコードされる (ストリーミングデコード)
#[test]
fn prop_streaming_decode_request() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let method = http_method(ctx);
        let uri = http_uri(ctx);
        let hdrs = headers(ctx);
        let method_str = method.as_str().to_string();
        let mut request =
            Request::new(method, &uri).expect("リクエストのパース / 構築は成功するはず (実装バグ)");
        let host_value = host_for_uri(&uri);
        request
            .add_header(HeaderName::from_static(b"Host"), &host_value)
            .expect("リクエストのパース / 構築は成功するはず (実装バグ)");
        for (name, value) in &hdrs {
            if name != "Host" {
                request
                    .add_header(name.clone(), value)
                    .expect("リクエストのパース / 構築は成功するはず (実装バグ)");
            }
        }

        let encoded = request
            .encode()
            .expect("リクエストのパース / 構築は成功するはず (実装バグ)");

        // 1 バイトずつ feed
        let mut decoder = RequestDecoder::new();
        for byte in &encoded {
            decoder
                .feed(std::slice::from_ref(byte))
                .expect("リクエストのパース / 構築は成功するはず (実装バグ)");
        }
        let (head, _) = decoder
            .decode_headers()
            .expect("結果は存在するはず (実装バグ)")
            .expect("リクエストのパース / 構築は成功するはず (実装バグ)");

        assert_eq!(head.method(), &method_str);
        assert_eq!(head.uri(), uri.as_str());
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

/// ボディ付きリクエストのチャンク分割 feed でヘッダーとボディが正しくデコードされる
#[test]
fn prop_streaming_decode_request_with_body() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let method = http_method(ctx);
        let uri = http_uri(ctx);
        let body_len = noprop::sample_usize_in(ctx, 1..=128);
        let body_data = noprop::sample_bytes_vec(ctx, body_len);
        let mut request = Request::new(method.clone(), &uri)
            .expect("リクエストのパース / 構築は成功するはず (実装バグ)");
        let host_value = host_for_uri(&uri);
        request
            .add_header(HeaderName::from_static(b"Host"), &host_value)
            .expect("リクエストのパース / 構築は成功するはず (実装バグ)");
        let request = request.body(body_data.clone());
        let encoded = request
            .encode()
            .expect("リクエストのパース / 構築は成功するはず (実装バグ)");

        // チャンクサイズで分割して feed し、デコード完了まで繰り返す
        let mut decoder = RequestDecoder::new();
        let mut headers_decoded = false;
        let mut body_kind = BodyKind::None;
        let mut decoded_body = Vec::new();
        let mut decoded_method = String::new();

        for chunk in encoded.chunks(7) {
            decoder
                .feed(chunk)
                .expect("リクエストのパース / 構築は成功するはず (実装バグ)");

            if !headers_decoded && let Ok(Some((head, kind))) = decoder.decode_headers() {
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
                            match decoder
                                .consume_body(len)
                                .expect("リクエストのパース / 構築は成功するはず (実装バグ)")
                            {
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

        assert!(headers_decoded, "ヘッダーがデコードされるべき");
        assert_eq!(&decoded_method, method.as_str());
        assert_eq!(&decoded_body, &body_data);
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
// 複数リクエストのデコードテスト
// ========================================

/// 1 つのデコーダーで複数リクエストを reset しながらデコードできる
#[test]
fn prop_multiple_requests_same_decoder() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let method_count = noprop::sample_usize_in(ctx, 2..=4);
        let uri_count = noprop::sample_usize_in(ctx, 2..=4);
        let methods: Vec<Method> = (0..method_count).map(|_| http_method(ctx)).collect();
        let uris: Vec<String> = (0..uri_count).map(|_| http_uri(ctx)).collect();
        let count = methods.len().min(uris.len());
        let mut decoder = RequestDecoder::new();

        for i in 0..count {
            if i > 0 {
                decoder.reset();
            }
            let mut request = Request::new(methods[i].clone(), &uris[i])
                .expect("リクエストのパース / 構築は成功するはず (実装バグ)");
            let host_value = host_for_uri(&uris[i]);
            request
                .add_header(HeaderName::from_static(b"Host"), &host_value)
                .expect("リクエストのパース / 構築は成功するはず (実装バグ)");
            let encoded = request
                .encode()
                .expect("リクエストのパース / 構築は成功するはず (実装バグ)");
            decoder
                .feed(&encoded)
                .expect("リクエストのパース / 構築は成功するはず (実装バグ)");
            let (head, _) = decoder
                .decode_headers()
                .expect("結果は存在するはず (実装バグ)")
                .expect("リクエストのパース / 構築は成功するはず (実装バグ)");

            assert_eq!(head.method(), methods[i].as_str());
            assert_eq!(head.uri(), uris[i].as_str());
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

// エラー後にデコーダーをリセットして再利用できる
/// 不正データを feed した後に reset すれば正常なリクエストをデコードできる
#[test]
fn prop_decoder_reuse_after_error() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let garbage_len = noprop::sample_usize_in(ctx, 1..=64);
        let garbage = noprop::sample_bytes_vec(ctx, garbage_len);
        let method = http_method(ctx);
        let uri = http_uri(ctx);
        let mut decoder = RequestDecoder::new();

        // 不正データを feed してエラーを発生させる
        let _ = decoder.feed(&garbage);
        let _ = decoder.decode_headers();

        // リセットして正常なリクエストをデコード
        decoder.reset();
        let mut request = Request::new(method.clone(), &uri)
            .expect("リクエストのパース / 構築は成功するはず (実装バグ)");
        let host_value = host_for_uri(&uri);
        request
            .add_header(HeaderName::from_static(b"Host"), &host_value)
            .expect("リクエストのパース / 構築は成功するはず (実装バグ)");
        let encoded = request
            .encode()
            .expect("リクエストのパース / 構築は成功するはず (実装バグ)");
        decoder
            .feed(&encoded)
            .expect("リクエストのパース / 構築は成功するはず (実装バグ)");
        let (head, _) = decoder
            .decode_headers()
            .expect("結果は存在するはず (実装バグ)")
            .expect("リクエストのパース / 構築は成功するはず (実装バグ)");

        assert_eq!(head.method(), method.as_str());
        assert_eq!(head.uri(), uri.as_str());
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
// Request API テスト
// ========================================

/// Request::new で構築した値が各 getter と一致する
#[test]
fn prop_request_new_creates_valid_request() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let method = http_method(ctx);
        let uri = http_uri(ctx);
        let request = Request::new(method.clone(), &uri)
            .expect("リクエストのパース / 構築は成功するはず (実装バグ)");

        assert_eq!(request.method(), method.as_str());
        assert_eq!(request.uri(), &uri);
        assert_eq!(request.version(), "HTTP/1.1");
        assert!(HttpHead::headers(&request).is_empty());
        assert!(request.body_bytes().is_none());
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

/// with_version で指定したバージョンが保存される
#[test]
fn prop_request_with_version() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let method = http_method(ctx);
        let uri = http_uri(ctx);
        let request10 = Request::with_version(method.clone(), &uri, "HTTP/1.0")
            .expect("リクエストのパース / 構築は成功するはず (実装バグ)");
        let request11 = Request::with_version(method, &uri, "HTTP/1.1")
            .expect("リクエストのパース / 構築は成功するはず (実装バグ)");

        assert_eq!(request10.version(), "HTTP/1.0");
        assert_eq!(request11.version(), "HTTP/1.1");
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

/// header() ビルダーで追加したヘッダーがそのまま残る
#[test]
fn prop_request_header_builder_pattern() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let method = http_method(ctx);
        let uri = http_uri(ctx);
        let name = header_name(ctx);
        let value = pbt::header_value(ctx);
        let request = Request::new(method, &uri)
            .expect("リクエストのパース / 構築は成功するはず (実装バグ)")
            .header(name.clone(), &value)
            .expect("リクエストのパース / 構築は成功するはず (実装バグ)");

        let headers = HttpHead::headers(&request);
        assert_eq!(headers.len(), 1);
        assert_eq!(&headers[0].0, &name);
        assert_eq!(&headers[0].1, &value);
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

/// body() ビルダーで設定したボディが body_bytes() で取得できる
#[test]
fn prop_request_body_builder_pattern() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let method = http_method(ctx);
        let uri = http_uri(ctx);
        let body_data = body(ctx);
        let request = Request::new(method, &uri)
            .expect("リクエストのパース / 構築は成功するはず (実装バグ)")
            .body(body_data.clone());

        assert_eq!(request.body_bytes(), Some(body_data.as_slice()));
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

/// get_header() は大文字小文字を区別しない
#[test]
fn prop_request_get_header_case_insensitive() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let method = http_method(ctx);
        let uri = http_uri(ctx);
        let value = pbt::header_value(ctx);
        let request = Request::new(method, &uri)
            .expect("リクエストのパース / 構築は成功するはず (実装バグ)")
            .header(HeaderName::from_static(b"Content-Type"), &value)
            .expect("リクエストのパース / 構築は成功するはず (実装バグ)");

        assert_eq!(request.get_header("content-type"), Some(value.as_str()));
        assert_eq!(request.get_header("CONTENT-TYPE"), Some(value.as_str()));
        assert_eq!(request.get_header("Content-Type"), Some(value.as_str()));
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

/// get_headers() は複数の同名ヘッダーを大文字小文字を区別せずに取得する
#[test]
fn prop_request_get_headers_case_insensitive_multiple() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let method = http_method(ctx);
        let uri = http_uri(ctx);
        let value1 = pbt::header_value(ctx);
        let value2 = pbt::header_value(ctx);
        let request = Request::new(method, &uri)
            .expect("リクエストのパース / 構築は成功するはず (実装バグ)")
            .header(HeaderName::from_static(b"X-Custom"), &value1)
            .expect("リクエストのパース / 構築は成功するはず (実装バグ)")
            .header(HeaderName::from_static(b"x-custom"), &value2)
            .expect("リクエストのパース / 構築は成功するはず (実装バグ)");

        let values = request.get_headers("X-CUSTOM");
        assert_eq!(values.len(), 2);
        assert!(values.contains(&value1.as_str()));
        assert!(values.contains(&value2.as_str()));
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
// 構築時バリデーションの PBT
// ========================================

// CRLF を含む method は常に拒否される
/// CRLF を含むメソッド名は Method::new が常に Err を返す
#[test]
fn prop_request_rejects_method_with_crlf() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let prefix = token_string(ctx, 8);
        let infix = match noprop::sample_usize_in(ctx, 0..3) {
            0 => "\r\n",
            1 => "\r",
            _ => "\n",
        };
        let suffix = token_string(ctx, 8);
        let method = format!("{prefix}{infix}{suffix}");
        let result = Method::new(method.as_bytes());
        assert!(result.is_err(), "CRLF を含むメソッド名は拒否されるべき");
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

// CRLF を含む URI は常に拒否される
/// CRLF を含む URI は Request::new が常に Err を返す
#[test]
fn prop_request_rejects_uri_with_crlf() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let prefix = uri_token(ctx, 16);
        let infix = match noprop::sample_usize_in(ctx, 0..3) {
            0 => "\r\n",
            1 => "\r",
            _ => "\n",
        };
        let suffix = uri_token(ctx, 16);
        let uri = format!("{prefix}{infix}{suffix}");
        let result = Request::new(Method::GET, &uri);
        let is_invalid_target = matches!(result, Err(EncodeError::InvalidRequestTarget { .. }));
        assert!(is_invalid_target);
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

// ヘッダー値に CRLF が含まれていれば構築時に拒否される (smuggling 防御)
/// CRLF を含むヘッダー値は header() が常に Err を返す
#[test]
fn prop_request_rejects_header_value_with_crlf() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let prefix = pbt::header_value(ctx);
        let infix = match noprop::sample_usize_in(ctx, 0..3) {
            0 => "\r\n",
            1 => "\r",
            _ => "\n",
        };
        let suffix = pbt::header_value(ctx);
        let req = Request::new(Method::GET, "/")
            .expect("リクエストのパース / 構築は成功するはず (実装バグ)");
        let value = format!("{prefix}{infix}{suffix}");
        let result = req.header(HeaderName::from_static(b"X-Test"), &value);
        let is_invalid_value = matches!(result, Err(EncodeError::InvalidHeaderValue { .. }));
        assert!(is_invalid_value);
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

// set_header のアトミック性: 不正な値で失敗しても既存ヘッダーは残る
/// set_header が不正な値で失敗しても既存ヘッダーは消えない (アトミック性)
#[test]
fn prop_request_set_header_atomicity_on_invalid_value() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let old_value = pbt::header_value(ctx);
        let new_value = pbt::header_value(ctx);
        let mut req = Request::new(Method::GET, "/")
            .expect("リクエストのパース / 構築は成功するはず (実装バグ)");
        req.add_header(HeaderName::from_static(b"X-Test"), &old_value)
            .expect("リクエストのパース / 構築は成功するはず (実装バグ)");
        // 不正な値で set_header 失敗
        let invalid = format!("{new_value}\r\nEvil: x");
        let result = req.set_header(HeaderName::from_static(b"X-Test"), &invalid);
        let is_invalid_value = matches!(result, Err(EncodeError::InvalidHeaderValue { .. }));
        assert!(is_invalid_value);
        // 既存ヘッダーが消えていない
        assert_eq!(req.get_header("X-Test"), Some(old_value.as_str()));
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

/// RFC 9112 Section 3.2 の request-target に現れ得る文字のみで URI の一部を生成する
///
/// `/` や予約文字を除き、`[a-zA-Z0-9/_.-]` の各文字を 1..=max_len 個並べた文字列を返す。
fn uri_token(ctx: &mut noprop::TestCaseContext, max_len: usize) -> String {
    let len = noprop::sample_usize_in(ctx, 1..=max_len);
    let mut s = String::new();
    for _ in 0..len {
        s.push(noprop::sample_choice(
            ctx,
            b"abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789/_.-",
        ) as char);
    }
    s
}
