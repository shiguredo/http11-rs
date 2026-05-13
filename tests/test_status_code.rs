//! StatusCode 型のユニットテスト
//!
//! IANA HTTP Status Code Registry に登録された const 定数の値、
//! `code()` / `canonical_reason()` / `from_code()` の挙動を検証する。
//! ラウンドトリップ系の検証は PBT (`prop_response.rs`) に任せ、
//! ここでは const 値そのものの不変条件と境界値を担保する。

use shiguredo_http11::{StatusClass, StatusCode};

// 全 const 定数の網羅リスト
// 追加・変更時は本リストにも反映する。
const ALL_STATUS_CODES: &[StatusCode] = &[
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
];

#[test]
fn test_status_code_representative_values() {
    // 代表値の code() / canonical_reason() を確認
    assert_eq!(StatusCode::OK.code(), 200);
    assert_eq!(StatusCode::OK.canonical_reason(), "OK");

    assert_eq!(StatusCode::NOT_FOUND.code(), 404);
    assert_eq!(StatusCode::NOT_FOUND.canonical_reason(), "Not Found");

    assert_eq!(StatusCode::INTERNAL_SERVER_ERROR.code(), 500);
    assert_eq!(
        StatusCode::INTERNAL_SERVER_ERROR.canonical_reason(),
        "Internal Server Error"
    );

    // RFC 2324 / RFC 7168: アポストロフィを含む reason phrase
    assert_eq!(StatusCode::IM_A_TEAPOT.code(), 418);
    assert_eq!(StatusCode::IM_A_TEAPOT.canonical_reason(), "I'm a teapot");

    // 範囲境界 (1xx 最小、5xx 最大)
    assert_eq!(StatusCode::CONTINUE.code(), 100);
    assert_eq!(StatusCode::NETWORK_AUTHENTICATION_REQUIRED.code(), 511);
}

#[test]
fn test_status_code_all_codes_in_valid_range() {
    // 全 const 定義の code() が RFC 9110 Section 15 の 100..=599 に収まる
    for status in ALL_STATUS_CODES {
        let code = status.code();
        assert!(
            (100..=599).contains(&code),
            "status code {} is out of range 100..=599",
            code
        );
    }
}

#[test]
fn test_status_code_all_canonical_reasons_non_empty() {
    // 全 const 定義の canonical_reason() が空文字列でない (RFC 9112 Section 4 の
    // reason-phrase = 1*( HTAB / SP / VCHAR / obs-text ) を満たす最低条件)
    for status in ALL_STATUS_CODES {
        let reason = status.canonical_reason();
        assert!(
            !reason.is_empty(),
            "canonical_reason for code {} is empty",
            status.code()
        );
    }
}

#[test]
fn test_status_code_no_duplicate_codes() {
    // 全 const 定義の code() に重複がない
    let mut codes: Vec<u16> = ALL_STATUS_CODES.iter().map(|s| s.code()).collect();
    codes.sort_unstable();
    let len_before = codes.len();
    codes.dedup();
    assert_eq!(
        codes.len(),
        len_before,
        "duplicate status code constant detected"
    );
}

#[test]
fn test_status_code_eq_and_copy() {
    // StatusCode は Copy / PartialEq / Eq を派生
    let a = StatusCode::OK;
    let b = StatusCode::OK;
    assert_eq!(a, b);
    let _c = a; // Copy で move されない
    let _d = a;
    assert_ne!(StatusCode::OK, StatusCode::CREATED);
}

