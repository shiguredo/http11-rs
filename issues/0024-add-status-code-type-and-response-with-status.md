# 0024: StatusCode 型を導入し Response::with_status を追加する

Created: 2026-05-06
Model: Opus 4.7

## 概要

IANA HTTP Status Code Registry に登録された status code を const 値として持つ `StatusCode` 型を新設し、`Response::with_status(StatusCode) -> Self` を追加する。`with_status` は引数が完全に const 値で構成されるため infallible (`Result` を返さない)。

純粋追加。既存 API (`Response::new` / `Response::with_version`) は触らない。後方互換あり。

ブランチ名は CLAUDE.md「git ブランチの命名規則」に従い `feature/add-status-code-type` を使用する。

依存関係:
- 本 issue は `0017` (Response フィールド非公開化) の完了後に着手する。`0017` で `Response::new` が `Result<Self, EncodeError>` を返すため、examples / tests / PBT / fuzz では `Response::new(200, "OK").unwrap()` の `.unwrap()` が大量発生している。本 issue でこれらを `Response::with_status(StatusCode::OK)` に書き換え、`.unwrap()` を一掃する。
- `0020` (StatusClass enum) との関係: `StatusClass` は status_code の **範囲分類** (1xx/2xx/3xx/4xx/5xx)、`StatusCode` は **個別 status code とその canonical reason の値** という別概念。両者は独立しており、`StatusCode::OK.code()` を `StatusClass::from_status_code(...)` に渡す形で連携する。
- 0025 (Request 非公開化) と並行構造で `Method` 型 + `Request::with_method` を別 issue で追加予定 (0025 完了後)。

## 根拠

### 問題 1: `Response::new(200, "OK").unwrap()` の冗長性

`0017` 完了後、すべての `Response::new` 呼び出しが `Result<Self, EncodeError>` を返す。しかし 99% のユースケースは IANA で標準化された status code (200 OK, 404 Not Found 等) であり、これらは構築時バリデーションを通過することが静的に確定している。にもかかわらず `.unwrap()` を毎回書く必要があり、API の表面が冗長。

```rust
// 現状 (0017 完了後)
Response::new(200, "OK").unwrap()
    .header("Content-Type", "text/plain").unwrap()
    .body(b"hi".to_vec())
```

const 値で構成される standard case には infallible API を提供すべき。

### 問題 2: status code と canonical reason のペアを毎回手書きする手間

`200 OK`, `404 Not Found`, `500 Internal Server Error` 等の組み合わせは IANA HTTP Status Code Registry で標準化されている。にもかかわらず利用側で毎回手書きするのは:

- typo の余地 (`"Not found"` vs `"Not Found"`)
- IANA の正確な reason phrase を覚える必要
- ローカライゼーションのような誤った理由でカスタム reason を入れがち

を招く。canonical reason をライブラリ側で持てば解消できる。

### 問題 3: エコシステム標準パターンへの追従

Rust HTTP エコシステムの事実上の標準である `http` crate は `StatusCode` 型を持ち、`StatusCode::OK` 等の const 値で参照できる。本ライブラリでも同等の API を提供することで、エコシステム慣用に沿う。

## 対応方針

### 影響範囲一覧

