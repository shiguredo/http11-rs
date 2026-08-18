//! HTTP 認証のプロパティテスト (Digest / Bearer / Authorization / AuthChallenge)

use shiguredo_http11::auth::{
    AuthChallenge, Authorization, BasicAuth, BearerChallenge, BearerToken, DigestAuth,
    DigestChallenge, ProxyAuthenticate, ProxyAuthorization, WwwAuthenticate,
};

// ========================================
// Strategy 定義
// ========================================

/// token68 文字集合 (A-Z, a-z, 0-9, -, ., _, ~, +, /)
const TOKEN68_CHARS: &[u8] =
    b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-._~+/";

/// realm / nonce などのパラメータ値: [a-zA-Z0-9._-]{1,32}
const PARAM_CHARS: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789._-";

/// 大文字 A-Z / 小文字 a-z
const ALPHA_CHARS: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz";

/// 大文字 A-Z / 小文字 a-z / 数字 0-9
const ALNUM_CHARS: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789";

/// 大文字 A-Z / 小文字 a-z / 数字 0-9 / アンダースコア
const ALNUM_UNDERSCORE_CHARS: &[u8] =
    b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789_";

/// BasicAuth の password 文字集合
const PASSWORD_CHARS: &[u8] =
    b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789!@#$%^&*";

/// Digest の response 文字集合
const HEX_LOWER_CHARS: &[u8] = b"0123456789abcdef";

/// 小文字 a-z
const ALPHA_LOWER_CHARS: &[u8] = b"abcdefghijklmnopqrstuvwxyz";

/// 小文字 a-z / スラッシュ
const ALPHA_LOWER_SLASH_CHARS: &[u8] = b"abcdefghijklmnopqrstuvwxyz/";

/// バイト集合から 1 文字を一様に選ぶ
fn char_from_bytes(ctx: &mut noprop::TestCaseContext, bytes: &[u8]) -> char {
    bytes[noprop::sample_usize_in(ctx, 0..bytes.len())] as char
}

/// バイト集合から構成される文字列を生成する
fn string_from_bytes(
    ctx: &mut noprop::TestCaseContext,
    bytes: &[u8],
    len_range: core::ops::RangeInclusive<usize>,
) -> String {
    let len = noprop::sample_usize_in(ctx, len_range);
    let mut s = String::with_capacity(len);
    for _ in 0..len {
        s.push(char_from_bytes(ctx, bytes));
    }
    s
}

/// token68 文字列 (RFC 7235 Section 2.1)
fn token68_string(ctx: &mut noprop::TestCaseContext, min: usize, max: usize) -> String {
    string_from_bytes(ctx, TOKEN68_CHARS, min..=max)
}

/// realm / nonce などのパラメータ値
fn param_value(ctx: &mut noprop::TestCaseContext) -> String {
    string_from_bytes(ctx, PARAM_CHARS, 1..=32)
}

/// BasicAuth の username: [a-zA-Z][a-zA-Z0-9_]{0,15}
fn basic_username(ctx: &mut noprop::TestCaseContext) -> String {
    let mut s = String::with_capacity(16);
    s.push(char_from_bytes(ctx, ALPHA_CHARS));
    s.push_str(&string_from_bytes(ctx, ALNUM_UNDERSCORE_CHARS, 0..=15));
    s
}

/// BasicAuth の username: [a-zA-Z][a-zA-Z0-9]{0,7}
fn basic_username_alnum(ctx: &mut noprop::TestCaseContext) -> String {
    let mut s = String::with_capacity(8);
    s.push(char_from_bytes(ctx, ALPHA_CHARS));
    s.push_str(&string_from_bytes(ctx, ALNUM_CHARS, 0..=7));
    s
}

/// BasicAuth の password: [a-zA-Z0-9!@#$%^&*]{0,16}
fn basic_password(ctx: &mut noprop::TestCaseContext) -> String {
    string_from_bytes(ctx, PASSWORD_CHARS, 0..=16)
}

