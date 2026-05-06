# 0024: StatusCode 型を導入し Response::with_status を追加する

Created: 2026-05-06
Completed: 2026-05-06
Model: Opus 4.7

## 概要

IANA HTTP Status Code Registry に登録された status code を const 値として持つ `StatusCode` 型を新設し、`Response::with_status(StatusCode) -> Self` を追加する。`with_status` は引数が完全に const 値で構成されるため infallible (`Result` を返さない)。

**現状**: `src/status_code.rs` は既に実装済み（const 定義、`new_const`、`code()` / `canonical_reason()` アクセサ）。未完了の作業は以下の通り:
1. `src/lib.rs` に `mod status_code;` と `pub use` を追加する
2. `src/response.rs` に `import` を追加し `with_status` メソッドを実装する
3. tests / examples / PBT / fuzz / SKILL.md の `.unwrap()` 呼び出しを書き換える

純粋追加。既存 API (`Response::new` / `Response::with_version`) は触らない。後方互換あり。

ブランチ名は AGENTS.md「git ブランチの命名規則」に従い `feature/add-status-code-type` を使用する。

依存関係:
- `0017` (Response フィールド非公開化) は完了済み。`Response::new` が `Result<Self, EncodeError>` を返すため、examples / tests / PBT / fuzz では `Response::new(200, "OK").unwrap()` の `.unwrap()` が大量発生している。本 issue でこれらを `Response::with_status(StatusCode::OK)` に書き換え、構築時の `.unwrap()` を一掃する。
- `0020` (StatusClass enum) との関係: `StatusClass` は status_code の **範囲分類** (1xx/2xx/3xx/4xx/5xx)、`StatusCode` は **個別 status code とその canonical reason の値** という別概念。両者は独立しており、`StatusCode::OK.code()` を `StatusClass::from_status_code(...)` に渡す形で連携する。
  - 注意: 0020 も `src/response.rs` を編集する（`is_informational` 等 5 メソッドの撤去）。本 issue が先にマージされた場合、0020 の差分適用時に競合が発生する。競合はマージの際に手動解決する前提で、本 issue では 0020 未完了のコードを前提に編集する。