| ファイル | 種別 | 内容 |
|---|---|---|
| `src/status_code.rs` | **新規作成** | `StatusCode` 型の定義 + IANA 登録 status code の const 値 |
| `src/lib.rs` | 修正 | `mod status_code;` を追加し `pub use` ブロックに `StatusCode` を追加 |
| `src/response.rs` | 修正 | `pub fn with_status(status: StatusCode) -> Self` を追加 |
| `tests/test_response.rs` | 修正 | `with_status` の動作確認テストを追加 (canonical reason が正しく設定されること、infallible で構築できること) |
| `pbt/tests/prop_response.rs` | 修正 | `with_status` の PBT を追加 (任意の `StatusCode` 定数で構築 → アクセサで code/reason が一致することを検証) |
| `examples/http11_server/src/main.rs` | 修正 | `Response::new(200, "OK").unwrap()` を `Response::with_status(StatusCode::OK)` に書き換え |
| `examples/http11_server_io_uring/src/main.rs` | 修正 | 同上 |
| `examples/http11_reverse_proxy/src/main.rs` | 修正 | 同上 |
| `examples/http11_client/src/main.rs` | 修正 | 同上 (受信 Response の構築箇所は `with_version` のままで良い、recv 側は raw 値が必要) |
| `tests/test_encoder.rs` | 修正 | 標準 status code の `.unwrap()` を `with_status` に書き換え |
| `tests/test_decoder.rs` | 修正 | 標準 status code の `.unwrap()` を `with_status` に書き換え |
| `pbt/tests/prop_encoder.rs` | 修正 | 同上 |
| `pbt/tests/prop_decoder/response.rs` | 修正 | 同上 |
| `fuzz/fuzz_targets/fuzz_encode_response.rs` | 確認 | カスタム値ベースなので `with_status` は使えない、変更なし |
| `fuzz/fuzz_targets/fuzz_decoder_chunked.rs` | 修正 | 標準 status code の `.unwrap()` を `with_status` に書き換え |
| `src/lib.rs` doctest | 修正 | doctest の `Response::new(200, "OK").unwrap()` を `Response::with_status(StatusCode::OK)` に書き換え |
| `skills/shiguredo-http11/SKILL.md` | 修正 | API 一覧に `StatusCode` / `with_status` を追加、サンプルコードを `with_status` に書き換え |

### src/status_code.rs (新規)