#[test]
fn test_status_code_from_code_known() {
    // IANA 登録済みコードは Some(StatusCode) を返す
    assert_eq!(StatusCode::from_code(200), Some(StatusCode::OK));
    assert_eq!(StatusCode::from_code(404), Some(StatusCode::NOT_FOUND));
    assert_eq!(
        StatusCode::from_code(500),
        Some(StatusCode::INTERNAL_SERVER_ERROR)
    );
    assert_eq!(StatusCode::from_code(418), Some(StatusCode::IM_A_TEAPOT));
    // 範囲境界
    assert_eq!(StatusCode::from_code(100), Some(StatusCode::CONTINUE));
    assert_eq!(
        StatusCode::from_code(511),
        Some(StatusCode::NETWORK_AUTHENTICATION_REQUIRED)
    );
}

#[test]
fn test_status_code_from_code_unknown_returns_none() {
    // IANA 未登録コードは None を返す
    // 未割当値
    assert_eq!(StatusCode::from_code(306), None); // 306 (Unused)
    assert_eq!(StatusCode::from_code(309), None);
    assert_eq!(StatusCode::from_code(420), None);
    assert_eq!(StatusCode::from_code(509), None);
    // 範囲外
    assert_eq!(StatusCode::from_code(0), None);
    assert_eq!(StatusCode::from_code(99), None);
    assert_eq!(StatusCode::from_code(600), None);
    assert_eq!(StatusCode::from_code(999), None);
    assert_eq!(StatusCode::from_code(u16::MAX), None);
}

#[test]
fn test_status_class_from_status_code_boundaries() {
    // RFC 9110 Section 15 の各クラス境界値を検証
    assert_eq!(StatusClass::from_status_code(0), None);
    assert_eq!(StatusClass::from_status_code(99), None);
    assert_eq!(
        StatusClass::from_status_code(100),
        Some(StatusClass::Informational)
    );
    assert_eq!(
        StatusClass::from_status_code(199),
        Some(StatusClass::Informational)
    );
    assert_eq!(
        StatusClass::from_status_code(200),
        Some(StatusClass::Successful)
    );
    assert_eq!(
        StatusClass::from_status_code(299),
        Some(StatusClass::Successful)
    );
    assert_eq!(
        StatusClass::from_status_code(300),
        Some(StatusClass::Redirection)
    );
    assert_eq!(
        StatusClass::from_status_code(399),
        Some(StatusClass::Redirection)
    );
    assert_eq!(
        StatusClass::from_status_code(400),
        Some(StatusClass::ClientError)
    );
    assert_eq!(
        StatusClass::from_status_code(499),
        Some(StatusClass::ClientError)
    );
    assert_eq!(
        StatusClass::from_status_code(500),
        Some(StatusClass::ServerError)
    );
    assert_eq!(
        StatusClass::from_status_code(599),
        Some(StatusClass::ServerError)
    );
    assert_eq!(StatusClass::from_status_code(600), None);
    assert_eq!(StatusClass::from_status_code(u16::MAX), None);
}

#[test]
fn test_status_code_class_representative() {
    // 主要 StatusCode のクラス分類を検証
    assert_eq!(StatusCode::CONTINUE.class(), StatusClass::Informational);
    assert_eq!(StatusCode::OK.class(), StatusClass::Successful);
    assert_eq!(StatusCode::NOT_MODIFIED.class(), StatusClass::Redirection);
    assert_eq!(StatusCode::NOT_FOUND.class(), StatusClass::ClientError);
    assert_eq!(
        StatusCode::INTERNAL_SERVER_ERROR.class(),
        StatusClass::ServerError
    );
}

#[test]
fn test_status_code_from_code_roundtrip_for_all_constants() {
    // 全 const 定義について from_code(code()) が同じ StatusCode を返す
    for status in ALL_STATUS_CODES {
        let recovered =
            StatusCode::from_code(status.code()).expect("登録済みコードは復元できるべき");
        assert_eq!(
            recovered.code(),
            status.code(),
            "from_code({}) が異なる StatusCode を返した",
            status.code()
        );
        assert_eq!(
            recovered.canonical_reason(),
            status.canonical_reason(),
            "from_code({}) が異なる canonical_reason の StatusCode を返した",
            status.code()
        );
    }
}