/// コロンを含むパスワード
fn password_with_colon(ctx: &mut noprop::TestCaseContext) -> String {
    format!(
        "{}:{}",
        string_from_bytes(ctx, ALNUM_CHARS, 0..=8),
        string_from_bytes(ctx, ALNUM_CHARS, 0..=8)
    )
}

/// quoted-string で囲まれた realm 値 (obs-text 含む)
fn qdtext_realm(ctx: &mut noprop::TestCaseContext) -> String {
    let len = noprop::sample_usize_in(ctx, 1..16);
    let mut s = String::with_capacity(len);
    for _ in 0..len {
        s.push(pbt::qdtext_char(ctx));
    }
    s
}

/// スキーム名のランダムケーシング ("Basic" -> "basic", "BASIC", "bAsIc" など)
fn randomize_case(ctx: &mut noprop::TestCaseContext, scheme: &'static str) -> String {
    scheme
        .chars()
        .map(|c| {
            if noprop::sample_bool(ctx) {
                c.to_uppercase().to_string()
            } else {
                c.to_lowercase().to_string()
            }
        })
        .collect()
}

/// realm: [a-z]{1,8}.[a-z]{2,6}
fn dotted_realm(ctx: &mut noprop::TestCaseContext) -> String {
    let a = noprop::sample_usize_in(ctx, 1..=8);
    let b = noprop::sample_usize_in(ctx, 2..=6);
    let mut s = String::with_capacity(a + 1 + b);
    for _ in 0..a {
        s.push(char::from(b'a' + noprop::sample_usize_in(ctx, 0..26) as u8));
    }
    s.push('.');
    for _ in 0..b {
        s.push(char::from(b'a' + noprop::sample_usize_in(ctx, 0..26) as u8));
    }
    s
}

/// Digest の uri: "/[a-z/]{1,16}"
fn digest_uri(ctx: &mut noprop::TestCaseContext) -> String {
    format!(
        "/{}",
        string_from_bytes(ctx, ALPHA_LOWER_SLASH_CHARS, 1..=16)
    )
}

/// Digest の uri: "/[a-z]{1,8}"
fn digest_uri_simple(ctx: &mut noprop::TestCaseContext) -> String {
    format!("/{}", string_from_bytes(ctx, ALPHA_LOWER_CHARS, 1..=8))
}

/// Digest の response: [a-f0-9]{32}
fn digest_response(ctx: &mut noprop::TestCaseContext) -> String {
    string_from_bytes(ctx, HEX_LOWER_CHARS, 32..=32)
}

// ========================================
// BasicAuth のテスト
// ========================================

/// BasicAuth ラウンドトリップ
#[test]
fn prop_basic_auth_roundtrip() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let username = basic_username(ctx);
        let password = basic_password(ctx);
        let auth = BasicAuth::new(&username, &password)
            .expect("認証ヘッダーのパースは成功するはず (実装バグ)");
        let header = auth.to_header_value();
        let reparsed =
            BasicAuth::parse(&header).expect("認証ヘッダーのパースは成功するはず (実装バグ)");

        assert_eq!(auth.username(), reparsed.username());
        assert_eq!(auth.password(), reparsed.password());
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

/// BasicAuth ラウンドトリップ (コロンを含むパスワード)
#[test]
fn prop_basic_auth_colon_in_password() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let username = basic_username_alnum(ctx);
        let password = password_with_colon(ctx);
        let auth = BasicAuth::new(&username, &password)
            .expect("認証ヘッダーのパースは成功するはず (実装バグ)");
        let header = auth.to_header_value();
        let reparsed =
            BasicAuth::parse(&header).expect("認証ヘッダーのパースは成功するはず (実装バグ)");

        assert_eq!(reparsed.username(), username.as_str());
        assert_eq!(reparsed.password(), password.as_str());
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

/// BasicAuth スキーム名の大文字小文字を区別しない
#[test]
fn prop_basic_auth_case_insensitive() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let scheme = randomize_case(ctx, "Basic");
        let username = basic_username_alnum(ctx);
        let password = basic_password(ctx);
        let auth = BasicAuth::new(&username, &password)
            .expect("認証ヘッダーのパースは成功するはず (実装バグ)");
        let canonical = auth.to_header_value();
        // スキーム名を差し替え
        let header = format!("{} {}", scheme, &canonical["Basic ".len()..]);
        let parsed =
            BasicAuth::parse(&header).expect("認証ヘッダーのパースは成功するはず (実装バグ)");

        assert_eq!(parsed.username(), username.as_str());
        assert_eq!(parsed.password(), password.as_str());
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
// WwwAuthenticate のテスト
// ========================================

