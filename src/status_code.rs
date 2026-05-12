//! HTTP ステータスコード型
//!
//! RFC 9110 Section 15 で定義されたステータスコードと、それに対応する
//! IANA HTTP Status Code Registry の canonical reason phrase を const 値として保持する。
//!
//! 任意のステータスコード (拡張、私的ステータスコード等) を使いたい場合は
//! `Response::new(code, reason)` または `Response::with_version(version, code, reason)` を
//! 使用すること。本型は IANA 登録済み code 専用の const 表現を提供する。

use core::num::NonZeroU16;

/// HTTP ステータスコード
///
/// `code` は RFC 9110 Section 15 の 100..=599 範囲内であることを保証する
/// (本型を経由した構築でのみ生成可能)。
/// `canonical_reason` は IANA HTTP Status Code Registry に登録された reason phrase。
///
/// `NonZeroU16` でラップしているのはニッチ最適化のため
/// (`Option<StatusCode>` 等が同じサイズで表現可能)。`code` が 0 になることは
/// `new_const` の assert! で静的に弾かれるため、不変条件は破られない。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct StatusCode {
    code: NonZeroU16,
    canonical_reason: &'static str,
}

impl StatusCode {
    // 1xx Informational (RFC 9110 Section 15.2)
    /// `100 Continue` (RFC 9110 Section 15.2.1)
    pub const CONTINUE: Self = Self::new_const(100, "Continue");
    /// `101 Switching Protocols` (RFC 9110 Section 15.2.2)
    pub const SWITCHING_PROTOCOLS: Self = Self::new_const(101, "Switching Protocols");
    /// `102 Processing` (RFC 2518 Section 10.1, WebDAV)
    pub const PROCESSING: Self = Self::new_const(102, "Processing");
    /// `103 Early Hints` (RFC 8297 Section 2)
    pub const EARLY_HINTS: Self = Self::new_const(103, "Early Hints");

    // 2xx Successful (RFC 9110 Section 15.3)
    /// `200 OK` (RFC 9110 Section 15.3.1)
    pub const OK: Self = Self::new_const(200, "OK");
    /// `201 Created` (RFC 9110 Section 15.3.2)
    pub const CREATED: Self = Self::new_const(201, "Created");
    /// `202 Accepted` (RFC 9110 Section 15.3.3)
    pub const ACCEPTED: Self = Self::new_const(202, "Accepted");
    /// `203 Non-Authoritative Information` (RFC 9110 Section 15.3.4)
    pub const NON_AUTHORITATIVE_INFORMATION: Self =
        Self::new_const(203, "Non-Authoritative Information");
    /// `204 No Content` (RFC 9110 Section 15.3.5)
    pub const NO_CONTENT: Self = Self::new_const(204, "No Content");
    /// `205 Reset Content` (RFC 9110 Section 15.3.6)
    pub const RESET_CONTENT: Self = Self::new_const(205, "Reset Content");
    /// `206 Partial Content` (RFC 9110 Section 15.3.7)
    pub const PARTIAL_CONTENT: Self = Self::new_const(206, "Partial Content");
    /// `207 Multi-Status` (RFC 4918 Section 11.1, WebDAV)
    pub const MULTI_STATUS: Self = Self::new_const(207, "Multi-Status");
    /// `208 Already Reported` (RFC 5842 Section 7.1, WebDAV binding)
    pub const ALREADY_REPORTED: Self = Self::new_const(208, "Already Reported");
    /// `226 IM Used` (RFC 3229 Section 10.4.1, Delta encoding)
    pub const IM_USED: Self = Self::new_const(226, "IM Used");