```rust
//! HTTP ステータスコード型
//!
//! RFC 9110 Section 15 で定義されたステータスコードと、それに対応する
//! IANA HTTP Status Code Registry の canonical reason phrase を const 値として保持する。

use core::num::NonZeroU16;

/// HTTP ステータスコード
///
/// `code` は RFC 9110 Section 15 の 100..=599 範囲内であることを保証する
/// (本型を経由した構築でのみ生成可能)。
/// `canonical_reason` は IANA HTTP Status Code Registry に登録された reason phrase。
///
/// 任意の status code を構築したい場合は `Response::new(code, reason)` を使うこと。
/// 本型は IANA 登録済み code 専用の const 表現を提供する。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct StatusCode {
    code: NonZeroU16,
    canonical_reason: &'static str,
}

impl StatusCode {
    // 1xx Informational (RFC 9110 Section 15.2)
    pub const CONTINUE: Self = Self::new_const(100, "Continue");
    pub const SWITCHING_PROTOCOLS: Self = Self::new_const(101, "Switching Protocols");
    pub const PROCESSING: Self = Self::new_const(102, "Processing");        // RFC 2518 (WebDAV)
    pub const EARLY_HINTS: Self = Self::new_const(103, "Early Hints");      // RFC 8297

    // 2xx Successful (RFC 9110 Section 15.3)
    pub const OK: Self = Self::new_const(200, "OK");
    pub const CREATED: Self = Self::new_const(201, "Created");
    pub const ACCEPTED: Self = Self::new_const(202, "Accepted");
    pub const NON_AUTHORITATIVE_INFORMATION: Self = Self::new_const(203, "Non-Authoritative Information");
    pub const NO_CONTENT: Self = Self::new_const(204, "No Content");
    pub const RESET_CONTENT: Self = Self::new_const(205, "Reset Content");
    pub const PARTIAL_CONTENT: Self = Self::new_const(206, "Partial Content");
    pub const MULTI_STATUS: Self = Self::new_const(207, "Multi-Status");    // RFC 4918 (WebDAV)
    pub const ALREADY_REPORTED: Self = Self::new_const(208, "Already Reported"); // RFC 5842
    pub const IM_USED: Self = Self::new_const(226, "IM Used");              // RFC 3229

    // 3xx Redirection (RFC 9110 Section 15.4)
    pub const MULTIPLE_CHOICES: Self = Self::new_const(300, "Multiple Choices");
    pub const MOVED_PERMANENTLY: Self = Self::new_const(301, "Moved Permanently");
    pub const FOUND: Self = Self::new_const(302, "Found");
    pub const SEE_OTHER: Self = Self::new_const(303, "See Other");
    pub const NOT_MODIFIED: Self = Self::new_const(304, "Not Modified");
    pub const USE_PROXY: Self = Self::new_const(305, "Use Proxy");
    pub const TEMPORARY_REDIRECT: Self = Self::new_const(307, "Temporary Redirect");
    pub const PERMANENT_REDIRECT: Self = Self::new_const(308, "Permanent Redirect"); // RFC 7538

    // 4xx Client Error (RFC 9110 Section 15.5)
    pub const BAD_REQUEST: Self = Self::new_const(400, "Bad Request");
    pub const UNAUTHORIZED: Self = Self::new_const(401, "Unauthorized");
    pub const PAYMENT_REQUIRED: Self = Self::new_const(402, "Payment Required");
    pub const FORBIDDEN: Self = Self::new_const(403, "Forbidden");
    pub const NOT_FOUND: Self = Self::new_const(404, "Not Found");
    pub const METHOD_NOT_ALLOWED: Self = Self::new_const(405, "Method Not Allowed");
    pub const NOT_ACCEPTABLE: Self = Self::new_const(406, "Not Acceptable");
    pub const PROXY_AUTHENTICATION_REQUIRED: Self = Self::new_const(407, "Proxy Authentication Required");
    pub const REQUEST_TIMEOUT: Self = Self::new_const(408, "Request Timeout");
    pub const CONFLICT: Self = Self::new_const(409, "Conflict");
    pub const GONE: Self = Self::new_const(410, "Gone");
    pub const LENGTH_REQUIRED: Self = Self::new_const(411, "Length Required");
    pub const PRECONDITION_FAILED: Self = Self::new_const(412, "Precondition Failed");
    pub const CONTENT_TOO_LARGE: Self = Self::new_const(413, "Content Too Large");
    pub const URI_TOO_LONG: Self = Self::new_const(414, "URI Too Long");
    pub const UNSUPPORTED_MEDIA_TYPE: Self = Self::new_const(415, "Unsupported Media Type");
    pub const RANGE_NOT_SATISFIABLE: Self = Self::new_const(416, "Range Not Satisfiable");
    pub const EXPECTATION_FAILED: Self = Self::new_const(417, "Expectation Failed");
    pub const IM_A_TEAPOT: Self = Self::new_const(418, "I'm a teapot");     // RFC 2324 / RFC 7168
    pub const MISDIRECTED_REQUEST: Self = Self::new_const(421, "Misdirected Request");
    pub const UNPROCESSABLE_CONTENT: Self = Self::new_const(422, "Unprocessable Content");
    pub const LOCKED: Self = Self::new_const(423, "Locked");                // RFC 4918 (WebDAV)
    pub const FAILED_DEPENDENCY: Self = Self::new_const(424, "Failed Dependency"); // RFC 4918
    pub const TOO_EARLY: Self = Self::new_const(425, "Too Early");          // RFC 8470
    pub const UPGRADE_REQUIRED: Self = Self::new_const(426, "Upgrade Required");
    pub const PRECONDITION_REQUIRED: Self = Self::new_const(428, "Precondition Required"); // RFC 6585
    pub const TOO_MANY_REQUESTS: Self = Self::new_const(429, "Too Many Requests"); // RFC 6585
    pub const REQUEST_HEADER_FIELDS_TOO_LARGE: Self = Self::new_const(431, "Request Header Fields Too Large"); // RFC 6585
    pub const UNAVAILABLE_FOR_LEGAL_REASONS: Self = Self::new_const(451, "Unavailable For Legal Reasons"); // RFC 7725

    // 5xx Server Error (RFC 9110 Section 15.6)
    pub const INTERNAL_SERVER_ERROR: Self = Self::new_const(500, "Internal Server Error");
    pub const NOT_IMPLEMENTED: Self = Self::new_const(501, "Not Implemented");
    pub const BAD_GATEWAY: Self = Self::new_const(502, "Bad Gateway");
    pub const SERVICE_UNAVAILABLE: Self = Self::new_const(503, "Service Unavailable");
    pub const GATEWAY_TIMEOUT: Self = Self::new_const(504, "Gateway Timeout");
    pub const HTTP_VERSION_NOT_SUPPORTED: Self = Self::new_const(505, "HTTP Version Not Supported");
    pub const VARIANT_ALSO_NEGOTIATES: Self = Self::new_const(506, "Variant Also Negotiates"); // RFC 2295
    pub const INSUFFICIENT_STORAGE: Self = Self::new_const(507, "Insufficient Storage"); // RFC 4918
    pub const LOOP_DETECTED: Self = Self::new_const(508, "Loop Detected");  // RFC 5842
    pub const NOT_EXTENDED: Self = Self::new_const(510, "Not Extended");    // RFC 2774 (廃止 RFC だが IANA 登録は残存)
    pub const NETWORK_AUTHENTICATION_REQUIRED: Self = Self::new_const(511, "Network Authentication Required"); // RFC 6585

    /// const コンテキスト用の内部コンストラクタ
    ///
    /// 100..=599 範囲外を渡すとコンパイルエラーになる (NonZeroU16 + assert で検証)。
    /// 公開 API ではないため呼び出し側は const 定数のみ生成する。
    const fn new_const(code: u16, canonical_reason: &'static str) -> Self {
        assert!(code >= 100 && code <= 599, "status code must be in 100..=599 (RFC 9110 Section 15)");
        // SAFETY: 上の assert で 100..=599 を保証しているため非ゼロ
        let code = match NonZeroU16::new(code) {
            Some(c) => c,
            None => panic!("unreachable: status code is non-zero"),
        };
        Self { code, canonical_reason }
    }

    /// ステータスコード値を取得
    pub const fn code(&self) -> u16 {
        self.code.get()
    }

    /// IANA 登録の canonical reason phrase を取得
    pub const fn canonical_reason(&self) -> &'static str {
        self.canonical_reason
    }
}
```