- 本 issue 完了後に `Method` 型 + `Request::with_method` の追加 issue を別途切る。これは 0025 とは別 issue となる。

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
| `src/status_code.rs` | **確認** | 既存ファイル。`from_code(u16) -> Option<Self>` を追加する。const 定義の参照 RFC 番号を最新化する (102: RFC 2518→RFC 4918、308: RFC 7538→RFC 9110、422: WebDAV→RFC 9110 core)。doc comment を整備する。 |
| `src/lib.rs` | 修正 | `mod status_code;` を追加し `pub use` ブロックに `StatusCode` を追加。`pub mod status_code;` として公開する。 |
| `src/response.rs` | 修正 | `use crate::StatusCode;` を追加し `pub fn with_status(status: StatusCode) -> Self` を追加 |
| `tests/test_status_code.rs` | **新規作成** | `StatusCode::code()` / `StatusCode::canonical_reason()` / `StatusCode::from_code()` の単体テスト。全 const 定義の網羅確認は PBT で行うため、単体テストでは 5 件程度の代表値 + `from_code` の既知/未知コードの分岐をテストする。 |
| `tests/test_response.rs` | 修正 | `with_status` の結合テストを追加 (with_status からエンコードまで)。`StatusCode` 定数を用いた構築テストは `test_status_code.rs` に分離する。 |
| `pbt/tests/prop_response.rs` | 修正 | `StatusCode` 全 const 定数の `prop::sample::select` 戦略で `with_status` の PBT を追加。動的生成の `status`/`phrase` を使う既存テストは `Response::new` のまま変更しない。 |
| `examples/http11_server/src/main.rs` | 修正 | `Response::new(status_code, reason_phrase)?` のうち固定リテラル箇所を `with_status` に置換。動的パラメータの `build_compressed_response` 関数は `status_code: u16` / `reason_phrase: &str` パラメータを持つため変更しない。 |
| `examples/http11_server_io_uring/src/main.rs` | 修正 | 同上 |
| `examples/http11_reverse_proxy/src/main.rs` | 修正 | `Response::new(resp_head.status_code, &resp_head.reason_phrase)?` はデコーダーからの動的値のため `with_status` に置換不可。変更なし。 |
| `examples/http11_client/src/main.rs` | 修正 | `Response::with_version(&h.version, h.status_code, &h.reason_phrase)?` はデコーダーからの受信 Response 構築のため置換不要。変更なし。 |
| `tests/test_encoder.rs` | 修正 | `Response::new(200, "OK").unwrap()` 等の固定リテラル呼び出しを `Response::with_status(StatusCode::OK)` に置換。動的生成値の `Response::new` はそのまま。 |
| `tests/test_decoder.rs` | **修正不要** | ファイル内に `Response::new` 呼び出しが存在しない。影響範囲から外す。 |
| `pbt/tests/prop_encoder.rs` | 修正 | 固定リテラル `(200, "OK")` の `Response::new` 呼び出しのみ `with_status` に置換。動的生成値の `status`/`phrase` を使うテストは変更しない。 |
| `pbt/tests/prop_decoder/response.rs` | 修正 | 同上 (動的 `status`/`reason`/`phrase` 戦略のテストはそのまま、固定リテラルのみ置換) |
| `fuzz/fuzz_targets/fuzz_encode_response.rs` | **確認のみ** | カスタム値ベースのため変更なし。影響範囲表に記載（存在確認のため）。 |
| `fuzz/fuzz_targets/fuzz_decoder_roundtrip.rs` | **確認のみ** | fuzz 入力由来の動的値 (`fuzz_resp.status_code`) のため変更なし。影響範囲表に追記。 |
| `fuzz/fuzz_targets/fuzz_decoder_chunked.rs` | 修正 | `Response::new(200, "OK").unwrap()` を `with_status` に置換 |
| `src/encoder.rs` (#[cfg(test)] mod capacity_tests) | 修正 | `Response::new(200, "OK").unwrap()` を `with_status` に置換 |
| `src/lib.rs` doctest | 修正 | doctest の `Response::new(200, "OK").unwrap()` を `Response::with_status(StatusCode::OK)` に書き換え |
| `skills/shiguredo-http11/SKILL.md` | 修正 | API 一覧に `StatusCode` / `with_status` を追加、サンプルコードを `with_status` に書き換え |

### src/status_code.rs (既存ファイル、要確認)

`src/status_code.rs` は既に存在している。issue 提出時のコードと実ファイルに差分があるため、実装時は **実ファイル側を正とする**。issue 本文のコード例は設計意図を示す参考として扱う。

```rust
//! HTTP ステータスコード型
//!
//! IANA HTTP Status Code Registry に登録されたステータスコードと、
//! それに対応する reason phrase を const 値として保持する。
//! RFC 9110 Section 15 で定義されたコードに加え、RFC 4918 (WebDAV)、
//! RFC 6585、RFC 7725 等の拡張コードも含む。
//!
//! 任意の status code (拡張、私的コード等) を使いたい場合は
//! `Response::new(code, reason)` 等を使用すること。
//! 本型は IANA 登録済み code 専用の const 表現を提供する。
//!
//! 注: RFC 9110 が将来的に status code の範囲を改訂する可能性がある。

use core::num::NonZeroU16;

/// HTTP ステータスコード
///
/// `code` は RFC 9110 Section 15 の 100..=599 範囲内であることを保証する
/// (本型を経由した構築でのみ生成可能)。
/// `canonical_reason` は IANA HTTP Status Code Registry に登録された reason phrase。
/// この値は HTTP の文脈での reason phrase であり、RTSP 等のプロトコルでは
/// 異なる reason phrase を持つ可能性がある。
///
/// `NonZeroU16` でラップしているのはニッチ最適化のため
/// (`Option<StatusCode>` 等が同じサイズで表現可能)。`code` が 0 になることは
/// `new_const` の assert! で静的に弾かれるため、不変条件は破られない。
///
/// 後方互換性のため `#[non_exhaustive]` は付与しない。
/// StatusCode は const 値の集合であり、フィールド追加のユースケースが
/// 想定されないため。仮にフィールド追加が必要になった場合は
/// StatusCodeV2 等の別型で対応する。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct StatusCode {
    code: NonZeroU16,
    canonical_reason: &'static str,
}

