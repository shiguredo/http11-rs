//! HTTP 認証のユニットテスト

use shiguredo_http11::auth::{
    AuthChallenge, AuthError, Authorization, BasicAuth, BearerToken, DigestAuth, DigestChallenge,
};

// ========================================
// AuthError のテスト
// ========================================

#[test]
fn test_auth_error_display() {
    let errors = [
        (AuthError::Empty, "empty authorization header"),
        (AuthError::InvalidFormat, "invalid authorization format"),
        (AuthError::NotBasicScheme, "not basic authentication scheme"),
        (
            AuthError::NotDigestScheme,
            "not digest authentication scheme",
        ),
        (
            AuthError::NotBearerScheme,
            "not bearer authentication scheme",
        ),
        (AuthError::Base64DecodeError, "base64 decode error"),
        (AuthError::Utf8Error, "utf-8 decode error"),
        (AuthError::MissingColon, "missing colon in credentials"),
        (AuthError::InvalidParameter, "invalid auth parameter"),
        (
            AuthError::MissingParameter,
            "missing required auth parameter",
        ),
        (AuthError::InvalidToken, "invalid auth token"),
        (AuthError::ColonInUserId, "colon in user-id"),
        (
            AuthError::ControlCharacter,
            "control character in credentials",
        ),
        (AuthError::InvalidCharset, "charset must be UTF-8"),
    ];

    for (error, expected) in errors {
        assert_eq!(error.to_string(), expected);
    }
}

// ========================================
// BearerToken のテスト
// ========================================

// BearerToken パースエラー
#[test]
fn test_bearer_token_parse_errors() {
    // 空
    assert!(matches!(BearerToken::parse(""), Err(AuthError::Empty)));
    // Bearer スキームでない
    assert!(matches!(
        BearerToken::parse("Basic abc"),
        Err(AuthError::NotBearerScheme)
    ));
    // トークンが空
    assert!(matches!(
        BearerToken::parse("Bearer "),
        Err(AuthError::InvalidFormat)
    ));
    // "Bearer" のみ（スペースなし）
    assert!(matches!(
        BearerToken::parse("Bearer"),
        Err(AuthError::InvalidFormat)
    ));
    // 不正な文字を含む
    assert!(matches!(
        BearerToken::parse("Bearer abc def"),
        Err(AuthError::InvalidToken)
    ));
}

// ========================================
// DigestAuth のテスト
// ========================================

// DigestAuth パースエラー
#[test]
fn test_digest_auth_parse_errors() {
    // 空
    assert!(matches!(DigestAuth::parse(""), Err(AuthError::Empty)));
    // Digest スキームでない
    assert!(matches!(
        DigestAuth::parse("Basic abc"),
        Err(AuthError::NotDigestScheme)
    ));
    // 必須パラメータが足りない
    assert!(matches!(
        DigestAuth::parse("Digest username=\"test\""),
        Err(AuthError::MissingParameter)
    ));
}

// ========================================
// DigestChallenge のテスト
// ========================================

// DigestChallenge パースエラー
#[test]
fn test_digest_challenge_parse_errors() {
    // 空
    assert!(matches!(DigestChallenge::parse(""), Err(AuthError::Empty)));
    // Digest スキームでない
    assert!(matches!(
        DigestChallenge::parse("Basic realm=\"test\""),
        Err(AuthError::NotDigestScheme)
    ));
    // 必須パラメータが足りない
    assert!(matches!(
        DigestChallenge::parse("Digest realm=\"test\""),
        Err(AuthError::MissingParameter)
    ));
}

// ========================================
// Authorization のテスト
// ========================================

// Authorization パースエラー
#[test]
fn test_authorization_parse_errors() {
    // 空
    assert!(matches!(Authorization::parse(""), Err(AuthError::Empty)));
    // 不明なスキーム
    assert!(matches!(
        Authorization::parse("Unknown token"),
        Err(AuthError::InvalidFormat)
    ));
}

// ========================================
// BasicAuth token68 バリデーションのテスト
// ========================================

#[test]
fn test_basic_auth_token68_internal_whitespace_rejected() {
    // RFC 7617 Section 2: credentials は token68 形式
    // 内部の空白は token68 で不正
    assert!(matches!(
        BasicAuth::parse("Basic ab cd"),
        Err(AuthError::InvalidToken)
    ));
    assert!(matches!(
        BasicAuth::parse("Basic ab\tcd"),
        Err(AuthError::InvalidToken)
    ));
}

// ========================================
// AuthChallenge のテスト
// ========================================

// AuthChallenge パースエラー
#[test]
fn test_auth_challenge_parse_errors() {
    // 空
    assert!(matches!(AuthChallenge::parse(""), Err(AuthError::Empty)));
    // 不明なスキーム
    assert!(matches!(
        AuthChallenge::parse("Unknown param=\"value\""),
        Err(AuthError::InvalidFormat)
    ));
}