### src/response.rs

```rust
impl Response {
    /// IANA 登録済みの StatusCode から Response を作成 (HTTP/1.1)
    ///
    /// `StatusCode` は const 値で構成され、すべて構築時バリデーションを
    /// 通過することが静的に保証されているため、本コンストラクタは infallible。
    ///
    /// version は `"HTTP/1.1"` 固定。`reason_phrase` は StatusCode の
    /// `canonical_reason` を使用する。
    ///
    /// カスタムバージョン / カスタム reason が必要な場合は
    /// `Response::with_version` / `Response::new` を使うこと。
    pub fn with_status(status: StatusCode) -> Self {
        // 内部の `with_version` を呼び、すべての引数が valid と分かっているため
        // unwrap で確定的に Self を取り出す。
        // - version: "HTTP/1.1" (リテラル) は is_valid_protocol_version を通過する
        // - status_code: StatusCode の不変条件 (100..=599) は is_valid_status_code を通過する
        // - canonical_reason: IANA 登録の ASCII 文字列、is_valid_reason_phrase を通過する
        Self::with_version("HTTP/1.1", status.code(), status.canonical_reason())
            .expect("StatusCode constants are always valid by construction")
    }
}
```

### IANA registry のスコープ判断

- 本 issue で含めるのは IANA HTTP Status Code Registry の **常用ステータスコード**
  - RFC 9110 で定義されたコア (100/101, 200-206, 300-308, 400-417/421-426/428-431/451, 500-505)
  - WebDAV (RFC 4918): 102, 207, 422, 423, 424, 507
  - その他広く使われる拡張: 208, 226, 425, 428, 429, 431, 451, 506, 508, 510, 511, 418
- 含めない: 廃止予定の status code (例: 305 Use Proxy は RFC 9110 で deprecated だが利用例があるため含める)、私的拡張
- 任意の status code を使いたい場合は `Response::new(code, reason)` を使えば良いので、漏れがあっても致命的ではない

### Response::with_version との関係

`Response::with_version(version, code, reason)` は引き続き残す。RTSP / HTTP/1.0 等のカスタム version 用の escape hatch。