impl StatusCode {
    // 各 const 定義には doc comment を付与し、参照 RFC と節番号を明記する。
    // 例:
    // /// `200 OK` (RFC 9110 Section 15.3.1)
    // pub const OK: Self = Self::new_const(200, "OK");

    // 1xx Informational (RFC 9110 Section 15.2)
    // ... (全 const 定義。参照 RFC 番号は最新のものを使用する:
    //   102 → RFC 4918 Section 11.1、308 → RFC 9110 Section 15.4.9、
    //   422 → RFC 9110 Section 15.5.21)

    // 注: 418 (I'm a teapot) は RFC 9110 Section 15.5.19 で "Unused" と
    // 定義されているが、相互運用性の観点から RFC 2324/7168 の記述を採用する。

    /// const コンテキスト用の内部コンストラクタ
    ///
    /// 100..=599 範囲外を渡すと const 評価で panic する (定数定義時に
    /// コンパイルエラーになる) ため、新しい定数を追加した際の typo は
    /// 静的に検出できる。
    ///
    /// `canonical_reason` は非空文字列を期待する (呼び出し側の責務)。
    /// 空文字列を渡しても const 評価では検出されないため、定数追加時の
    /// レビューで担保する。
    const fn new_const(code: u16, canonical_reason: &'static str) -> Self {
        assert!(
            code >= 100 && code <= 599,
            "status code must be in 100..=599 (RFC 9110 Section 15)"
        );
        // 上の assert で 100..=599 を保証済みのため非ゼロ。
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

    /// IANA 登録の reason phrase を取得
    ///
    /// 注: この値は HTTP の文脈での reason phrase である。
    /// RTSP 等のプロトコルでは異なる reason phrase を持つ可能性があるため、
    /// クロスプロトコル利用時は注意すること。
    pub const fn canonical_reason(&self) -> &'static str {
        self.canonical_reason
    }
}