    // 3xx Redirection (RFC 9110 Section 15.4)
    /// `300 Multiple Choices` (RFC 9110 Section 15.4.1)
    pub const MULTIPLE_CHOICES: Self = Self::new_const(300, "Multiple Choices");
    /// `301 Moved Permanently` (RFC 9110 Section 15.4.2)
    pub const MOVED_PERMANENTLY: Self = Self::new_const(301, "Moved Permanently");
    /// `302 Found` (RFC 9110 Section 15.4.3)
    pub const FOUND: Self = Self::new_const(302, "Found");
    /// `303 See Other` (RFC 9110 Section 15.4.4)
    pub const SEE_OTHER: Self = Self::new_const(303, "See Other");
    /// `304 Not Modified` (RFC 9110 Section 15.4.5)
    pub const NOT_MODIFIED: Self = Self::new_const(304, "Not Modified");
    /// `305 Use Proxy` (RFC 9110 Section 15.4.6, deprecated)
    pub const USE_PROXY: Self = Self::new_const(305, "Use Proxy");
    /// `307 Temporary Redirect` (RFC 9110 Section 15.4.8)
    pub const TEMPORARY_REDIRECT: Self = Self::new_const(307, "Temporary Redirect");
    /// `308 Permanent Redirect` (RFC 9110 Section 15.4.9)
    pub const PERMANENT_REDIRECT: Self = Self::new_const(308, "Permanent Redirect");

    // 4xx Client Error (RFC 9110 Section 15.5)
    /// `400 Bad Request` (RFC 9110 Section 15.5.1)
    pub const BAD_REQUEST: Self = Self::new_const(400, "Bad Request");
    /// `401 Unauthorized` (RFC 9110 Section 15.5.2)
    pub const UNAUTHORIZED: Self = Self::new_const(401, "Unauthorized");
    /// `402 Payment Required` (RFC 9110 Section 15.5.3)
    pub const PAYMENT_REQUIRED: Self = Self::new_const(402, "Payment Required");
    /// `403 Forbidden` (RFC 9110 Section 15.5.4)
    pub const FORBIDDEN: Self = Self::new_const(403, "Forbidden");
    /// `404 Not Found` (RFC 9110 Section 15.5.5)
    pub const NOT_FOUND: Self = Self::new_const(404, "Not Found");
    /// `405 Method Not Allowed` (RFC 9110 Section 15.5.6)
    pub const METHOD_NOT_ALLOWED: Self = Self::new_const(405, "Method Not Allowed");
    /// `406 Not Acceptable` (RFC 9110 Section 15.5.7)
    pub const NOT_ACCEPTABLE: Self = Self::new_const(406, "Not Acceptable");
    /// `407 Proxy Authentication Required` (RFC 9110 Section 15.5.8)
    pub const PROXY_AUTHENTICATION_REQUIRED: Self =
        Self::new_const(407, "Proxy Authentication Required");
    /// `408 Request Timeout` (RFC 9110 Section 15.5.9)
    pub const REQUEST_TIMEOUT: Self = Self::new_const(408, "Request Timeout");
    /// `409 Conflict` (RFC 9110 Section 15.5.10)
    pub const CONFLICT: Self = Self::new_const(409, "Conflict");
    /// `410 Gone` (RFC 9110 Section 15.5.11)
    pub const GONE: Self = Self::new_const(410, "Gone");
    /// `411 Length Required` (RFC 9110 Section 15.5.12)
    pub const LENGTH_REQUIRED: Self = Self::new_const(411, "Length Required");
    /// `412 Precondition Failed` (RFC 9110 Section 15.5.13)
    pub const PRECONDITION_FAILED: Self = Self::new_const(412, "Precondition Failed");
    /// `413 Content Too Large` (RFC 9110 Section 15.5.14)
    pub const CONTENT_TOO_LARGE: Self = Self::new_const(413, "Content Too Large");
    /// `414 URI Too Long` (RFC 9110 Section 15.5.15)
    pub const URI_TOO_LONG: Self = Self::new_const(414, "URI Too Long");
    /// `415 Unsupported Media Type` (RFC 9110 Section 15.5.16)
    pub const UNSUPPORTED_MEDIA_TYPE: Self = Self::new_const(415, "Unsupported Media Type");
    /// `416 Range Not Satisfiable` (RFC 9110 Section 15.5.17)
    pub const RANGE_NOT_SATISFIABLE: Self = Self::new_const(416, "Range Not Satisfiable");
    /// `417 Expectation Failed` (RFC 9110 Section 15.5.18)
    pub const EXPECTATION_FAILED: Self = Self::new_const(417, "Expectation Failed");
    /// `418 I'm a teapot` (RFC 2324 Section 2.3.2 / RFC 7168 Section 2.3.3)
    pub const IM_A_TEAPOT: Self = Self::new_const(418, "I'm a teapot");
    /// `421 Misdirected Request` (RFC 9110 Section 15.5.20)
    pub const MISDIRECTED_REQUEST: Self = Self::new_const(421, "Misdirected Request");
    /// `422 Unprocessable Content` (RFC 9110 Section 15.5.21)
    pub const UNPROCESSABLE_CONTENT: Self = Self::new_const(422, "Unprocessable Content");
    /// `423 Locked` (RFC 4918 Section 11.3, WebDAV)
    pub const LOCKED: Self = Self::new_const(423, "Locked");
    /// `424 Failed Dependency` (RFC 4918 Section 11.4, WebDAV)
    pub const FAILED_DEPENDENCY: Self = Self::new_const(424, "Failed Dependency");
    /// `425 Too Early` (RFC 8470 Section 5.2)
    pub const TOO_EARLY: Self = Self::new_const(425, "Too Early");
    /// `426 Upgrade Required` (RFC 9110 Section 15.5.22)
    pub const UPGRADE_REQUIRED: Self = Self::new_const(426, "Upgrade Required");
    /// `428 Precondition Required` (RFC 6585 Section 3)
    pub const PRECONDITION_REQUIRED: Self = Self::new_const(428, "Precondition Required");
    /// `429 Too Many Requests` (RFC 6585 Section 4)
    pub const TOO_MANY_REQUESTS: Self = Self::new_const(429, "Too Many Requests");
    /// `431 Request Header Fields Too Large` (RFC 6585 Section 5)
    pub const REQUEST_HEADER_FIELDS_TOO_LARGE: Self =
        Self::new_const(431, "Request Header Fields Too Large");
    /// `451 Unavailable For Legal Reasons` (RFC 7725 Section 3)
    pub const UNAVAILABLE_FOR_LEGAL_REASONS: Self =
        Self::new_const(451, "Unavailable For Legal Reasons");