将来的に `with_version_status(version, StatusCode)` のような StatusCode + version カスタムの組み合わせも欲しくなる可能性があるが、本 issue ではスコープ外。必要になった時点で追加検討。

### 命名規則

- 型名: `StatusCode` (Rust API Guidelines C-CASE: 型は PascalCase)
- 定数名: `StatusCode::OK` 等 (SCREAMING_SNAKE_CASE)
- 命名は IANA HTTP Status Code Registry の名称を **そのまま機械的に SCREAMING_SNAKE_CASE 変換** する
  - `"Not Found"` → `NOT_FOUND`
  - `"I'm a teapot"` → `IM_A_TEAPOT` (アポストロフィを除去)
  - `"Unavailable For Legal Reasons"` → `UNAVAILABLE_FOR_LEGAL_REASONS`

### Method 型との並行構造

本 issue 完了後、`0025` (Request 非公開化) を経て、`Method` 型 + `Request::with_method` の追加 issue を別途切る。両者は同じパターンで:

| Response | Request |
|---|---|
| `StatusCode` 型 (本 issue) | `Method` 型 (将来 issue) |
| `Response::with_status(StatusCode::OK)` | `Request::with_method(Method::GET, uri)?` |
| infallible (全 const 値) | fallible (uri がフリー文字列のため) |

## CHANGES.md

`## develop` セクションに以下を追加する:

```
- [ADD] `StatusCode` 型を導入し IANA HTTP Status Code Registry の status code を const 値として提供する
  - `Response::with_status(StatusCode::OK)` 等で infallible に Response を構築できる
  - `StatusCode::code()` / `StatusCode::canonical_reason()` でアクセス
  - @voluntas
```

## 検証方針

### 単体テスト (`tests/test_response.rs`)

- 各 IANA 登録 StatusCode に対し、`Response::with_status(StatusCode::OK)` 等で構築 → `status_code()` が code 値、`reason_phrase()` が canonical reason、`version()` が `"HTTP/1.1"` を返すことを確認
- `with_status` で構築した Response は `validate_response_fields` (encoder 二重バリデーション) を通過することを確認 (encode 成功)

### PBT (`pbt/tests/prop_response.rs`)

- `prop::sample::select(&[StatusCode::OK, StatusCode::NOT_FOUND, ...])` 戦略で任意の StatusCode 定数を選び、`with_status` で構築した Response がアクセサで code/reason を保存していることを検証
- `Response::with_status(...)` → encode → decode のラウンドトリップで code/reason が保存されることを検証 (encoder/decoder と StatusCode の整合性)

### `assert!` パニックの const 評価検証

- `StatusCode::new_const(50, "...")` のような不正値が **コンパイル時に panic する** ことを確認 (compile_fail テストまたは手動検証)
  - rustc の const evaluator は assert! を const context で評価するため、新しい定数を追加した際の typo (例: `StatusCode::new_const(50, ...)`) はコンパイルエラーになる

## 受け入れ基準

- ブランチ名が `feature/add-status-code-type` であること
- `make fmt && make clippy && make check && make test` がすべて成功する
- `src/status_code.rs` が新規作成されており、IANA HTTP Status Code Registry に登録された主要 status code が `StatusCode` 定数として定義されている
- `Response::with_status(StatusCode) -> Self` が追加されており、infallible (Result を返さない) である
- examples / tests / PBT で `Response::new(200, "OK").unwrap()` 等の呼び出しが `Response::with_status(StatusCode::OK)` に書き換えられている
- `.unwrap()` が examples からほぼ消えていること (動的入力ベースの `add_header(...)?` などは残るが、構築時の `.unwrap()` は消える)
- `StatusCode::code()` / `StatusCode::canonical_reason()` の単体テストが存在する
- `Response::with_status(StatusCode::OK)` → encode → decode のラウンドトリップ PBT が存在する
- `skills/shiguredo-http11/SKILL.md` の API 一覧およびサンプルコードが更新されている
- `Response::new` / `Response::with_version` は変更されていない (後方互換維持)