// `u16` からの逆引きは `from_code` で提供する。
// デコーダーからの `status_code()` の戻り値 (`u16`) を
// `StatusCode` に変換するユースケースに対応する。
impl StatusCode {
    /// ステータスコード値から `StatusCode` を取得
    ///
    /// IANA 登録外のコードは `None` を返す。
    /// 任意のコードに対しては `Response::new(code, reason)` を使うこと。
    pub fn from_code(code: u16) -> Option<Self> {
        // match で全 const 定義を網羅
        Some(match code {
            100 => Self::CONTINUE,
            101 => Self::SWITCHING_PROTOCOLS,
            // ... 全 const 定義
            _ => return None,
        })
    }
}
```

`PartialOrd` / `Ord` は derive しない。status code の数値による全順序は存在するが、StatusCode は意味的な型であり数値での比較は `code()` で取り出して行うことを推奨するため。`Display` も現時点では実装しない（`canonical_reason()` で取り出せるため）。

### src/response.rs

`use crate::StatusCode;` を追加したうえで、以下のメソッドを追加する:

```rust
impl Response {
    /// IANA 登録済みの StatusCode から Response を作成 (HTTP/1.1)
    ///
    /// `StatusCode` は const 値で構成され、すべて構築時バリデーションを
    /// 通過することが静的に保証されているため、本コンストラクタは infallible。
    ///
    /// version は `"HTTP/1.1"` 固定。`reason_phrase` は StatusCode の
    /// `canonical_reason` を使用する。
    /// body / headers / omit_body は初期状態 (`None`, `Vec::new()`, `false`)。
    /// これらの振る舞いは `Response::new` と同等。
    ///
    /// RTSP/1.0 等のカスタムバージョンが必要な場合は `Response::with_version` を
    /// 使うこと。`StatusCode::code()` / `StatusCode::canonical_reason()` を
    /// 引数に渡せば同等の結果を得られる。
    ///
    /// カスタム reason が必要な場合も `Response::new` / `Response::with_version` を
    /// 使うこと。
    pub fn with_status(status: StatusCode) -> Self {
        Self::with_version("HTTP/1.1", status.code(), status.canonical_reason())
            .expect("StatusCode constants are always valid by construction")
    }
}
```

### IANA registry のスコープ判断

本 issue で含める status code の選択基準:
- **含める**: IANA HTTP Status Code Registry に永続登録され、RFC 9110 Section 15 またはそれを改訂した最新 RFC で定義されている status code
  - RFC 9110 Section 15 で定義されたコアコード (100-101、200-206、300-305、307-308、400-417、421-422、426、500-505)
  - RFC 9110 に取り込まれた外部仕様由来のコード (422、425、428-429、431、451 等)
  - WebDAV (RFC 4918): 102、207、423、424、507
  - その他広く使われる拡張: 208、226、506、508、510、511、418
- **含めない**: 一時登録 (TEMPORARY) の status code (例: 104 Upload Resumption Supported)、私的拡張、IANA 未登録のコード
- 305 (Use Proxy) は RFC 9110 Section 15.4.6 で deprecated とされているが、IANA に永続登録されており利用例もあるため含める。deprecated であることはコードコメントに明記する。
- 418 は RFC 9110 Section 15.5.19 で "Unused" と定義されているが、RFC 2324/7168 の "I'm a teapot" が広く認知されているため、本実装では後者を採用する。この判断は RFC 9110 Section 1.2 の「HTTP の目的は相互運用性」に基づく。
- 任意の status code を使いたい場合は `Response::new(code, reason)` を使えば良いので、漏れがあっても致命的ではない。

### Response::with_version との関係

`Response::with_version(version, code, reason)` は引き続き残す。RTSP / HTTP/1.0 等のカスタム version 用の escape hatch。

将来的に `with_version_status(version, StatusCode)` のような StatusCode + version カスタムの組み合わせも欲しくなる可能性があるが、本 issue ではスコープ外。必要になった時点で追加検討。

### 命名規則

- 型名: `StatusCode` (Rust API Guidelines C-CASE: 型は PascalCase)
- 定数名: `StatusCode::OK` 等 (SCREAMING_SNAKE_CASE)
- 命名は IANA HTTP Status Code Registry の名称を **そのまま機械的に SCREAMING_SNAKE_CASE 変換** する
  - `"Not Found"` → `NOT_FOUND`
  - `"I'm a teapot"` → `IM_A_TEAPOT` (アポストロフィは Rust 識別子に使用できないため除去)
  - `"Unavailable For Legal Reasons"` → `UNAVAILABLE_FOR_LEGAL_REASONS`
  - ハイフン、ピリオド等、Rust 識別子で合法な文字はそのまま残す

### RFC 9112 Section 4 (Status Line) 参照

`StatusCode` 型の設計背景として、以下を踏まえる:

- reason-phrase は status-line ABNF で OPTIONAL (RFC 9112 Section 4): サーバは reason-phrase が absent でも status-code の後ろの SP を送信しなければならない (MUST)
- RFC 9110 Section 15.1: reason phrase は推奨に過ぎず、ローカライズや除去が可能
- RFC 9112 Section 4: クライアントは reason-phrase の内容を SHOULD ignore

本型の `canonical_reason` という命名は「IANA 登録値」を意味する実装上の命名であり、RFC 上の正規性 (canonicality) を主張するものではない。

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
  - `StatusCode::code()` / `StatusCode::canonical_reason()` / `StatusCode::from_code()` でアクセス
  - @voluntas
```