/// WwwAuthenticate charset UTF-8 付きラウンドトリップ
#[test]
fn prop_www_authenticate_charset_roundtrip() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let realm = param_value(ctx);
        let auth = WwwAuthenticate::basic(&realm).with_charset_utf8();
        let header = auth.to_string();
        let reparsed =
            WwwAuthenticate::parse(&header).expect("認証ヘッダーのパースは成功するはず (実装バグ)");

        assert_eq!(reparsed.realm(), realm.as_str());
        assert_eq!(reparsed.charset(), Some("UTF-8"));
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

/// WwwAuthenticate スキーム名の大文字小文字を区別しない
#[test]
fn prop_www_authenticate_case_insensitive() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let scheme = randomize_case(ctx, "Basic");
        let realm = param_value(ctx);
        let header = format!("{} realm=\"{}\"", scheme, realm);
        let parsed =
            WwwAuthenticate::parse(&header).expect("認証ヘッダーのパースは成功するはず (実装バグ)");

        assert_eq!(parsed.realm(), realm.as_str());
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
// BearerToken のテスト
// ========================================

/// BearerToken ラウンドトリップ
#[test]
fn prop_bearer_token_roundtrip() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let token = token68_string(ctx, 1, 64);
        let bearer = BearerToken::parse(&format!("Bearer {}", token))
            .expect("認証ヘッダーのパースは成功するはず (実装バグ)");
        let header = bearer.to_header_value();
        let reparsed =
            BearerToken::parse(&header).expect("認証ヘッダーのパースは成功するはず (実装バグ)");

        assert_eq!(bearer.token(), reparsed.token());
        assert_eq!(bearer.token(), token.as_str());
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

/// 大文字小文字を区別しない
#[test]
fn prop_bearer_token_case_insensitive() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let token = token68_string(ctx, 1, 32);
        let lower = BearerToken::parse(&format!("bearer {}", token))
            .expect("認証ヘッダーのパースは成功するはず (実装バグ)");
        let upper = BearerToken::parse(&format!("Bearer {}", token))
            .expect("認証ヘッダーのパースは成功するはず (実装バグ)");

        assert_eq!(lower.token(), upper.token());
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
// BearerChallenge のテスト
// ========================================

/// BearerChallenge ラウンドトリップ
#[test]
fn prop_bearer_challenge_roundtrip() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let realm = param_value(ctx);
        let error = noprop::sample_choice(
            ctx,
            &["invalid_token", "invalid_request", "insufficient_scope"],
        );
        let header = format!("Bearer realm=\"{}\", error=\"{}\"", realm, error);
        let challenge =
            BearerChallenge::parse(&header).expect("認証ヘッダーのパースは成功するはず (実装バグ)");

        assert_eq!(challenge.param("realm"), Some(realm.as_str()));
        assert_eq!(challenge.param("error"), Some(error));
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
// DigestAuth のテスト
// ========================================

/// DigestAuth ラウンドトリップ
#[test]
fn prop_digest_auth_roundtrip() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let username = param_value(ctx);
        let realm = param_value(ctx);
        let nonce = param_value(ctx);
        let uri = digest_uri(ctx);
        let response = digest_response(ctx);
        let header = format!(
            "Digest username=\"{}\", realm=\"{}\", nonce=\"{}\", uri=\"{}\", response=\"{}\"",
            username, realm, nonce, uri, response
        );
        let auth =
            DigestAuth::parse(&header).expect("認証ヘッダーのパースは成功するはず (実装バグ)");

        assert_eq!(auth.username(), Some(username.as_str()));
        assert_eq!(auth.realm(), Some(realm.as_str()));
        assert_eq!(auth.nonce(), Some(nonce.as_str()));
        assert_eq!(auth.uri(), Some(uri.as_str()));
        assert_eq!(auth.response(), Some(response.as_str()));

        // to_header_value で再エンコードできる
        let header_value = auth.to_header_value();
        assert!(header_value.starts_with("Digest "));
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