    // 5xx Server Error (RFC 9110 Section 15.6)
    /// `500 Internal Server Error` (RFC 9110 Section 15.6.1)
    pub const INTERNAL_SERVER_ERROR: Self = Self::new_const(500, "Internal Server Error");
    /// `501 Not Implemented` (RFC 9110 Section 15.6.2)
    pub const NOT_IMPLEMENTED: Self = Self::new_const(501, "Not Implemented");
    /// `502 Bad Gateway` (RFC 9110 Section 15.6.3)
    pub const BAD_GATEWAY: Self = Self::new_const(502, "Bad Gateway");
    /// `503 Service Unavailable` (RFC 9110 Section 15.6.4)
    pub const SERVICE_UNAVAILABLE: Self = Self::new_const(503, "Service Unavailable");
    /// `504 Gateway Timeout` (RFC 9110 Section 15.6.5)
    pub const GATEWAY_TIMEOUT: Self = Self::new_const(504, "Gateway Timeout");
    /// `505 HTTP Version Not Supported` (RFC 9110 Section 15.6.6)
    pub const HTTP_VERSION_NOT_SUPPORTED: Self = Self::new_const(505, "HTTP Version Not Supported");
    /// `506 Variant Also Negotiates` (RFC 2295 Section 8.1)
    pub const VARIANT_ALSO_NEGOTIATES: Self = Self::new_const(506, "Variant Also Negotiates");
    /// `507 Insufficient Storage` (RFC 4918 Section 11.5, WebDAV)
    pub const INSUFFICIENT_STORAGE: Self = Self::new_const(507, "Insufficient Storage");
    /// `508 Loop Detected` (RFC 5842 Section 7.2, WebDAV binding)
    pub const LOOP_DETECTED: Self = Self::new_const(508, "Loop Detected");
    /// `510 Not Extended` (RFC 2774 Section 7、廃止 RFC だが IANA 登録は残存)
    pub const NOT_EXTENDED: Self = Self::new_const(510, "Not Extended");
    /// `511 Network Authentication Required` (RFC 6585 Section 6)
    pub const NETWORK_AUTHENTICATION_REQUIRED: Self =
        Self::new_const(511, "Network Authentication Required");