## 検証方針

### 単体テスト (`tests/test_status_code.rs` — 新規作成)

- 代表的な StatusCode 定数 (OK、NOT_FOUND、INTERNAL_SERVER_ERROR の 3 件) に対し、`code()` が正しい値、`canonical_reason()` が正しい文字列を返すことを確認
- `from_code(200)` → `Some(StatusCode::OK)`、`from_code(999)` → `None` の逆引きテスト
- `from_code` で全登録コードが取得可能なことを確認
- 全 const 定義の canonical_reason が空でないことの確認は単体テストで行う (PBT の select 戦略より単体テストの assert! ループの方が保守性が高い)

### 単体テスト (`tests/test_response.rs`)

- `with_status(StatusCode::OK)` で構築した Response が `new(200, "OK").unwrap()` と同一のフィールドを持つことを確認
- `with_status` で構築した Response は `encode()` が成功することを確認

### PBT (`pbt/tests/prop_response.rs`)

- `prop::sample::select(&[StatusCode::OK, StatusCode::NOT_FOUND, ...])` 戦略で任意の StatusCode 定数を選び、`with_status` で構築した Response がアクセサで code/reason を保存していることを検証
- `Response::with_status(...)` → encode → decode のラウンドトリップで code/reason が保存されることを検証

### const 評価の検証

- `StatusCode::new_const(50, "...")` のような不正値が **コンパイル時に panic する** ことは、以下の理由により自動検証されるため専用テストは不要:
  - `new_const` は `pub` でないため外部からのコンパイルテストは不可能
  - すべての const 定数定義がコンパイルを通ること自体が `assert!` の正しさを証明する
  - 新しい定数追加時の typo は rustc の const evaluator が検出する

### `Display` / `PartialOrd` / `Ord` について

- `Display`: 現時点では実装しない。canonical reason の出力が必要なら `canonical_reason()` を使う。0020 の `StatusClass` も同様の方針。
- `PartialOrd` / `Ord`: 現時点では実装しない。数値比較は `code()` で取り出して行うことを推奨する。

## 受け入れ基準

- ブランチ名が `feature/add-status-code-type` であること
- `make fmt && make clippy && make check && make test` がすべて成功する
- `src/lib.rs` に `pub mod status_code;` および `pub use` で `StatusCode` が追加されている
- `src/response.rs` に `use crate::StatusCode;` と `pub fn with_status(status: StatusCode) -> Self` が追加されており、infallible (Result を返さない) である
- `src/status_code.rs` に `pub fn from_code(code: u16) -> Option<Self>` が追加されている
- `tests/test_status_code.rs` が新規作成されており、code/reason/canonical_reason の単体テストが存在する
- examples / tests / PBT / fuzz で `Response::new(200, "OK").unwrap()` 等の固定リテラル呼び出しが `Response::with_status(StatusCode::OK)` に書き換えられている
- 動的生成値を使う箇所 (PBT strategy、`build_compressed_response` パラメータ、デコーダーからの受信値) は `Response::new` のまま変更されていない
- `Response::with_status(StatusCode::OK)` の呼び出しに `.unwrap()` が付いていないこと
- `Response::with_status(StatusCode::OK)` → encode → decode のラウンドトリップ PBT が存在する
- `StatusCode::from_code(200)` → `Some(StatusCode::OK)`、`StatusCode::from_code(999)` → `None` の単体テストが存在する
- `skills/shiguredo-http11/SKILL.md` の API 一覧およびサンプルコードが更新されている
- `Response::new` / `Response::with_version` は変更されていない (後方互換維持)