/// param で大文字小文字を区別しない
#[test]
fn prop_digest_auth_param_case_insensitive() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let username = param_value(ctx);
        let realm = param_value(ctx);
        let nonce = param_value(ctx);
        let uri = digest_uri_simple(ctx);
        let response = digest_response(ctx);
        let header = format!(
            "Digest username=\"{}\", realm=\"{}\", nonce=\"{}\", uri=\"{}\", response=\"{}\"",
            username, realm, nonce, uri, response
        );
        let auth =
            DigestAuth::parse(&header).expect("認証ヘッダーのパースは成功するはず (実装バグ)");

        assert_eq!(auth.param("USERNAME"), Some(username.as_str()));
        assert_eq!(auth.param("REALM"), Some(realm.as_str()));
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
// DigestChallenge のテスト
// ========================================

/// DigestChallenge ラウンドトリップ
#[test]
fn prop_digest_challenge_roundtrip() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let realm = param_value(ctx);
        let nonce = param_value(ctx);
        let header = format!("Digest realm=\"{}\", nonce=\"{}\"", realm, nonce);
        let challenge =
            DigestChallenge::parse(&header).expect("認証ヘッダーのパースは成功するはず (実装バグ)");

        assert_eq!(challenge.realm(), Some(realm.as_str()));
        assert_eq!(challenge.nonce(), Some(nonce.as_str()));

        // to_header_value で再エンコードできる
        let header_value = challenge.to_header_value();
        assert!(header_value.starts_with("Digest "));
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
// Authorization enum のテスト
// ========================================

