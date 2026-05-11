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
        (
            AuthError::ConflictingUsernameField,
            "both username and username* present (RFC 7616 Section 3.4)",
        ),
        (
            AuthError::InvalidUsernameExtValue,
            "invalid username* ext-value",
        ),
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

// ========================================
// auth-param カンマ区切り必須 (RFC 9110 Section 11.2)
// ========================================

#[test]
fn test_digest_challenge_params_without_comma_error() {
    // カンマなしの複数パラメータは不正
    // RFC 9110: auth-param *( OWS "," OWS auth-param )
    assert!(matches!(
        DigestChallenge::parse("Digest realm=\"test\" nonce=\"abc\""),
        Err(AuthError::InvalidParameter)
    ));
}

#[test]
fn test_digest_challenge_params_with_comma_ok() {
    // カンマ区切りの複数パラメータは正常
    let result = DigestChallenge::parse("Digest realm=\"test\", nonce=\"abc\"");
    assert!(result.is_ok());
}

// ========================================
// quoted-string / quoted-pair の CTL 拒否 (RFC 9110 Section 5.6.4)
// ========================================

/// RFC 9110 §5.6.4: quoted-pair = "\" ( HTAB / SP / VCHAR / obs-text )
/// CR / LF / NUL を escape したものは reject する
#[test]
fn test_auth_quoted_pair_rejects_crlf() {
    let input = "Basic realm=\"a\\\rb\"";
    let result = BasicAuth::parse(input);
    assert!(
        result.is_err(),
        "quoted-pair で CR を escape したものは reject されるべき"
    );

    let input = "Basic realm=\"a\\\nb\"";
    let result = BasicAuth::parse(input);
    assert!(
        result.is_err(),
        "quoted-pair で LF を escape したものは reject されるべき"
    );
}

/// RFC 9110 §5.6.4: qdtext に CR / LF を含むものは reject する
#[test]
fn test_auth_qdtext_rejects_crlf() {
    let input = "Basic realm=\"a\rb\"";
    let result = BasicAuth::parse(input);
    assert!(result.is_err(), "qdtext に CR を含むものは reject される");

    let input = "Basic realm=\"a\nb\"";
    let result = BasicAuth::parse(input);
    assert!(result.is_err(), "qdtext に LF を含むものは reject される");
}

// ========================================
// RFC 7616 Section 3.4: DigestAuth username* テスト
// ========================================

/// `username*` で UTF-8 ユーザー名 (RFC 5987 ext-value) を受理する
#[test]
fn test_digest_auth_accepts_username_star_with_utf8() {
    // `ユーザ` を UTF-8 percent-encoded した値
    let input = "Digest username*=\"UTF-8''%E3%83%A6%E3%83%BC%E3%82%B6\", \
                 realm=\"r\", nonce=\"n\", uri=\"/\", response=\"resp\"";
    let auth = DigestAuth::parse(input).expect("username* (UTF-8) は受理される想定");

    // username (ASCII) は None、username_decoded で UTF-8 が取れる
    assert_eq!(auth.username(), None);
    assert_eq!(auth.username_decoded().as_deref(), Some("ユーザ"));
}

/// `username` (ASCII) のみは引き続き受理される
#[test]
fn test_digest_auth_accepts_username_ascii_only() {
    let input = "Digest username=\"alice\", realm=\"r\", nonce=\"n\", uri=\"/\", response=\"resp\"";
    let auth = DigestAuth::parse(input).unwrap();
    assert_eq!(auth.username(), Some("alice"));
    assert_eq!(auth.username_decoded().as_deref(), Some("alice"));
}

/// `username` と `username*` が両方ある場合は ConflictingUsernameField で reject (RFC 7616 §3.4 MUST NOT)
#[test]
fn test_digest_auth_rejects_both_username_and_star() {
    let input = "Digest username=\"alice\", username*=\"UTF-8''alice\", \
                 realm=\"r\", nonce=\"n\", uri=\"/\", response=\"resp\"";
    let result = DigestAuth::parse(input);
    assert!(
        matches!(result, Err(AuthError::ConflictingUsernameField)),
        "username と username* の両方は reject される想定。actual = {:?}",
        result
    );
}

/// `username` も `username*` もない場合は MissingParameter で reject
#[test]
fn test_digest_auth_rejects_missing_both_username_fields() {
    let input = "Digest realm=\"r\", nonce=\"n\", uri=\"/\", response=\"resp\"";
    let result = DigestAuth::parse(input);
    assert!(
        matches!(result, Err(AuthError::MissingParameter)),
        "username / username* のどちらも無い場合は reject 想定。actual = {:?}",
        result
    );
}

/// `username*` の charset が UTF-8 以外は InvalidUsernameExtValue で reject (RFC 7616 §3.4)
#[test]
fn test_digest_auth_rejects_username_star_non_utf8_charset() {
    let input = "Digest username*=\"ISO-8859-1''alice\", \
                 realm=\"r\", nonce=\"n\", uri=\"/\", response=\"resp\"";
    let result = DigestAuth::parse(input);
    assert!(
        matches!(result, Err(AuthError::InvalidUsernameExtValue)),
        "UTF-8 以外の charset は reject 想定。actual = {:?}",
        result
    );
}

/// `username*` の ext-value 形式が不正 (シングルクォート欠落) は reject
#[test]
fn test_digest_auth_rejects_username_star_invalid_ext_value() {
    let input = "Digest username*=\"alice\", \
                 realm=\"r\", nonce=\"n\", uri=\"/\", response=\"resp\"";
    let result = DigestAuth::parse(input);
    assert!(
        matches!(result, Err(AuthError::InvalidUsernameExtValue)),
        "ext-value 形式不正 (シングルクォート欠落) は reject 想定。actual = {:?}",
        result
    );
}