    /// const コンテキスト用の内部コンストラクタ
    ///
    /// 100..=599 範囲外を渡すと const 評価で panic する (定数定義時に
    /// コンパイルエラーになる) ため、新しい定数を追加した際の typo は
    /// 静的に検出できる。
    const fn new_const(code: u16, canonical_reason: &'static str) -> Self {
        assert!(
            code >= 100 && code <= 599,
            "status code must be in 100..=599 (RFC 9110 Section 15)"
        );
        // 上の assert で 100..=599 を保証済みのため非ゼロ。`NonZeroU16::new`
        // が `Some` を返さないケースは到達不能。
        let code = match NonZeroU16::new(code) {
            Some(c) => c,
            None => panic!("unreachable: status code is non-zero"),
        };
        Self {
            code,
            canonical_reason,
        }
    }

    /// ステータスコード値を取得
    pub const fn code(&self) -> u16 {
        self.code.get()
    }

    /// IANA 登録の canonical reason phrase を取得
    ///
    /// 注: この値は HTTP の文脈での reason phrase である。RTSP 等のプロトコルでは
    /// 異なる reason phrase を持つ可能性があるため、クロスプロトコル利用時は
    /// 注意すること。
    pub const fn canonical_reason(&self) -> &'static str {
        self.canonical_reason
    }

    /// ステータスコード値から `StatusCode` を逆引き
    ///
    /// IANA 登録済みコードは `Some(StatusCode)` を、未登録コード (一時登録 /
    /// 私的拡張 / 範囲外を含む) は `None` を返す。
    ///
    /// 任意の status code を扱いたい場合は `Response::new(code, reason)` を
    /// 使うこと。
    ///
    /// デコーダーから得た `u16` 値を `StatusCode` に変換するユースケースを想定する。
    pub const fn from_code(code: u16) -> Option<Self> {
        // match を使って const fn として実装する。HashMap は const コンテキストで
        // 構築できないため、定数集合の数だけ分岐を書く。
        // 並びは `impl StatusCode` の定数定義と一致させる。
        Some(match code {
            // 1xx
            100 => Self::CONTINUE,
            101 => Self::SWITCHING_PROTOCOLS,
            102 => Self::PROCESSING,
            103 => Self::EARLY_HINTS,
            // 2xx
            200 => Self::OK,
            201 => Self::CREATED,
            202 => Self::ACCEPTED,
            203 => Self::NON_AUTHORITATIVE_INFORMATION,
            204 => Self::NO_CONTENT,
            205 => Self::RESET_CONTENT,
            206 => Self::PARTIAL_CONTENT,
            207 => Self::MULTI_STATUS,
            208 => Self::ALREADY_REPORTED,
            226 => Self::IM_USED,
            // 3xx
            300 => Self::MULTIPLE_CHOICES,
            301 => Self::MOVED_PERMANENTLY,
            302 => Self::FOUND,
            303 => Self::SEE_OTHER,
            304 => Self::NOT_MODIFIED,
            305 => Self::USE_PROXY,
            307 => Self::TEMPORARY_REDIRECT,
            308 => Self::PERMANENT_REDIRECT,
            // 4xx
            400 => Self::BAD_REQUEST,
            401 => Self::UNAUTHORIZED,
            402 => Self::PAYMENT_REQUIRED,
            403 => Self::FORBIDDEN,
            404 => Self::NOT_FOUND,
            405 => Self::METHOD_NOT_ALLOWED,
            406 => Self::NOT_ACCEPTABLE,
            407 => Self::PROXY_AUTHENTICATION_REQUIRED,
            408 => Self::REQUEST_TIMEOUT,
            409 => Self::CONFLICT,
            410 => Self::GONE,
            411 => Self::LENGTH_REQUIRED,
            412 => Self::PRECONDITION_FAILED,
            413 => Self::CONTENT_TOO_LARGE,
            414 => Self::URI_TOO_LONG,
            415 => Self::UNSUPPORTED_MEDIA_TYPE,
            416 => Self::RANGE_NOT_SATISFIABLE,
            417 => Self::EXPECTATION_FAILED,
            418 => Self::IM_A_TEAPOT,
            421 => Self::MISDIRECTED_REQUEST,
            422 => Self::UNPROCESSABLE_CONTENT,
            423 => Self::LOCKED,
            424 => Self::FAILED_DEPENDENCY,
            425 => Self::TOO_EARLY,
            426 => Self::UPGRADE_REQUIRED,
            428 => Self::PRECONDITION_REQUIRED,
            429 => Self::TOO_MANY_REQUESTS,
            431 => Self::REQUEST_HEADER_FIELDS_TOO_LARGE,
            451 => Self::UNAVAILABLE_FOR_LEGAL_REASONS,
            // 5xx
            500 => Self::INTERNAL_SERVER_ERROR,
            501 => Self::NOT_IMPLEMENTED,
            502 => Self::BAD_GATEWAY,
            503 => Self::SERVICE_UNAVAILABLE,
            504 => Self::GATEWAY_TIMEOUT,
            505 => Self::HTTP_VERSION_NOT_SUPPORTED,
            506 => Self::VARIANT_ALSO_NEGOTIATES,
            507 => Self::INSUFFICIENT_STORAGE,
            508 => Self::LOOP_DETECTED,
            510 => Self::NOT_EXTENDED,
            511 => Self::NETWORK_AUTHENTICATION_REQUIRED,
            _ => return None,
        })
    }

    /// この `StatusCode` のクラス分類を返す。
    ///
    /// `StatusCode` は構築時に `100..=599` が保証されているため、
    /// 必ず分類が定まる (戻り値は `Option` ではない)。
    #[must_use]
    pub const fn class(&self) -> StatusClass {
        // `code` は `new_const` の assert で 100..=599 が保証されているため
        // `from_status_code` は常に `Some` を返す。`const fn` 内では
        // `Option::expect` / `unreachable!()` のいずれも const 非互換なため、
        // match で明示的に分岐し、到達不能な `None` アームは `panic!()` に留める。
        // Rust 2024 の const fn では `panic!()` が const 互換な唯一の
        // フォールバック手段である。
        match StatusClass::from_status_code(self.code.get()) {
            Some(c) => c,
            None => panic!("StatusCode constraint violation: code out of 100..=599"),
        }
    }
}