/// Authorization::Basic ラウンドトリップ
#[test]
fn prop_authorization_basic_roundtrip() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let username = basic_username_alnum(ctx);
        let password = basic_password(ctx);
        let auth = BasicAuth::new(&username, &password)
            .expect("認証ヘッダーのパースは成功するはず (実装バグ)");
        let header = auth.to_header_value();
        let parsed =
            Authorization::parse(&header).expect("認証ヘッダーのパースは成功するはず (実装バグ)");

        if let Authorization::Basic(basic) = parsed {
            assert_eq!(basic.username(), username.as_str());
            assert_eq!(basic.password(), password.as_str());
        } else {
            panic!("Authorization::Basic を期待");
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

/// Authorization::Bearer ラウンドトリップ
#[test]
fn prop_authorization_bearer_roundtrip() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let token = token68_string(ctx, 1, 32);
        let header = format!("Bearer {}", token);
        let parsed =
            Authorization::parse(&header).expect("認証ヘッダーのパースは成功するはず (実装バグ)");

        if let Authorization::Bearer(bearer) = &parsed {
            assert_eq!(bearer.token(), token.as_str());
        } else {
            panic!("Authorization::Bearer を期待");
        }

        // to_header_value
        let header_value = parsed.to_header_value();
        assert_eq!(header_value, format!("Bearer {}", token));
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

/// Authorization::Digest ラウンドトリップ
#[test]
fn prop_authorization_digest_roundtrip() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let username = param_value(ctx);
        let realm = param_value(ctx);
        let nonce = param_value(ctx);
        let uri = digest_uri_simple(ctx);
        let response = digest_response(ctx);
        let header = format!(
            "Digest username=\"{}\", realm=\"{}\", nonce=\"{}\", uri=\"{}\", response=\"{}\"",
            username, realm, nonce, uri, response
        );
        let parsed =
            Authorization::parse(&header).expect("認証ヘッダーのパースは成功するはず (実装バグ)");

        if let Authorization::Digest(digest) = &parsed {
            assert_eq!(digest.username(), Some(username.as_str()));
        } else {
            panic!("Authorization::Digest を期待");
        }

        // to_header_value
        let header_value = parsed.to_header_value();
        assert!(header_value.starts_with("Digest "));
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
// AuthChallenge enum のテスト
// ========================================

/// AuthChallenge::Basic ラウンドトリップ
#[test]
fn prop_auth_challenge_basic_roundtrip() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let realm = param_value(ctx);
        let header = format!("Basic realm=\"{}\"", realm);
        let parsed =
            AuthChallenge::parse(&header).expect("認証ヘッダーのパースは成功するはず (実装バグ)");

        if let AuthChallenge::Basic(basic) = &parsed {
            assert_eq!(basic.realm(), realm.as_str());
        } else {
            panic!("AuthChallenge::Basic を期待");
        }

        // to_header_value
        let header_value = parsed.to_header_value();
        assert!(header_value.starts_with("Basic "));
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

/// AuthChallenge::Bearer ラウンドトリップ
#[test]
fn prop_auth_challenge_bearer_roundtrip() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let realm = param_value(ctx);
        let header = format!("Bearer realm=\"{}\"", realm);
        let parsed =
            AuthChallenge::parse(&header).expect("認証ヘッダーのパースは成功するはず (実装バグ)");

        if let AuthChallenge::Bearer(_) = &parsed {
            // OK
        } else {
            panic!("AuthChallenge::Bearer を期待");
        }

        // to_header_value
        let header_value = parsed.to_header_value();
        assert!(header_value.starts_with("Bearer "));
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

/// AuthChallenge::Digest ラウンドトリップ
#[test]
fn prop_auth_challenge_digest_roundtrip() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let realm = param_value(ctx);
        let nonce = param_value(ctx);
        let header = format!("Digest realm=\"{}\", nonce=\"{}\"", realm, nonce);
        let parsed =
            AuthChallenge::parse(&header).expect("認証ヘッダーのパースは成功するはず (実装バグ)");

        if let AuthChallenge::Digest(digest) = &parsed {
            assert_eq!(digest.realm(), Some(realm.as_str()));
            assert_eq!(digest.nonce(), Some(nonce.as_str()));
        } else {
            panic!("AuthChallenge::Digest を期待");
        }

        // to_header_value
        let header_value = parsed.to_header_value();
        assert!(header_value.starts_with("Digest "));
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
// Authorization スキーム名の大文字小文字を区別しない
// ========================================

/// Authorization::Basic のスキーム名は大文字小文字を区別しない
#[test]
fn prop_authorization_case_insensitive_basic() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let scheme = randomize_case(ctx, "Basic");
        let username = basic_username_alnum(ctx);
        let password = basic_password(ctx);
        let auth = BasicAuth::new(&username, &password)
            .expect("認証ヘッダーのパースは成功するはず (実装バグ)");
        let canonical = auth.to_header_value();
        let header = format!("{} {}", scheme, &canonical["Basic ".len()..]);
        let parsed =
            Authorization::parse(&header).expect("認証ヘッダーのパースは成功するはず (実装バグ)");

        if let Authorization::Basic(basic) = parsed {
            assert_eq!(basic.username(), username.as_str());
        } else {
            panic!("Authorization::Basic を期待");
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

/// Authorization::Bearer のスキーム名は大文字小文字を区別しない
#[test]
fn prop_authorization_case_insensitive_bearer() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let scheme = randomize_case(ctx, "Bearer");
        let token = token68_string(ctx, 1, 32);
        let header = format!("{} {}", scheme, token);
        let parsed =
            Authorization::parse(&header).expect("認証ヘッダーのパースは成功するはず (実装バグ)");

        if let Authorization::Bearer(bearer) = parsed {
            assert_eq!(bearer.token(), token.as_str());
        } else {
            panic!("Authorization::Bearer を期待");
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

/// Authorization::Digest のスキーム名は大文字小文字を区別しない
#[test]
fn prop_authorization_case_insensitive_digest() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let scheme = randomize_case(ctx, "Digest");
        let username = param_value(ctx);
        let realm = param_value(ctx);
        let nonce = param_value(ctx);
        let uri = digest_uri_simple(ctx);
        let response = digest_response(ctx);
        let header = format!(
            "{} username=\"{}\", realm=\"{}\", nonce=\"{}\", uri=\"{}\", response=\"{}\"",
            scheme, username, realm, nonce, uri, response
        );
        let parsed =
            Authorization::parse(&header).expect("認証ヘッダーのパースは成功するはず (実装バグ)");

        if let Authorization::Digest(digest) = parsed {
            assert_eq!(digest.username(), Some(username.as_str()));
        } else {
            panic!("Authorization::Digest を期待");
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

// ========================================
// AuthChallenge スキーム名の大文字小文字を区別しない
// ========================================

/// AuthChallenge::Basic のスキーム名は大文字小文字を区別しない
#[test]
fn prop_auth_challenge_case_insensitive_basic() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let scheme = randomize_case(ctx, "Basic");
        let realm = param_value(ctx);
        let header = format!("{} realm=\"{}\"", scheme, realm);
        let parsed =
            AuthChallenge::parse(&header).expect("認証ヘッダーのパースは成功するはず (実装バグ)");

        if let AuthChallenge::Basic(basic) = parsed {
            assert_eq!(basic.realm(), realm.as_str());
        } else {
            panic!("AuthChallenge::Basic を期待");
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

/// AuthChallenge::Bearer のスキーム名は大文字小文字を区別しない
#[test]
fn prop_auth_challenge_case_insensitive_bearer() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let scheme = randomize_case(ctx, "Bearer");
        let realm = param_value(ctx);
        let header = format!("{} realm=\"{}\"", scheme, realm);
        let parsed =
            AuthChallenge::parse(&header).expect("認証ヘッダーのパースは成功するはず (実装バグ)");

        if let AuthChallenge::Bearer(_) = parsed {
            // OK
        } else {
            panic!("AuthChallenge::Bearer を期待");
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

/// AuthChallenge::Digest のスキーム名は大文字小文字を区別しない
#[test]
fn prop_auth_challenge_case_insensitive_digest() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let scheme = randomize_case(ctx, "Digest");
        let realm = param_value(ctx);
        let nonce = param_value(ctx);
        let header = format!("{} realm=\"{}\", nonce=\"{}\"", scheme, realm, nonce);
        let parsed =
            AuthChallenge::parse(&header).expect("認証ヘッダーのパースは成功するはず (実装バグ)");

        if let AuthChallenge::Digest(digest) = parsed {
            assert_eq!(digest.realm(), Some(realm.as_str()));
            assert_eq!(digest.nonce(), Some(nonce.as_str()));
        } else {
            panic!("AuthChallenge::Digest を期待");
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

// ========================================
// ProxyAuthorization のテスト
// ========================================

/// ProxyAuthorization ラウンドトリップ
#[test]
fn prop_proxy_authorization_roundtrip() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let username = basic_username_alnum(ctx);
        let password = basic_password(ctx);
        let auth = BasicAuth::new(&username, &password)
            .expect("認証ヘッダーのパースは成功するはず (実装バグ)");
        let header = auth.to_header_value();
        let proxy_auth = ProxyAuthorization::parse(&header)
            .expect("認証ヘッダーのパースは成功するはず (実装バグ)");

        if let Authorization::Basic(basic) = proxy_auth.authorization() {
            assert_eq!(basic.username(), username.as_str());
            assert_eq!(basic.password(), password.as_str());
        } else {
            panic!("Authorization::Basic を期待");
        }

        // to_header_value
        let header_value = proxy_auth.to_header_value();
        assert!(header_value.starts_with("Basic "));
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
// ProxyAuthenticate のテスト
// ========================================

/// ProxyAuthenticate ラウンドトリップ
#[test]
fn prop_proxy_authenticate_roundtrip() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let realm = param_value(ctx);
        let header = format!("Basic realm=\"{}\"", realm);
        let proxy_auth = ProxyAuthenticate::parse(&header)
            .expect("認証ヘッダーのパースは成功するはず (実装バグ)");

        if let AuthChallenge::Basic(basic) = proxy_auth.challenge() {
            assert_eq!(basic.realm(), realm.as_str());
        } else {
            panic!("AuthChallenge::Basic を期待");
        }

        // to_header_value
        let header_value = proxy_auth.to_header_value();
        assert!(header_value.starts_with("Basic "));
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
// WwwAuthenticate のテスト
// ========================================

/// WwwAuthenticate ラウンドトリップ
#[test]
fn prop_www_authenticate_roundtrip() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let realm = dotted_realm(ctx);
        let auth = WwwAuthenticate::basic(&realm);
        let header = auth.to_string();
        let reparsed =
            WwwAuthenticate::parse(&header).expect("認証ヘッダーのパースは成功するはず (実装バグ)");

        assert_eq!(auth.realm(), reparsed.realm());
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

/// WwwAuthenticate with charset UTF-8 ラウンドトリップ
#[test]
fn prop_www_authenticate_with_charset_utf8_roundtrip() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let realm = dotted_realm(ctx);
        let auth = WwwAuthenticate::basic(&realm).with_charset_utf8();
        let header = auth.to_string();
        let reparsed =
            WwwAuthenticate::parse(&header).expect("認証ヘッダーのパースは成功するはず (実装バグ)");

        assert_eq!(reparsed.realm(), realm.as_str());
        assert_eq!(reparsed.charset(), Some("UTF-8"));
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
// obs-text Unicode scalar 拡張のラウンドトリップ
// ========================================

/// WwwAuthenticate (Basic realm=...) に obs-text を含む UTF-8 char が含まれても
/// `parse -> to_string -> parse` のラウンドトリップで一致する。
#[test]
fn prop_www_authenticate_obs_text_roundtrip() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let realm = qdtext_realm(ctx);
        let input = format!("Basic realm=\"{}\"", realm);
        let parsed =
            WwwAuthenticate::parse(&input).expect("認証ヘッダーのパースは成功するはず (実装バグ)");
        assert_eq!(parsed.realm(), realm.as_str());

        let displayed = parsed.to_string();
        let reparsed = WwwAuthenticate::parse(&displayed)
            .expect("認証ヘッダーのパースは成功するはず (実装バグ)");
        assert_eq!(reparsed.realm(), realm.as_str());
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

/// DigestChallenge の realm / nonce に obs-text を含む UTF-8 char が含まれても
/// ラウンドトリップで一致する。
#[test]
fn prop_digest_challenge_obs_text_roundtrip() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let realm = qdtext_realm(ctx);
        let nonce = qdtext_realm(ctx);
        let input = format!("Digest realm=\"{}\", nonce=\"{}\"", realm, nonce);
        let parsed =
            DigestChallenge::parse(&input).expect("認証ヘッダーのパースは成功するはず (実装バグ)");
        assert_eq!(parsed.realm(), Some(realm.as_str()));
        assert_eq!(parsed.nonce(), Some(nonce.as_str()));

        let displayed = parsed.to_header_value();
        let reparsed = DigestChallenge::parse(&displayed)
            .expect("認証ヘッダーのパースは成功するはず (実装バグ)");
        assert_eq!(reparsed.realm(), Some(realm.as_str()));
        assert_eq!(reparsed.nonce(), Some(nonce.as_str()));
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
// auth-param の hard cap
// ========================================

/// 33..=200 個のパラメータは `TooManyParameters` を返す
#[test]
fn prop_auth_challenge_too_many_params() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let count = noprop::sample_usize_in(ctx, 33..=200);
        let mut s = String::from("Bearer ");
        for i in 0..count {
            if i > 0 {
                s.push_str(", ");
            }
            s.push_str(&format!("p{}=\"v\"", i));
        }
        let result = AuthChallenge::parse(&s);
        assert!(matches!(
            result,
            Err(shiguredo_http11::auth::AuthError::TooManyParameters)
        ));
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

/// 1..=32 個のパラメータは正常に parse される
#[test]
fn prop_auth_challenge_at_most_32_params_ok() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let count = noprop::sample_usize_in(ctx, 1..=32);
        let mut s = String::from("Bearer ");
        for i in 0..count {
            if i > 0 {
                s.push_str(", ");
            }
            s.push_str(&format!("p{}=\"v\"", i));
        }
        let result = AuthChallenge::parse(&s);
        assert!(result.is_ok(), "count={}: {:?}", count, result);
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