/// HTTP ステータスコードのクラス分類 — RFC 9110 Section 15 準拠。
///
/// # 分類表
///
/// | バリアント       | 範囲          | RFC 9110 |
/// |------------------|---------------|----------|
/// | `Informational`  | `100..=199`   | §15.2    |
/// | `Successful`     | `200..=299`   | §15.3    |
/// | `Redirection`    | `300..=399`   | §15.4    |
/// | `ClientError`    | `400..=499`   | §15.5    |
/// | `ServerError`    | `500..=599`   | §15.6    |
///
/// 範囲外 (`0..=99`, `600..=65535`) の値は `from_status_code` で `None` を返す。
/// 本ライブラリ内では `Response` と `ResponseHead` の構築時に
/// `100..=599` のバリデーションが効いているため、これらの型を経由する限り
/// 範囲外の値が到達することはない。
///
/// RFC 9110 Section 15 (lines 6828-6832) は範囲外の status code を受信した
/// クライアントに対して「5xx (Server Error) として扱うべき (SHOULD)」と勧告している。
/// `from_status_code` の `None` は「分類不能」を表現しており、この SHOULD 勧告に
/// 従ったフォールバック (例: `unwrap_or(StatusClass::ServerError)`) は API 利用者の責務。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[non_exhaustive]
pub enum StatusClass {
    /// 1xx Informational — RFC 9110 Section 15.2
    Informational,
    /// 2xx Successful — RFC 9110 Section 15.3
    Successful,
    /// 3xx Redirection — RFC 9110 Section 15.4
    Redirection,
    /// 4xx Client Error — RFC 9110 Section 15.5
    ClientError,
    /// 5xx Server Error — RFC 9110 Section 15.6
    ServerError,
}

impl StatusClass {
    /// `u16` のステータスコードから `StatusClass` を生成する。
    ///
    /// 範囲外の値 (`0..=99`, `600..=65535`) は `None` を返す。
    #[must_use]
    pub const fn from_status_code(code: u16) -> Option<Self> {
        Some(match code {
            100..=199 => StatusClass::Informational,
            200..=299 => StatusClass::Successful,
            300..=399 => StatusClass::Redirection,
            400..=499 => StatusClass::ClientError,
            500..=599 => StatusClass::ServerError,
            _ => return None,
        })
    }
}
